//! 内核栈管理模块
//!
//! # Overview
//! 本模块提供内核栈的分配与管理功能，用于操作系统内核线程。
//! 每个内核线程分配独立的栈，并为每个线程分配 Trap Context 页面，用于保存用户态寄存器状态。
//! 模块保证内核栈在虚拟地址空间的安全分配、映射与释放。
//!
//! # Design
//! - 内核栈从虚拟地址空间顶端的 `TRAMPOLINE` 向低地址分配。
//! - 每个栈之间设置一页保护页，防止栈溢出。
//! - 使用 `RecycleAllocator` 进行栈 ID 管理：先复用回收的 ID，否则分配新的。
//! - `KernelStack` 对象 drop 时，会自动解除映射并回收栈 ID。
//!
//! # Assumptions
//! - 单线程环境下 `KSTACK_ALLOCATOR` 的独占访问由 `UPIntrFreeCell` 保证。
//! - `TRAMPOLINE` 和 `TRAP_CONTEXT_BASE` 已正确配置。
//! - `PAGE_SIZE` 与硬件页大小一致。
//!
//! # Safety
//! - `push_on_top` 通过裸指针写入内核栈，必须保证栈空间足够并且类型大小正确。
//! - 映射与解除映射必须遵守虚拟内存管理规则。
//!
//! # Invariants
//! - 每个 `KernelStack` 对应唯一的栈 ID。
//! - 回收的 ID 仅在完全释放后才会被重新使用。
//! - 内核栈在使用期间，虚拟地址范围始终完整映射。

use crate::hal::{UserStackBase, KERNEL_STACK_SIZE, PAGE_SIZE, TRAMPOLINE, TRAP_CONTEXT_BASE, USER_STACK_SIZE};
use crate::mm::{MapPermission, VirtAddr, KERNEL_SPACE};
use crate::sync::UPIntrFreeCell;
use alloc::vec::Vec;
use lazy_static::lazy_static;

lazy_static! {
    /// 全局内核栈分配器实例
    ///
    /// # Safety
    /// `UPIntrFreeCell` 保证单核环境下的独占访问。
    static ref KSTACK_ALLOCATOR: UPIntrFreeCell<RecycleAllocator> =
        unsafe { UPIntrFreeCell::new(RecycleAllocator::new()) };
}

/// 回收式内核栈分配器
///
/// # Fields
/// - `current`：当前已分配的最大栈 ID
/// - `recycled`：回收的栈 ID，等待复用
struct RecycleAllocator {
    /// 当前分配的最大栈 ID
    current: usize,
    /// 回收的栈 ID，可以重新分配
    recycled: Vec<usize>,
}

/// 分配一个新的内核栈并映射到内核空间
///
/// # Returns
/// `KernelStack` 栈句柄
pub fn kstack_alloc() -> KernelStack {
    // 从分配器获得一个可用栈 ID
    let kstack_id = KSTACK_ALLOCATOR.exclusive_access().alloc();

    // 根据栈 ID 计算栈的虚拟地址范围
    let (kstack_bottom, kstack_top) = kernel_stack_position(kstack_id);

    // 在内核地址空间中映射该栈的物理页，并设置读写权限
    KERNEL_SPACE.exclusive_access().insert_framed_area(
        kstack_bottom.into(),
        kstack_top.into(),
        MapPermission::R | MapPermission::W,
    );

    // 返回内核栈对象
    KernelStack(kstack_id)
}

/// 根据栈 ID 计算内核栈底和栈顶地址
///
/// # Arguments
/// - `kstack_id`：内核栈 ID
///
/// # Returns
/// `(bottom, top)` 虚拟地址
fn kernel_stack_position(kstack_id: usize) -> (usize, usize) {
    // 栈从 trampoline 段向低地址增长，每个栈之间间隔一页保护页
    let top = TRAMPOLINE - kstack_id * (KERNEL_STACK_SIZE + PAGE_SIZE);
    let bottom: usize = top - KERNEL_STACK_SIZE;
    (bottom, top)
}

impl RecycleAllocator {
    /// 创建一个新的回收式栈分配器
    fn new() -> Self {
        RecycleAllocator {
            current: 0,
            recycled: Vec::new(),
        }
    }

    /// 分配一个栈 ID
    ///
    /// 优先使用回收的 ID，如果没有回收的则使用新的 ID
    fn alloc(&mut self) -> usize {
        if let Some(id) = self.recycled.pop() {
            id
        } else {
            self.current += 1;
            self.current - 1
        }
    }

    /// 回收一个栈 ID
    ///
    /// # Panics
    /// - `id >= current` 时会 panic
    /// - `id` 已经被回收过时会 panic
    fn dealloc(&mut self, id: usize) {
        assert!(id < self.current);
        assert!(
            !self.recycled.iter().any(|i| *i == id),
            "id {} has been deallocated!",
            id
        );
        self.recycled.push(id);
    }
}

/// 内核栈句柄
///
/// # Fields
/// - `0`：栈 ID，由 `RecycleAllocator` 分配
pub struct KernelStack(pub usize);

/// 获取线程对应 Trap Context 页底部地址
///
/// # Arguments
/// - `tid`：线程 ID
///
/// # Returns
/// Trap Context 页底部虚拟地址
pub fn trap_cx_bottom_from_tid(tid: usize) -> usize {
    TRAP_CONTEXT_BASE - tid * PAGE_SIZE
}

/// 根据用户栈基地址和线程 ID 获取用户栈底部地址
///
/// # Arguments
/// - `ustack_base`：用户栈基地址
/// - `tid`：线程 ID
///
/// # Returns
/// 用户栈底部虚拟地址
pub fn ustack_bottom_from_tid( tid: usize) -> usize {
    UserStackBase + tid * (PAGE_SIZE + USER_STACK_SIZE)
}

impl KernelStack {
    /// 在内核栈顶压入一个值
    ///
    /// # Safety
    /// - 使用裸指针写入内核栈
    /// - 调用者必须保证栈空间足够
    #[allow(unused)]
    pub fn push_on_top<T>(&self, value: T) -> *mut T
    where
        T: Sized,
    {
        // 获取栈顶虚拟地址
        let kernel_stack_top = self.get_top();
        // 栈顶向下移动 sizeof(T) 个字节
        let ptr_mut = (kernel_stack_top - core::mem::size_of::<T>()) as *mut T;

        unsafe {
            *ptr_mut = value;
        }
        ptr_mut
    }

    /// 获取内核栈顶部虚拟地址
    pub fn get_top(&self) -> usize {
        let (_, kernel_stack_top) = kernel_stack_position(self.0);
        kernel_stack_top
    }
}

impl Drop for KernelStack {
    /// 内核栈销毁时，解除映射并回收栈 ID
    fn drop(&mut self) {
        let (kernel_stack_bottom, _) = kernel_stack_position(self.0);
        let kernel_stack_bottom_va: VirtAddr = kernel_stack_bottom.into();

        // 从内核地址空间中移除该栈对应的虚拟页
        KERNEL_SPACE
            .exclusive_access()
            .remove_area_with_start_vpn(kernel_stack_bottom_va.into());

        // 回收栈 ID
        KSTACK_ALLOCATOR.exclusive_access().dealloc(self.0);
    }
}
