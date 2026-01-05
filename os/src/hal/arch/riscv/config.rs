//! 内存布局和大小相关常量
//!
//! 这些常量用于操作系统内核的内存管理、栈分配、堆分配以及特定功能段（如 trampoline 和 trap context）。
//! 所有大小都以字节为单位，部分使用页面（4KB）为单位。

#![allow(unused)]

/// 单页大小，4KB
pub const PAGE_SIZE: usize = 0x1000; // 4 * 1024 = 4096 bytes

/// 页大小对应的位数，用于位运算
/// 例如，页对齐地址可以用 addr >> PAGE_SIZE_BITS
pub const PAGE_SIZE_BITS: usize = 0xc; // 12，即 2^12 = 4096 bytes

/// 用户态栈大小，2 页，总共 8KB
pub const USER_STACK_SIZE: usize = PAGE_SIZE * 2; // 8 KB

/// 内核栈大小，2 页，总共 8KB
pub const KERNEL_STACK_SIZE: usize = PAGE_SIZE * 2; // 8 KB

/// 内核堆大小，16MB
/// 0x4000 = 16384 页，每页 4KB
pub const KERNEL_HEAP_SIZE: usize = PAGE_SIZE * 0x4000; // 16 MB

/// Trampoline 段的起始地址
/// Trampoline 是用于内核与用户态切换的特殊代码段，通常映射到最高虚拟地址
pub const TRAMPOLINE: usize = usize::MAX - PAGE_SIZE + 1; // 通常是虚拟地址空间的最顶端 - 4KB

/// Trap Context 的基地址
/// Trap Context 用于保存用户态到内核态的寄存器状态
/// 紧邻 trampoline 之下，占一页
pub const TRAP_CONTEXT_BASE: usize = TRAMPOLINE - PAGE_SIZE; // 位于 trampoline 之前

/// 内存结束地址
/// 用于标记物理或虚拟内存的可用上限
pub const MEMORY_END: usize = 0x8800_0000; // 约 2.2 GB

/// 内存块大小，512 字节
/// 常用于文件系统或磁盘块管理
pub const BLOCK_SZ: usize = 512;
pub const UserStackBase: usize = TRAP_CONTEXT_BASE - 8 * (PAGE_SIZE + USER_STACK_SIZE);