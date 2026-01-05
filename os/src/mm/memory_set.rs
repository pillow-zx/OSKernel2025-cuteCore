//! 内核虚拟内存管理模块。
//!
//! 本模块封装了内核的虚拟地址空间管理功能，
//! 包括：
//! - 内核空间映射（Kernel Space）
//! - 用户空间映射（User Space）
//! - ELF 可执行文件加载映射
//! - 页帧分配与映射管理
//!
//! 核心概念：
//! - `MemorySet`：表示一组连续的虚拟地址区域及对应映射
//! - `MapArea`：表示一段连续虚拟页范围和映射类型
//! - `PageTable`：页表抽象，实际实现由 `PageTableImpl` 提供
//! - `MapType`：映射类型（Identical / Framed / Linear）
//! - `MapPermission`：映射权限（R/W/X/U）
//!
//! # Safety / Invariants
//! - 内核空间 `KERNEL_SPACE` 只初始化一次
//! - 所有映射、解除映射操作需保证单核独占访问（使用 UPIntrFreeCell）
//! - ELF 加载区域假设合法且与用户栈、trap_context 不冲突
//! - Framed 类型映射的页帧在 `MapArea` 内部追踪，确保不会泄漏

use crate::hal::{PageTableEntryImpl, PageTableImpl, MEMORY_END, MMIO, PAGE_SIZE, TRAMPOLINE, UserStackBase, TRAP_CONTEXT_BASE};
use crate::mm::address::{VPNRange,align_up};
use crate::mm::{
    frame_alloc, FrameTracker, PageTable, PhysAddr, PhysPageNum, StepByOne, VirtAddr, VirtPageNum,
};
use crate::sync::UPIntrFreeCell;
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;
use bitflags::bitflags;
use lazy_static::lazy_static;
use log::info;

// 内核段符号，由链接脚本提供
extern "C" {
    fn stext();
    fn etext();
    fn srodata();
    fn erodata();
    fn sdata();
    fn edata();
    fn sbss_with_stack();
    fn ebss();
    fn ekernel();
    fn strampoline();
}

lazy_static! {
    /// 全局内核地址空间
    ///
    /// 使用 `Arc<UPIntrFreeCell<MemorySet<PageTableImpl>>>` 封装，
    /// 确保在单核下独占访问。
    ///
    /// INVARIANT:
    /// - 内核空间只初始化一次
    /// - 页面映射范围不会重复
    /// - 所有内核态映射都在此 MemorySet 管理
    pub static ref KERNEL_SPACE: Arc<UPIntrFreeCell<MemorySet<PageTableImpl>>> =
        Arc::new(unsafe { UPIntrFreeCell::new(MemorySet::new_kernel()) });
}

/// 获取内核页表 token
pub fn kernel_token() -> usize {
    KERNEL_SPACE.exclusive_access().token()
}

/// 表示一组虚拟地址映射集合
pub struct MemorySet<T: PageTable> {
    /// 页表实例
    page_table: T,
    /// 管理的 MapArea 列表
    areas: Vec<MapArea>,
    /// 堆顶地址
    pub brk: usize,
    /// 堆起始地址
    pub heap_start: usize,
}

impl<T: PageTable> MemorySet<T> {
    /// 创建一个空 MemorySet，不包含任何区域
    pub fn new_bare() -> Self {
        Self {
            page_table: T::new_kernel(),
            areas: Vec::new(),
            brk:0,
            heap_start:0,
        }
    }

    /// 获取页表 token
    pub fn token(&self) -> usize {
        self.page_table.token()
    }

    /// 为 MemorySet 插入一段新映射区（Framed 类型）
    /// 假设无地址冲突
    pub fn insert_framed_area(
        &mut self,
        start_va: VirtAddr,
        end_va: VirtAddr,
        permission: MapPermission,
    ) {
        self.push(
            MapArea::new(start_va, end_va, MapType::Framed, permission),
            None,
        );
    }

