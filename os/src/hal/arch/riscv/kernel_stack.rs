use crate::hal::{KERNEL_STACK_SIZE, PAGE_SIZE, TRAMPOLINE, TRAP_CONTEXT_BASE, USER_STACK_SIZE};
use crate::mm::{MapPermission, VirtAddr, KERNEL_SPACE};
use crate::sync::UPIntrFreeCell;
use alloc::vec::Vec;
use lazy_static::lazy_static;

lazy_static! {
    /// 单线程安全的内核栈分配器
    ///
    /// `lazy_static` 宏保证在第一次使用时初始化
    static ref KSTACK_ALLOCATOR: UPIntrFreeCell<RecycleAllocator> =
        unsafe { UPIntrFreeCell::new(RecycleAllocator::new()) };
}


/// 内核栈分配器结构体
///
/// 使用简单的回收分配策略（Recycle Allocator）：
struct RecycleAllocator {
    /// 当前分配的最大栈 ID
    current: usize,
    /// 回收的栈 ID，可以重新分配
    recycled: Vec<usize>,
}

/// 分配一个新的内核栈
///
/// 返回一个 `KernelStack` 句柄，内部记录栈 ID
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

/// 根据内核栈 ID 计算栈的底部和顶部虚拟地址
fn kernel_stack_position(kstack_id: usize) -> (usize, usize) {
    // 栈从 trampoline 段向低地址增长，每个栈之间间隔一页保护页
    let top = TRAMPOLINE - kstack_id * (KERNEL_STACK_SIZE + PAGE_SIZE);
    let bottom: usize = top - KERNEL_STACK_SIZE;
    (bottom, top)
}

impl RecycleAllocator {
    /// 创建一个新的回收分配器
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
    /// 确保 ID 在当前范围内，并且未被重复回收
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

/// 内核栈句柄，封装栈 ID
pub struct KernelStack(pub usize);

/// 通过线程 ID 获取 Trap Context 的底部虚拟地址
///
/// Trap Context 存储用户态寄存器状态，每个线程占一页
pub fn trap_cx_bottom_from_tid(tid: usize) -> usize {
    TRAP_CONTEXT_BASE - tid * PAGE_SIZE
}

/// 通过线程 ID 和用户栈基地址计算用户栈底部虚拟地址
///
/// 每个用户栈占 USER_STACK_SIZE + 保护页
pub fn ustack_bottom_from_tid(ustack_base: usize, tid: usize) -> usize {
    ustack_base + tid * (PAGE_SIZE + USER_STACK_SIZE)
}

impl KernelStack {
    /// 将一个值压入内核栈顶部
    ///
    /// 返回指向压入值的指针
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
