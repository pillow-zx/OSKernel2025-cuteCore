mod context;
mod pid;
mod process;
mod task;
mod signal;

use alloc::sync::Arc;
pub use context::TaskContext;
pub use task::{TaskControlBlock};

pub fn current_task() -> Option<Arc<TaskControlBlock>> {
    todo!()
}

pub fn suspend_current_and_run_next() {
    todo!()
}

pub fn block_current_and_run_next() {
    todo!()
}

pub fn block_current_task() -> *mut TaskContext {
    todo!()
}

pub fn wakeup_task(_task: Arc<TaskControlBlock>) {
    todo!()
}