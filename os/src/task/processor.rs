//! 处理器与任务调度模块。
//!
//! 本模块定义了 `Processor` 结构体以及与之相关的全局调度逻辑，
//! 用于描述单个 CPU 核心在内核中的运行状态，并负责在任务之间
//! 进行上下文切换。
//!
//! # Overview
//! - 系统中每个 CPU 核心对应一个全局 `Processor` 实例
//! - `Processor` 记录当前正在运行的任务以及空闲任务的上下文
//! - 调度器通过 `__switch` 在任务上下文与空闲上下文之间切换
//!
//! # Concurrency Model
//! - 本模块假定运行在单核环境（UP）或已禁用抢占的上下文中
//! - 所有对 `Processor` 的访问都必须通过 `UPIntrFreeCell` 进行
//! - 在进入调度与上下文切换前，必须保证不存在并发访问
//!
//! # Safety
//! - 本模块包含多处 `unsafe` 代码，用于执行底层上下文切换
//! - 所有 `unsafe` 使用点均在局部通过 `SAFETY:` 注释说明其正确性前提
//! - 调用方必须遵守文档中描述的不变量，否则行为未定义
//!
//! # Invariants
//! - 任意时刻，至多只有一个任务处于 Running 状态
//! - `PROCESSOR.current` 与实际正在 CPU 上运行的任务保持一致
//! - 上下文切换期间，不得并发访问任务或处理器状态

use crate::hal::{TrapContext, __switch};
use crate::sync::UPIntrFreeCell;
use crate::task::manager::fetch_task;
use crate::task::process::ProcessControlBlock;
use crate::task::{TaskContext, TaskControlBlock, TaskStatus};
use crate::fs::{open_dir, open_file};
use crate::fs::inode::{OSInode,OpenFlags};
use alloc::sync::Arc;
use lazy_static::lazy_static;

/// Processor 表示一个 CPU 核心的调度状态。
///
/// 每个 CPU 核心对应一个 `Processor` 实例，用于保存
/// 当前正在运行的任务以及空闲任务的上下文。
///
/// # Design
/// - 每个 CPU 核心恰好对应一个 `Processor`
/// - `Processor` 不会在 CPU 核心之间迁移
///
/// # Invariants
/// - 任意时刻，至多只有一个执行流可以访问该 `Processor`
/// - 所有访问必须通过 `UPIntrFreeCell` 进行
pub struct Processor {
    /// 当前正在运行的任务。
    ///
    /// - `Some`：当前 CPU 正在运行该任务
    /// - `None`：当前 CPU 处于空闲状态
    ///
    /// INVARIANT:
    /// - 在调度切换期间可能暂时为 `None`
    /// - 不允许同时存在多个运行任务
    current: Option<Arc<TaskControlBlock>>,

    /// 空闲任务（idle task）的上下文。
    ///
    /// 当没有可运行任务时，CPU 会切换到该上下文执行。
    idle_task_cx: TaskContext,
}

impl Processor {
    /// 创建一个新的 Processor。
    ///
    /// 初始状态下：
    /// - 当前任务为 `None`
    /// - 空闲任务上下文被零初始化
    pub fn new() -> Self {
        Self {
            current: None,
            idle_task_cx: TaskContext::zero_init(),
        }
    }

    /// 获取空闲任务上下文的可变指针。
    ///
    /// SAFETY:
    /// - 返回的指针仅在调度切换时使用
    /// - 调用方必须保证在使用该指针期间不存在并发访问
    fn get_idle_task_cx_ptr(&mut self) -> *mut TaskContext {
        &mut self.idle_task_cx as *mut _
    }

    /// 取出当前正在运行的任务。
    ///
    /// 调用后，Processor 的当前任务将变为 `None`。
    pub fn take_current(&mut self) -> Option<Arc<TaskControlBlock>> {
        self.current.take()
    }

