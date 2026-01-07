//! # 任务控制块（TaskControlBlock）模块
//!
//! ## Overview
//! 本模块实现了内核中 **任务控制块（TCB）** 与 **用户任务资源管理（TaskUserRes）**。
//! 它是内核调度与进程管理的核心数据结构，包含任务的内核栈、用户栈、trap 上下文、
//! 状态信息以及与所属进程的关联。
//!
//! 功能包括：
//! - 管理任务上下文（`TaskContext`）
//! - 提供就绪、阻塞、运行状态管理（`TaskStatus`）
//! - 用户栈与 trap 上下文分配与回收
//! - 绑定所属进程控制块（PCB）
//!
//! ## Assumptions
//! - 系统运行在单处理器 + 中断并发模型下
//! - 所有内存分配、栈管理由内核提供的 `kstack_alloc`、`memory_set` 等接口完成
//! - `TaskUserRes` 的生命周期与 `TaskControlBlock` 紧密绑定
//!
//! ## Safety
//! - 内核栈和 trap 上下文地址必须合法且对齐
//! - 用户栈分配需保证不与其他任务冲突
//! - 对 TCB 内部状态的修改均通过 `UPIntrFreeCell` 保护
//!
//! ## Invariants
//! - `TaskControlBlockInner.task_status` 与调度队列状态保持一致
//! - `TaskUserRes.tid` 唯一且在所属进程范围内有效
//! - `trap_cx_ppn` 对应的物理页已经被分配并映射
//! - 用户栈与 trap 上下文空间互不重叠
//!
//! ## Behavior
//! - `TaskControlBlock::new`：
//!   - 分配内核栈、trap 上下文页
//!   - 创建用户栈与 trap 上下文（可选）
//!   - 初始化 TCB 为 `Ready` 状态
//! - `TaskUserRes`：
//!   - 分配 / 回收用户栈和 trap 上下文
//!   - 分配 / 回收 TID
//! - TCB 与 TID 的生命周期严格绑定

use crate::hal::{kstack_alloc, trap_cx_bottom_from_tid, ustack_bottom_from_tid, KernelStack, PageTableImpl, TrapContext, UserStackBase, PAGE_SIZE, USER_STACK_SIZE};
use crate::mm::{MapPermission, MemorySet, PageTable, PhysPageNum, VirtAddr};
use crate::sync::{UPIntrFreeCell, UPIntrRefMut};
use crate::task::context::TaskContext;
use crate::task::process::ProcessControlBlock;
use alloc::sync::{Arc, Weak};

/// 任务控制块
///
/// ## Overview
/// 保存内核调度所需的任务信息
pub struct TaskControlBlock {
    /// 所属进程控制块（弱引用）
    pub process: Weak<ProcessControlBlock>,
    /// 内核栈
    pub kstack: KernelStack,
    /// 内部可变状态，由 UPIntrFreeCell 保护
    pub inner: UPIntrFreeCell<TaskControlBlockInner>,
}

impl TaskControlBlock {
    /// 获取内部可变状态的独占访问
    pub fn inner_exclusive_access(&self) -> UPIntrRefMut<'_, TaskControlBlockInner> {
        self.inner.exclusive_access()
    }

    /// 获取任务所属进程的用户页表 token
    pub fn get_user_token(&self) -> usize {
        let process = self.process.upgrade().unwrap();
        let inner = process.inner_exclusive_access();
        inner.memory_set.token()
    }
}

impl TaskControlBlock {
    /// 创建一个新的任务控制块
    ///
    /// ## Parameters
    /// - `process`：所属进程
    /// - `ustack_base`：用户栈基址
    /// - `alloc_user_res`：是否分配用户栈与 trap 上下文
    pub fn new(
        process: Arc<ProcessControlBlock>,
        ustack_base: usize,
        alloc_user_res: bool,
    ) -> Self {
        let res = TaskUserRes::new(Arc::clone(&process),  alloc_user_res);
        let trap_cx_ppn = res.trap_cx_ppn();
        let kstack = kstack_alloc();
        let kstack_top = kstack.get_top();
        Self {
            process: Arc::downgrade(&process),
            kstack,
            inner: unsafe {
                UPIntrFreeCell::new(TaskControlBlockInner {
                    res: Some(res),
                    trap_cx_ppn,
                    task_cx: TaskContext::goto_trap_return(kstack_top),
                    task_status: TaskStatus::Ready,
                    exit_code: None,
                })
            },
        }
    }
}

/// TCB 内部状态
pub struct TaskControlBlockInner {
    /// 用户任务资源（用户栈 + trap 上下文）
    pub res: Option<TaskUserRes>,
    /// trap 上下文物理页号
    pub trap_cx_ppn: PhysPageNum,
    /// 任务上下文（内核栈上下文）
    pub task_cx: TaskContext,
    /// 任务状态
    pub task_status: TaskStatus,
    /// 退出码（None 表示未退出）
    pub exit_code: Option<i32>,
}

