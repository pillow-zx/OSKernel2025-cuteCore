mod heap_allocator;
mod frame_allocator;
pub mod address;
mod memory_set;
mod pagetable;

pub fn init() {
    heap_allocator::init_heap();
    frame_allocator::init_frame_allocator();
    KERNEL_SPACE.exclusive_access().activate();
}

pub use frame_allocator::{FrameTracker, frame_alloc, frame_dealloc, frame_alloc_more};
pub use address::{PhysAddr, VirtAddr, PhysPageNum, VirtPageNum, StepByOne};
pub use pagetable::{PageTable, UserBuffer};
pub use crate::mm::memory_set::{KERNEL_SPACE, kernel_token, MapPermission, MemorySet};