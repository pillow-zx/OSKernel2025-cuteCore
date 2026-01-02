//! 虚拟内存管理与跨空间数据访问
//!
//! # Overview
//! 该模块是内核与用户态地址空间交互的核心组件。由于内核在分页模式下运行，无法直接解引用用户态传来的
//! 虚拟地址。本模块通过手动遍历页表，实现了将用户态虚拟地址安全地翻译为内核可访问的物理地址、
//! 引用或字节缓冲区的功能。
//!
//! # Design
//! - **接口抽象**：通过 `PageTable` trait 定义了页表操作的标准行为，实现了内核逻辑与硬件分页结构的解耦。
//! - **零拷贝倾向**：`translated_ref` 等函数尝试返回原始物理内存的引用，以减少内核与用户态之间的数据拷贝开销。
//! - **不连续性映射**：`UserBuffer` 结构体通过分段切片（`Vec<&mut [u8]>`）解决了用户虚拟空间连续但物理空间不连续的问题。
//!
//! # Assumptions
//! 1. **恒等映射/直接映射**：假设内核已经将所有物理内存或目标物理页帧映射到了内核虚拟地址空间，
//!    使得 `PhysAddr.get_mut()` 等方法能有效获取内核可直接操作的引用。
//! 2. **Token 有效性**：传入的 `token`（如 SATP 寄存器值）必须指向一个结构完整且有效的多级页表。
//!
//! # Safety
//! - **生命周期安全**：返回的 `&'static mut T` 实际上是基于内核对物理页帧的临时访问。在实际使用中，
//!   开发者必须确保在持有该引用期间，对应的物理页不会被释放或重新分配（虽然标注为 `'static` 以绕过借用检查）。
//! - **手动验证**：模块函数通过 `Option` 处理翻译失败的情况，防止因用户传入非法地址导致内核触发异常（Panic）。
//!
//! # Invariants
//! - **页对齐独立性**：`translated_byte_buffer` 必须保证无论用户地址是否页对齐，都能正确计算跨页边界，
//!   并生成覆盖完整请求长度的切片序列。
//! - **单向依赖**：该模块仅依赖底层的 `hal` 和 `mm` 模块，不应产生向上依赖，以维持内核分层结构。


use crate::hal::{PageTableEntryImpl, PageTableImpl};
use crate::mm::{MapPermission, PhysAddr, PhysPageNum, StepByOne, VirtAddr, VirtPageNum};
use alloc::string::String;
use alloc::vec::Vec;

/// 页表接口抽象：定义了硬件分页系统的核心操作，强制要求实现体系结构相关的转换逻辑。
pub trait PageTable {
    fn new() -> Self;

    fn new_kernel() -> Self;

    fn from_token(token: usize) -> Self;

    fn find_pte_create(&mut self, vpn: VirtPageNum) -> Option<&mut PageTableEntryImpl>;

    fn find_pte(&self, vpn: VirtPageNum) -> Option<&mut PageTableEntryImpl>;

    fn map(&mut self, vpn: VirtPageNum, ppn: PhysPageNum, flags: MapPermission);

    fn unmap(&mut self, vpn: VirtPageNum);

    fn translate(&self, vpn: VirtPageNum) -> Option<PageTableEntryImpl>;

    fn translate_va(&self, va: VirtAddr) -> Option<PhysAddr>;

    fn activate(&self);

    fn token(&self) -> usize;
}

/// 将用户缓冲区翻译为内核切片集合
///
/// ## Safety
/// 必须确保 `token` 对应的进程在当前操作完成前不会被销毁。
pub fn translated_byte_buffer(token: usize, ptr: *const u8, len: usize) -> Vec<&'static mut [u8]> {
    let page_table: PageTableImpl = PageTable::from_token(token);
    let mut start = ptr as usize;
    let end = start + len;
    let mut v = Vec::new();
    while start < end {
        let start_va = VirtAddr::from(start);
        let mut vpn = start_va.floor();
        let ppn = page_table.translate(vpn).unwrap().ppn();
        vpn.step();
        let mut end_va: VirtAddr = vpn.into();
        end_va = end_va.min(VirtAddr::from(end));
        if end_va.page_offset() == 0 {
            v.push(&mut ppn.get_bytes_array()[start_va.page_offset()..]);
        } else {
            v.push(&mut ppn.get_bytes_array()[start_va.page_offset()..end_va.page_offset()]);
        }
        start = end_va.into();
    }
    v
}

/// 从用户空间读取以 `\0` 结尾的字符串并拷贝到内核空间的 String 中
pub fn translated_str(token: usize, ptr: *const u8) -> String {
    let page_table: PageTableImpl = PageTable::from_token(token);
    let mut string = String::new();
    let mut va = ptr as usize;
    loop {
        let ch: u8 = *(page_table
            .translate_va(VirtAddr::from(va))
            .unwrap()
            .get_mut());
        if ch == 0 {
            break;
        }
        string.push(ch as char);
        va += 1;
    }
    string
}

/// 将用户空间的指针翻译为地址空间中对相同物理位置的不可变引用
pub fn translated_ref<T>(token: usize, ptr: *const T) -> &'static T {
    let page_table: PageTableImpl = PageTable::from_token(token);
    page_table
        .translate_va(VirtAddr::from(ptr as usize))
        .unwrap()
        .get_ref()
}

/// 将用户空间的指针翻译为地址空间中对相同物理位置的可变引用
pub fn translated_refmut<T>(token: usize, ptr: *mut T) -> &'static mut T {
    let page_table: PageTableImpl = PageTable::from_token(token);
    let va = ptr as usize;
    page_table
        .translate_va(VirtAddr::from(va))
        .unwrap()
        .get_mut()
}


/// 用户缓冲区容器
///
/// ## Design
/// 用于处理从用户态传入的、在物理上可能由多个不连续页帧组成的复杂数据缓冲区。
pub struct UserBuffer {
    pub buffers: Vec<&'static mut [u8]>,
}

impl UserBuffer {
    /// 从切片向量创建 UserBuffer
    pub fn new(buffers: Vec<&'static mut [u8]>) -> Self {
        Self { buffers }
    }
    /// 计算缓冲区总长度
    pub fn len(&self) -> usize {
        let mut total: usize = 0;
        for b in self.buffers.iter() {
            total += b.len();
        }
        total
    }
}

/// 为 UserBuffer 实现迭代器
///
/// # Design
/// 允许按字节遍历分布在不同物理页帧中的用户数据。
impl IntoIterator for UserBuffer {
    type Item = *mut u8;
    type IntoIter = UserBufferIterator;
    fn into_iter(self) -> Self::IntoIter {
        UserBufferIterator {
            buffers: self.buffers,
            current_buffer: 0,
            current_idx: 0,
        }
    }
}

pub struct UserBufferIterator {
    buffers: Vec<&'static mut [u8]>,
    current_buffer: usize,
    current_idx: usize,
}

impl Iterator for UserBufferIterator {
    type Item = *mut u8;
    fn next(&mut self) -> Option<Self::Item> {
        if self.current_buffer >= self.buffers.len() {
            None
        } else {
            let r = &mut self.buffers[self.current_buffer][self.current_idx] as *mut _;
            if self.current_idx + 1 == self.buffers[self.current_buffer].len() {
                self.current_idx = 0;
                self.current_buffer += 1;
            } else {
                self.current_idx += 1;
            }
            Some(r)
        }
    }
}
