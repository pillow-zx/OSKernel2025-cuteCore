//! # 任务调度与生命周期模块
//!
//! ## Overview
//! 本模块实现了内核中 **任务（线程）调度、阻塞、唤醒及退出** 的核心逻辑。
//! 同时提供对初始进程 `initproc` 的管理，以及对信号的检查与发送。
//!
//! ## Assumptions
//! - 单处理器环境，任务调度通过手动切换 `TaskContext` 实现
//! - 每个进程至少有一个主线程
//! - `INITPROC` 始终存在，且 PID 为非回收的初始 PID
//!
//! ## Safety
//! - 所有对 PCB/TCB 内部的可变访问通过 `UPIntrFreeCell` 或 `UPIntrRefMut` 独占访问，避免数据竞争
//! - 用户资源（ustack、trap_cx、tid）在主线程退出前被正确释放，防止内存泄漏
//! - `schedule` 必须提供有效 `TaskContext` 指针，否则会导致上下文切换错误
//!
//! ## Invariants
//! - `TaskStatus::Running` 的任务在调度器中不可重复存在
//! - PCB 内部 `tasks` 的索引与 TID 一一对应
//! - 当主线程退出时，所有子进程被重新挂载到 `initproc`
//! - 文件描述符表、内存空间、用户资源正确清理，防止重复释放
//!
//! ## Behavior
//! - `suspend_current_and_run_next()`：
//!   - 将当前 Running 任务标记为 Ready
//!   - 放回调度器队列
//!   - 调用 `schedule` 执行下一任务
//! - `block_current_task()`：
//!   - 将当前任务标记为 Blocked
//!   - 返回任务上下文指针
//! - `block_current_and_run_next()`：
//!   - 阻塞当前任务并调度下一任务
//! - `exit_current_and_run_next(exit_code)`：
//!   - 记录退出码，释放用户资源
//!   - 如果主线程退出，处理 PCB 回收、子进程重新挂载到 `initproc`
//!   - 调度下一任务
//! - `INITPROC`：
//!   - 通过 ELF 文件创建初始进程 PCB
//!   - 保证系统启动后至少有一个进程存在
//! - 信号处理：
//!   - `check_signals_of_current()` 返回当前进程的错误信号
//!   - `current_add_signal(signal)` 向当前进程添加信号

mod context;
mod manager;
mod pid;
mod process;
mod processor;
mod signal;
mod task;

use alloc::sync::Arc;
use alloc::vec::Vec;
pub use context::TaskContext;
use lazy_static::lazy_static;
pub use manager::{add_task, pid2process, remove_from_pid2process, wakeup_task};
pub use processor::{
    current_kstack_top, current_process, current_task, current_trap_cx, current_trap_cx_user_va,
    current_user_token, run_tasks, schedule, take_current_task,
};

use crate::fs::{open_initproc, OpenFlags};
use crate::hal::shutdown;
use crate::task::pid::IDLE_PID;
pub use crate::task::process::{ProcessControlBlock,ProcessControlBlockInner};
use crate::task::task::TaskUserRes;
pub use signal::SignalFlags;
pub use task::{TaskControlBlock, TaskStatus};

/// 挂起当前任务并运行下一个任务
///
/// - 当前任务状态置为 Ready
/// - 放回调度器
/// - 调用 `schedule` 切换到下一任务
pub fn suspend_current_and_run_next() {
    // There must be an application running.
    let task = take_current_task().unwrap();

    // ---- access current TCB exclusively
    let mut task_inner = task.inner_exclusive_access();
    let task_cx_ptr = &mut task_inner.task_cx as *mut TaskContext;
    // Change status to Ready
    task_inner.task_status = TaskStatus::Ready;
    drop(task_inner);
    // ---- release current TCB

    // push back to ready queue.
    add_task(task);
    // jump to scheduling cycle
    schedule(task_cx_ptr);
}

/// 阻塞当前任务
///
/// - 当前任务状态置为 Blocked
/// - 返回任务上下文指针
pub fn block_current_task() -> *mut TaskContext {
    let task = take_current_task().unwrap();
    let mut task_inner = task.inner_exclusive_access();
    task_inner.task_status = TaskStatus::Blocked;
    &mut task_inner.task_cx as *mut TaskContext
}