    /// 移除以指定起始虚拟页号为起点的区域
    pub fn remove_area_with_start_vpn(&mut self, start_vpn: VirtPageNum) {
        if let Some((idx, area)) = self
            .areas
            .iter_mut()
            .enumerate()
            .find(|(_, area)| area.vpn_range.get_start() == start_vpn)
        {
            area.unmap(&mut self.page_table);
            self.areas.remove(idx);
        }
    }

    /// 将 MapArea 插入 MemorySet，并可附加数据写入页帧
    pub fn push(&mut self, mut map_area: MapArea, data: Option<&[u8]>) {
        map_area.map(&mut self.page_table);
        if let Some(data) = data {
            map_area.copy_data(&self.page_table, data);
        }
        self.areas.push(map_area);
    }
    /// 映射 trampoline，不归 areas 管理
    fn map_trampoline(&mut self) {
        self.page_table.map(
            VirtAddr::from(TRAMPOLINE).into(),
            PhysAddr::from(strampoline as *const () as usize).into(),
            // PTEFlags::R | PTEFlags::X,
            MapPermission::R | MapPermission::X,
        );
    }
    /// 扩展堆区到 new_brk
    pub fn expand_heap(&mut self, new_brk: usize) -> Result<(), ()> {
        let old_brk = self.brk;

        let old_page = align_up(old_brk, PAGE_SIZE);
        let new_page = align_up(new_brk, PAGE_SIZE);

        if new_page > old_page {
            self.insert_framed_area(
                old_page.into(),
                new_page.into(),
                MapPermission::R | MapPermission::W | MapPermission::U,
            );
        }
        Ok(())
    }

    /// 构建内核空间 MemorySet，不包含内核栈
    pub fn new_kernel() -> Self {
        let mut memory_set = Self::new_bare();

        // 映射跳板
        memory_set.map_trampoline();

        // 映射内核段
        memory_set.push(
            MapArea::new(
                (stext as usize).into(),
                (etext as usize).into(),
                MapType::Identical,
                MapPermission::R | MapPermission::X,
            ),
            None,
        );

        memory_set.push(
            MapArea::new(
                (srodata as usize).into(),
                (erodata as usize).into(),
                MapType::Identical,
                MapPermission::R,
            ),
            None,
        );

        memory_set.push(
            MapArea::new(
                (sdata as usize).into(),
                (edata as usize).into(),
                MapType::Identical,
                MapPermission::R | MapPermission::W,
            ),
            None,
        );

        memory_set.push(
            MapArea::new(
                (sbss_with_stack as usize).into(),
                (ebss as usize).into(),
                MapType::Identical,
                MapPermission::R | MapPermission::W,
            ),
            None,
        );

        // 映射物理内存剩余空间
        memory_set.push(
            MapArea::new(
                (ekernel as usize).into(),
                MEMORY_END.into(),
                MapType::Identical,
                MapPermission::R | MapPermission::W,
            ),
            None,
        );

        // 映射 MMIO 外设
        for pair in MMIO {
            memory_set.push(
                MapArea::new(
                    (*pair).0.into(),
                    ((*pair).0 + (*pair).1).into(),
                    MapType::Identical,
                    MapPermission::R | MapPermission::W,
                ),
                None,
            );
        }
        memory_set
    }

