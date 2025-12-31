use alloc::sync::{Arc, Weak};
use alloc::vec::Vec;
use crate::fs::File;
use crate::hal::PageTableImpl;
use crate::mm::MemorySet;
use crate::sync::{Condvar, Mutex, Semaphore, UPIntrFreeCell};
use crate::task::pid::{PidHandle, RecycleAllocator};
use crate::task::signal::SignalFlags;
use crate::task::task::TaskControlBlock;

pub struct ProcessControlBlock {
    pub pid: PidHandle,
    inner: UPIntrFreeCell<ProcessControlBlockInner>,
}

pub struct ProcessControlBlockInner {
    pub is_zombie: bool,
    pub memory_set: MemorySet<PageTableImpl>,
    pub parent: Option<Weak<ProcessControlBlock>>,
    pub children: Vec<Arc<ProcessControlBlock>>,
    pub exit_code: i32,
    pub fd_table: Vec<Option<Arc<dyn File + Send + Sync>>>,
    pub signals: SignalFlags,
    pub tasks: Vec<Option<Arc<TaskControlBlock>>>,
    pub task_res_allocator: RecycleAllocator,
    pub mutex_list: Vec<Option<Arc<dyn Mutex>>>,
    pub semaphore_list: Vec<Option<Arc<Semaphore>>>,
    pub condvar_list: Vec<Option<Arc<Condvar>>>,
}