/// 阻塞当前任务并调度下一任务
pub fn block_current_and_run_next() {
    let task_cx_ptr = block_current_task();
    schedule(task_cx_ptr);
}

/// 退出当前任务并运行下一任务
///
/// - 记录退出码，释放用户资源
/// - 如果是主线程退出，处理 PCB 回收、子进程重新挂载到 `initproc`
/// - 调用 `schedule` 调度下一任务
pub fn exit_current_and_run_next(exit_code: i32) {
    let task = take_current_task().unwrap();
    let mut task_inner = task.inner_exclusive_access();
    let process = task.process.upgrade().unwrap();
    let tid = task_inner.res.as_ref().unwrap().tid;
    // record exit code
    task_inner.exit_code = Some(exit_code);
    task_inner.res = None;
    // here we do not remove the thread since we are still using the kstack
    // it will be deallocated when sys_waittid is called
    drop(task_inner);
    drop(task);
    // however, if this is the main thread of current process
    // the process should terminate at once
    if tid == 0 {
        let pid = process.getpid();
        if pid == IDLE_PID {
            println!(
                "[kernel] Idle process exit with exit_code {} ...",
                exit_code
            );
            if exit_code != 0 {
                //crate::sbi::shutdown(255); //255 == -1 for err hint
                shutdown();
            } else {
                //crate::sbi::shutdown(0); //0 for success hint
                shutdown();
            }
        }
        remove_from_pid2process(pid);
        let mut process_inner = process.inner_exclusive_access();
        // mark this process as a zombie process
        process_inner.is_zombie = true;
        // record exit code of main process
        process_inner.exit_code = exit_code;

        {
            // move all child processes under init process
            let mut initproc_inner = INITPROC.inner_exclusive_access();
            for child in process_inner.children.iter() {
                child.inner_exclusive_access().parent = Some(Arc::downgrade(&INITPROC));
                initproc_inner.children.push(child.clone());
            }
        }

        // deallocate user res (including tid/trap_cx/ustack) of all threads
        // it has to be done before we dealloc the whole memory_set
        // otherwise they will be deallocated twice
        let mut recycle_res = Vec::<TaskUserRes>::new();
        for task in process_inner.tasks.iter().filter(|t| t.is_some()) {
            let task = task.as_ref().unwrap();
            let mut task_inner = task.inner_exclusive_access();
            if let Some(res) = task_inner.res.take() {
                recycle_res.push(res);
            }
        }
        // dealloc_tid and dealloc_user_res require access to PCB inner, so we
        // need to collect those user res first, then release process_inner
        // for now to avoid deadlock/double borrow problem.
        drop(process_inner);
        recycle_res.clear();

        let mut process_inner = process.inner_exclusive_access();
        process_inner.children.clear();
        // deallocate other data in user space i.e. program code/data section
        process_inner.memory_set.recycle_data_pages();
        // drop file descriptors
        process_inner.fd_table.clear();
        // Remove all tasks except for the main thread itself.
        // This is because we are still using the kstack under the TCB
        // of the main thread. This TCB, including its kstack, will be
        // deallocated when the process is reaped via waitpid.
        while process_inner.tasks.len() > 1 {
            process_inner.tasks.pop();
        }
    }
    drop(process);
    // we do not have to save task context
    let mut _unused = TaskContext::zero_init();
    schedule(&mut _unused as *mut _);
}

lazy_static! {
    /// 系统初始化进程 PCB
    pub static ref INITPROC: Arc<ProcessControlBlock> = {
        let inode = open_initproc(OpenFlags::RDONLY).unwrap();  // 已仅读模式打开 initproc 文件
        let v = inode.read_all();   // 读取 initproc 文件的全部内容到内存中
        ProcessControlBlock::new(v.as_slice())  // 创建 initproc 进程控制块
    };
}

/// 将 INITPROC 添加到系统中
pub fn add_initproc() {
    let _initproc = INITPROC.clone(); // 提前克隆 INITPROC，确保其在后续使用中不会被释放
}

/// 检查当前进程的信号
pub fn check_signals_of_current() -> Option<(i32, &'static str)> {
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    process_inner.signals.check_error()
}

/// 向当前进程添加信号
pub fn current_add_signal(signal: SignalFlags) {
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    process_inner.signals |= signal;
}