    /// 从 ELF 数据构建用户空间 MemorySet
    /// 返回 (MemorySet, user_stack_base, entry_point)
    pub fn from_elf(elf_data: &[u8]) -> (Self,  usize) {
        let mut memory_set = Self::new_bare();
        // map trampoline
        memory_set.map_trampoline();
        // map program headers of elf, with U flag
        let elf = xmas_elf::ElfFile::new(elf_data).unwrap();
        let elf_header = elf.header;
        let magic = elf_header.pt1.magic;
        assert_eq!(magic, [0x7f, 0x45, 0x4c, 0x46], "invalid elf!");
        let ph_count = elf_header.pt2.ph_count();
        let mut max_end_vpn = VirtPageNum(0);
        // 映射每一个段
        for i in 0..ph_count {
            let ph = elf.program_header(i).unwrap();
            if ph.get_type().unwrap() == xmas_elf::program::Type::Load {
                let start_va: VirtAddr = (ph.virtual_addr() as usize).into();
                let end_va: VirtAddr = ((ph.virtual_addr() + ph.mem_size()) as usize).into();
                let mut map_perm = MapPermission::U;
                let ph_flags = ph.flags();
                if ph_flags.is_read() {
                    map_perm |= MapPermission::R;
                }
                if ph_flags.is_write() {
                    map_perm |= MapPermission::W;
                }
                if ph_flags.is_execute() {
                    map_perm |= MapPermission::X;
                }
                let map_area = MapArea::new(start_va, end_va, MapType::Framed, map_perm);
                // 选择最大的作为结束虚拟页号
                max_end_vpn = max_end_vpn.max(map_area.vpn_range.get_end());
                // 插入映射，并拷贝数据，初始化数据区为 0
                memory_set.push(
                    map_area,
                    Some(&elf.input[ph.offset() as usize..(ph.offset() + ph.file_size()) as usize]),
                );
            }
        }
        let max_end_va: VirtAddr = max_end_vpn.into();
        let heap_start = align_up(max_end_va.into(), PAGE_SIZE);

        info!("heap_start:  {:#x}\n", heap_start);
        memory_set.heap_start = heap_start;
        memory_set.brk = heap_start;
        let mut user_stack_base: usize = UserStackBase;
        user_stack_base += PAGE_SIZE;

        //用户栈顶的位置为 TRAP_CONTEXT_BASE
        let user_stack_top = TRAP_CONTEXT_BASE;
        (
            memory_set,
            elf.header.pt2.entry_point() as usize,
        )
    }

    /// 从已存在的用户空间 MemorySet 克隆新的 MemorySet
    pub fn from_existed_user(user_space: &MemorySet<T>) -> MemorySet<T> {
        let mut memory_set = Self::new_bare();
        // 映射跳板
        memory_set.map_trampoline();

        // 复制用户空间的每个映射区域
        for area in user_space.areas.iter() {
            let new_area = MapArea::from_another(area);
            memory_set.push(new_area, None);

            // 复制用户数据页内容
            for vpn in area.vpn_range {
                let src_ppn = user_space.translate(vpn).unwrap().ppn();
                let dst_ppn = memory_set.translate(vpn).unwrap().ppn();
                dst_ppn
                    .get_bytes_array()
                    .copy_from_slice(src_ppn.get_bytes_array());
            }
        }
        memory_set
    }

    /// 激活页表
    pub fn activate(&self) {
        self.page_table.activate();
    }

    /// 虚拟页号到页表项翻译
    pub fn translate(&self, vpn: VirtPageNum) -> Option<PageTableEntryImpl> {
        self.page_table.translate(vpn)
    }

    /// 回收数据页（清空 areas）
    pub fn recycle_data_pages(&mut self) {
        //*self = Self::new_bare();
        self.areas.clear();
    }
}

/// 表示连续虚拟页范围的映射区域
///
/// `vpn_range`：虚拟页号范围
///
/// `data_frames`：数据页帧追踪表（仅 Framed 类型使用）
///
/// `map_type`：映射类型
///
/// `map_perm`：映射权限
pub struct MapArea {
    /// 虚拟页号范围
    vpn_range: VPNRange,
    /// 数据页帧追踪表（仅 Framed 类型使用）
    ///
    /// 键：虚拟页号
    /// 值：对应的物理页帧追踪器
    data_frames: BTreeMap<VirtPageNum, FrameTracker>,
    /// 映射类型
    ///
    /// `Identical`：虚拟页号与物理页号相同映射
    /// `Framed`：为每个虚拟页分配独立物理页帧
    /// `Linear(offset)`：线性映射，物理页号 = 虚拟页号 + offset
    map_type: MapType,
    /// 映射权限
    ///
    /// `MapPermission` 位标志，表示读(R)/写(W)/执行(X)/用户权限(U)
    map_perm: MapPermission,
}