    /// 获取当前正在运行任务的引用。
    ///
    /// 不会改变 Processor 内部状态。
    pub fn current(&self) -> Option<Arc<TaskControlBlock>> {
        self.current.as_ref().map(Arc::clone)
    }
}

lazy_static! {
        /// 全局 Processor 实例。
        ///
        /// INVARIANT:
        /// - 系统中只存在一个全局 `Processor`
        /// - 所有访问都必须通过 `UPIntrFreeCell` 串行化
        ///
        /// SAFETY:
        /// - `Processor::new()` 仅在系统初始化阶段调用一次
        /// - 初始化期间不会发生中断或并发访问
    pub static ref PROCESSOR: UPIntrFreeCell<Processor> =
        unsafe { UPIntrFreeCell::new(Processor::new()) };
}

/// 调度循环，不断取出可运行任务并执行。
///
/// 当存在可运行任务时，CPU 会从空闲任务切换到该任务。
pub fn run_tasks() {
    loop {
        let mut processor = PROCESSOR.exclusive_access();
        if let Some(task) = fetch_task() {
            let idle_task_cx_ptr = processor.get_idle_task_cx_ptr();

            // SAFETY:
            // - 当前持有任务内部的独占访问权
            // - 返回的指针在任务状态切换前保持有效
            let next_task_cx_ptr = task.inner.exclusive_session(|task_inner| {
                task_inner.task_status = TaskStatus::Running;
                &task_inner.task_cx as *const TaskContext
            });
            processor.current = Some(task);

            // 在上下文切换前显式释放 Processor 的访问权
            drop(processor);

            // SAFETY:
            // - idle_task_cx_ptr 和 next_task_cx_ptr 均指向有效的 TaskContext
            // - 当前不会发生并发上下文切换
            unsafe {
                __switch(idle_task_cx_ptr, next_task_cx_ptr);
            }
        } else {
            println!("No task to run, shutting down...");
        }
    }
}

/// 获得当前正在运行任务的 TCB，并将其从处理器中取出
pub fn take_current_task() -> Option<Arc<TaskControlBlock>> {
    PROCESSOR.exclusive_access().take_current()
}

/// 获得当前正在运行任务的 TCB 的引用
pub fn current_task() -> Option<Arc<TaskControlBlock>> {
    PROCESSOR.exclusive_access().current()
}

/// 获得当前正在运行任务所属的进程 PCB 的引用
pub fn current_process() -> Arc<ProcessControlBlock> {
    current_task().unwrap().process.upgrade().unwrap()
}

/// 获得当前正在运行任务所属进程的用户空间页表令牌
pub fn current_user_token() -> usize {
    let task = current_task().unwrap();
    task.get_user_token()
}

/// 获取当前正在运行任务的 TrapContext。
///
/// SAFETY:
/// - 返回的引用在任务切换前保持有效
/// - 调用方必须保证不会并发访问 TrapContext
pub fn current_trap_cx() -> &'static mut TrapContext {
    current_task()
        .unwrap()
        .inner_exclusive_access()
        .get_trap_cx()
}

/// 获取当前任务 TrapContext 在用户空间的虚拟地址。
pub fn current_trap_cx_user_va() -> usize {
    current_task()
        .unwrap()
        .inner_exclusive_access()
        .res
        .as_ref()
        .unwrap()
        .trap_cx_user_va()
}

/// 获取当前任务的内核栈顶地址。
pub fn current_kstack_top() -> usize {
    current_task().unwrap().kstack.get_top()
}

/// 切换回调度循环，恢复空闲任务上下文。
///
/// SAFETY:
/// - `switched_task_cx_ptr` 必须指向当前任务的有效 TaskContext
/// - 调用时不得存在并发上下文切换
pub fn schedule(switched_task_cx_ptr: *mut TaskContext) {
    let idle_task_cx_ptr =
        PROCESSOR.exclusive_session(|processor| processor.get_idle_task_cx_ptr());
    unsafe {
        __switch(switched_task_cx_ptr, idle_task_cx_ptr);
    }
}
