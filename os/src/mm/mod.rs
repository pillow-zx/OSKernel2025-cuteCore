pub mod address;
mod frame_allocator;
mod heap_allocator;
mod memory_set;
mod pagetable;

/// 初始化内存管理子系统
/// 包括堆内存分配器、物理页帧分配器和内核虚拟地址空间的建立与激活
pub fn init() {
    heap_allocator::init_heap();
    frame_allocator::init_frame_allocator();
    KERNEL_SPACE.exclusive_access().activate();
}

pub use crate::mm::memory_set::{kernel_token, MapPermission, MemorySet, KERNEL_SPACE,MapFlags};
pub use address::{PhysAddr, PhysPageNum, StepByOne, VirtAddr, VirtPageNum};
pub use frame_allocator::{frame_alloc, frame_alloc_more, frame_dealloc, FrameTracker};
pub use pagetable::{
    translated_byte_buffer, translated_ref, translated_refmut, translated_str, PageTable,
    UserBuffer,copy_to_user,
};