impl MapArea {
    /// 构建 MapArea，帧未分配
    pub fn new(
        start_va: VirtAddr,
        end_va: VirtAddr,
        map_type: MapType,
        map_perm: MapPermission,
    ) -> Self {
        let start_vpn: VirtPageNum = start_va.floor();
        let end_vpn: VirtPageNum = end_va.ceil();
        Self {
            vpn_range: VPNRange::new(start_vpn, end_vpn),
            data_frames: BTreeMap::new(),
            map_type,
            map_perm,
        }
    }

    /// 克隆 MapArea，不克隆帧内容
    pub fn from_another(another: &MapArea) -> Self {
        Self {
            vpn_range: VPNRange::new(another.vpn_range.get_start(), another.vpn_range.get_end()),
            data_frames: BTreeMap::new(),
            map_type: another.map_type,
            map_perm: another.map_perm,
        }
    }

    /// 映射单个虚拟页
    ///
    /// 自动处理不同映射类型
    pub fn map_one<T: PageTable>(&mut self, page_table: &mut T, vpn: VirtPageNum) {
        let ppn: PhysPageNum;
        match self.map_type {
            MapType::Identical => {
                ppn = PhysPageNum(vpn.0);
            }
            MapType::Framed => {
                let frame = frame_alloc().unwrap();
                ppn = frame.ppn;
                self.data_frames.insert(vpn, frame);
            }
            MapType::Linear(pn_offset) => {
                // check for sv39
                assert!(vpn.0 < (1usize << 27));
                ppn = PhysPageNum((vpn.0 as isize + pn_offset) as usize);
            }
        }
        let pte_flags = MapPermission::from_bits(self.map_perm.bits()).unwrap();
        page_table.map(vpn, ppn, pte_flags);
    }

    /// 解除单页映射
    pub fn unmap_one<T: PageTable>(&mut self, page_table: &mut T, vpn: VirtPageNum) {
        if self.map_type == MapType::Framed {
            self.data_frames.remove(&vpn);
        }
        page_table.unmap(vpn);
    }

    /// 映射整个 MapArea
    pub fn map<T: PageTable>(&mut self, page_table: &mut T) {
        for vpn in self.vpn_range {
            self.map_one(page_table, vpn);
        }
    }

    /// 解除整个 MapArea 映射
    pub fn unmap<T: PageTable>(&mut self, page_table: &mut T) {
        for vpn in self.vpn_range {
            self.unmap_one(page_table, vpn);
        }
    }

    /// 将数据拷贝到映射的页帧
    ///
    /// 假设所有帧已清零
    pub fn copy_data<T: PageTable>(&mut self, page_table: &T, data: &[u8]) {
        assert_eq!(self.map_type, MapType::Framed);
        let mut start: usize = 0;
        let mut current_vpn = self.vpn_range.get_start();
        let len = data.len();
        loop {
            let src = &data[start..len.min(start + PAGE_SIZE)];
            let dst = &mut page_table
                .translate(current_vpn)
                .unwrap()
                .ppn()
                .get_bytes_array()[..src.len()];
            dst.copy_from_slice(src);
            start += PAGE_SIZE;
            if start >= len {
                break;
            }
            current_vpn.step();
        }
    }
}

/// 页映射类型
///
/// `Identical`：虚拟页号与物理页号相同映射
///
/// `Framed`：为每个虚拟页分配独立物理页帧
///
/// `Linear(offset)`：线性映射，物理页号 = 虚拟页号 + offset
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum MapType {
    /// vpn == ppn
    Identical,
    /// 每个页分配独立帧
    Framed,
    /// 映射关系为线性偏移， ppn = vpn + offset
    Linear(isize),
}

bitflags! {
    /// 页映射权限
    ///
    /// R：可读
    ///
    /// W：可写
    ///
    /// X：可执行
    ///
    /// U：用户态可访问
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    pub struct MapPermission: u8 {
        /// 可读
        const R = 1 << 1;
        /// 可写
        const W = 1 << 2;
        /// 可执行
        const X = 1 << 3;
        /// 用户态可访问
        const U = 1 << 4;
    }
}
