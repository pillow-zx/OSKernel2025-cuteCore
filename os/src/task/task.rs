use alloc::sync::{Arc, Weak};
use crate::hal::KernelStack;
use crate::mm::PhysPageNum;
use crate::sync::UPIntrFreeCell;
use crate::task::context::TaskContext;
use crate::task::process::ProcessControlBlock;

pub struct TaskControlBlock {
    pub process: Weak<ProcessControlBlock>,
    pub kstack: KernelStack,
    pub inner: UPIntrFreeCell<TaskContrlBlockInner>,
}

pub struct TaskContrlBlockInner {
    pub res: Option<TaskUserRes>,
    pub trap_cx_ppn: PhysPageNum,
    pub task_cx: TaskContext,
    pub task_status: TaskStatus,
    pub exit_code: Option<i32>,
}

struct TaskUserRes {
    pub tid: usize,
    pub ustack_bas: usize,
    pub process: Weak<ProcessControlBlock>,
}

#[derive(Copy, Clone, PartialEq)]
pub enum TaskStatus {
    Ready,
    Running,
    Blocked,
}