impl TaskControlBlockInner {
    /// 获取 trap 上下文的可变引用
    pub fn get_trap_cx(&self) -> &'static mut TrapContext {
        self.trap_cx_ppn.get_mut()
    }

    #[allow(unused)]
    fn get_status(&self) -> TaskStatus {
        self.task_status
    }
}

/// 用户任务资源
///
/// ## Overview
/// 管理任务的 TID、用户栈及 trap 上下文
pub struct TaskUserRes {
    /// 任务 ID（在所属进程范围内唯一）
    pub tid: usize,
    // /// 用户栈基址
    // pub ustack_base: usize,
    /// 所属进程控制块（弱引用）
    pub process: Weak<ProcessControlBlock>,
}

impl TaskUserRes {
    /// 创建新的用户任务资源
    ///
    /// ## Behavior
    /// - 分配 TID
    /// - 根据 `alloc_user_res` 决定是否分配用户栈和 trap 上下文
    pub fn new(
        process: Arc<ProcessControlBlock>,
        // ustack_base: usize,
        alloc_user_res: bool,
    ) -> Self {
        let tid = process.inner_exclusive_access().alloc_tid();
        let task_user_res = Self {
            tid,
            // ustack_base,
            process: Arc::downgrade(&process),
        };
        if alloc_user_res {
            task_user_res.alloc_user_res();
        }
        task_user_res
    }

    /// 分配用户栈与 trap 上下文
    pub fn alloc_user_res(&self) {
        let process = self.process.upgrade().unwrap();
        let mut process_inner = process.inner_exclusive_access();

        // 用户栈
        let ustack_bottom = ustack_bottom_from_tid(self.tid);
        let ustack_top = ustack_bottom + USER_STACK_SIZE;
        process_inner.memory_set.insert_framed_area(
            ustack_bottom.into(),
            ustack_top.into(),
            MapPermission::R | MapPermission::W | MapPermission::U,
        );

        // trap 上下文
        let trap_cx_bottom = trap_cx_bottom_from_tid(self.tid);
        let trap_cx_top = trap_cx_bottom + PAGE_SIZE;
        process_inner.memory_set.insert_framed_area(
            trap_cx_bottom.into(),
            trap_cx_top.into(),
            MapPermission::R | MapPermission::W,
        );
    }

    /// 回收用户资源
    fn dealloc_user_res(&self) {
        // 回收资源前确保进程仍然存在
        let process = self.process.upgrade().unwrap();
        let mut process_inner = process.inner_exclusive_access();

        // 回收用户栈
        let ustack_bottom_va: VirtAddr = ustack_bottom_from_tid(self.tid).into();
        process_inner
            .memory_set
            .remove_area_with_start_vpn(ustack_bottom_va.into());

        // trap 上下文
        let trap_cx_bottom_va: VirtAddr = trap_cx_bottom_from_tid(self.tid).into();
        process_inner
            .memory_set
            .remove_area_with_start_vpn(trap_cx_bottom_va.into());
    }

    /// 分配新的 TID
    #[allow(unused)]
    pub fn alloc_tid(&mut self) {
        self.tid = self
            .process
            .upgrade()
            .unwrap()
            .inner_exclusive_access()
            .alloc_tid();
    }

    /// 回收 TID
    pub fn dealloc_tid(&self) {
        let process = self.process.upgrade().unwrap();
        let mut process_inner = process.inner_exclusive_access();
        process_inner.dealloc_tid(self.tid);
    }

    /// 获取 trap 上下文在用户空间的虚拟地址
    pub fn trap_cx_user_va(&self) -> usize {
        trap_cx_bottom_from_tid(self.tid)
    }

    /// 获取 trap 上下文对应的物理页号
    pub fn trap_cx_ppn(&self) -> PhysPageNum {
        let process = self.process.upgrade().unwrap();
        let process_inner = process.inner_exclusive_access();
        let trap_cx_bottom_va: VirtAddr = trap_cx_bottom_from_tid(self.tid).into();
        process_inner
            .memory_set
            .translate(trap_cx_bottom_va.into())
            .unwrap()
            .ppn()
    }

    /// 用户栈基址
    /// 后续写死在config里面，这里是之前的接口
    pub fn ustack_base(&self) -> usize {
        UserStackBase
    }

    /// 用户栈顶部地址
    /// 由于起始位置是从guard页之后开始的，且一次跳栈大小和guard页，所以这里就不用了
    pub fn ustack_top(&self) -> usize {
        ustack_bottom_from_tid(self.tid) + USER_STACK_SIZE
    }
}

impl Drop for TaskUserRes {
    /// 回收用户资源与 TID
    fn drop(&mut self) {
        self.dealloc_tid();
        self.dealloc_user_res();
    }
}

/// 任务状态
#[derive(Copy, Clone, PartialEq)]
pub enum TaskStatus {
    Ready,
    Running,
    Blocked,
}
