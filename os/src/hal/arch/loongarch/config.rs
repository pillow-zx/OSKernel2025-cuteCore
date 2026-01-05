//! 内存布局和大小相关常量
//!
//! 这些常量用于操作系统内核的内存管理、栈分配、堆分配以及特定功能段（如 trampoline 和 trap context）
//! 所有大小都以字节为单位，部分使用页面（4KB）为单位

#![allow(unused)]

use crate::hal::platform;
use core::arch::asm;

/// 单页大小，4KB
pub const PAGE_SIZE: usize = 0x1000; // 4 * 1024 = 4096 bytes

/// 页大小对应的位数，用于位运算
/// 例如，页对齐地址可以用 addr >> PAGE_SIZE_BITS
pub const PAGE_SIZE_BITS: usize = 0xc; // 12，即 2^12 = 4096 bytes

//todo 看一下这个原先UER_STACK_SIZE原先设为8MB是不是预期的
//todo 我需要这个字段去给每一个线程分配用户栈，8MB是不是太大了？
//todo 我加了一个USER_STACK_Totol_SIZE字段，表示用户栈的总大小。是否是预期总大小为8MB?
/// 用户栈最大大小：8 MB
pub const USER_STACK_Totol_SIZE: usize = 8 * 0x1000 * 0x1000; // 8 MB
/// 每一个用户栈大小，2 页，总共 8KB
pub const USER_STACK_SIZE: usize = PAGE_SIZE * 2; // 8 KB

/// 内核栈大小，16 MB
pub const KERNEL_STACK_SIZE: usize = 16 * 0x1000 * 0x1000; // 16 MB

/// 用户堆最大大小：512 MB
pub const USER_HEAP_SIZE: usize = 512 * 0x1000 * 0x1000; // 512 MB

/// 内核堆大小，64 MB
pub const KERNEL_HEAP_SIZE: usize = 64 * 0x1000 * 0x1000; // 64 MB

/// 39 位虚拟地址
pub const VA_BITS: usize = 39; // 39 bits for virtual address

/// 虚拟地址掩码，低 39 位为 1
pub const VA_MASK: usize = (1 << VA_BITS) - 1; // Mask for 39-bit virtual address

/// 虚拟地址空间大小，512 GB
pub const VA_SPACE_SIZE: usize = 1 << VA_BITS; // 512 GB virtual address space
/// Trampoline 位于虚拟地址空间的顶端 - 4KB
pub const TRAMPOLINE: usize = VA_SPACE_SIZE - PAGE_SIZE + 1;
/// Trap Context 的基地址
pub const TRAP_CONTEXT_BASE: usize = TRAMPOLINE - PAGE_SIZE;
/// 用户栈的基地址，根据预留的大小计算得出
pub const UserStackBase: usize = TRAP_CONTEXT_BASE - USER_STACK_Totol_SIZE;
// /// ========================
// /// 内存与系统资源相关常量
// /// ========================
//
// /// 物理内存总容量（字节）
// /// 由平台层提供，不同 SoC / 仿真环境不同
// pub const MEMORY_SIZE: usize = platform::MEM_SIZE;
//
// /// 用户态栈大小：64 个页
// /// 64 * 4KB = 256KB
// pub const USER_STACK_SIZE: usize = PAGE_SIZE * 0x40;
//
// /// 用户态堆大小：32 个页
// /// 32 * 4KB = 128KB
// pub const USER_HEAP_SIZE: usize = PAGE_SIZE * 0x20;
//
// /// 系统允许的最大任务数量（进程/线程总和）
// pub const SYSTEM_TASK_LIMIT: usize = 128;
//
// /// 系统允许的最大文件描述符数量
// pub const SYSTEM_FD_LIMIT: usize = 256;
//
// /// ========================
// /// 页与页表相关常量
// /// ========================
//
// /// 页面大小：4KB
// pub const PAGE_SIZE: usize = 0x1000;
//
// /// 页面大小对应的位数
// /// 4096 = 2^12，因此为 12
// /// 常用于地址右移 PAGE_SIZE_BITS 得到页号
// pub const PAGE_SIZE_BITS: usize = PAGE_SIZE.trailing_zeros() as usize;
//
// /// 页表项（PTE）宽度：8 字节（64 位架构）
// pub const PTE_WIDTH: usize = 8;
//
// /// 页表项宽度对应的位数
// /// 8 = 2^3
// pub const PTE_WIDTH_BITS: usize = PTE_WIDTH.trailing_zeros() as usize;
//
// /// 页目录索引宽度
// /// 一个页表页大小为 4KB，可容纳：
// /// 4096 / 8 = 512 个页表项
// /// 512 = 2^9
// pub const DIR_WIDTH: usize = PAGE_SIZE_BITS - PTE_WIDTH_BITS;
//
// /// 内核栈页数偏移量
// /// 16.trailing_zeros() = 4
// /// 表示内核栈最多使用 2^4 = 16 页（64KB）
// pub const KSTACK_PG_NUM_SHIFT: usize = 16usize.trailing_zeros() as usize;
//
// /// 内核栈大小（平台相关）
// pub const KERNEL_STACK_SIZE: usize = platform::KERNEL_STACK_SIZE;
//
// /// 内核堆大小（平台相关）
// pub const KERNEL_HEAP_SIZE: usize = platform::KERNEL_HEAP_SIZE;
//
// /// ========================
// /// 地址长度与掩码
// /// ========================
//
// /// 物理地址长度：48 位
// pub const PALEN: usize = 48;
//
// /// 虚拟地址长度：48 位
// pub const VALEN: usize = 48;
//
// /// 虚拟地址掩码
// /// 低 48 位为 1，高位为 0
// /// 用于屏蔽符号扩展位
// pub const VA_MASK: usize = (1 << VALEN) - 1;
//
// /// 段掩码：高位（符号扩展部分）
// /// 常用于判断高半区 / 低半区地址
// pub const SEG_MASK: usize = !VA_MASK;
//
// /// 虚拟页号（VPN）对应的段掩码
// pub const VPN_SEG_MASK: usize = SEG_MASK >> PAGE_SIZE_BITS;
//
// /// ========================
// /// LoongArch 直接映射窗口
// /// ========================
//
// /// 高地址直接映射窗口（DWM）起始
// pub const HIGH_BASE_EIGHT: usize = 0x8000_0000_0000_0000;
//
// /// 低地址直接映射窗口
// pub const HIGH_BASE_ZERO: usize = 0x0000_0000_0000_0000;
//
// /// SUC（Strongly-Ordered Uncached）
// /// 直接映射窗口段号
// pub const SUC_DMW_VSEG: usize = 8;
//
// /// 当前使用的直接映射基址
// pub const MEMORY_HIGH_BASE: usize = HIGH_BASE_ZERO;
//
// /// 直接映射基址对应的虚拟页号
// pub const MEMORY_HIGH_BASE_VPN: usize = MEMORY_HIGH_BASE >> PAGE_SIZE_BITS;
//
// /// ========================
// /// 用户空间布局
// /// ========================
//
// /// 用户栈基址：
// /// 位于 TASK_SIZE 末尾，向下增长
// pub const USER_STACK_BASE: usize = TASK_SIZE - PAGE_SIZE | LA_START;
//
// /// 物理内存起始地址
// pub const MEMORY_START: usize = platform::MEM_START;
//
// /// 物理内存结束地址（不包含）
// pub const MEMORY_END: usize = MEMORY_SIZE + MEMORY_START;
//
// /// SV39 虚拟地址空间大小：2^39
// pub const SV39_SPACE: usize = 1 << 39;
//
// /// 用户空间长度（1/4 SV39）
// pub const USR_SPACE_LEN: usize = SV39_SPACE >> 2;
//
// /// LoongArch 用户虚拟地址起始
// pub const LA_START: usize = 0x1_2000_0000;
//
// /// 用户虚拟空间结束地址
// pub const USR_VIRT_SPACE_END: usize = USR_SPACE_LEN - 1;
//
// /// 信号跳板页（trampoline）
// /// 不映射在 LA 区域
// pub const TRAMPOLINE: usize = SIGNAL_TRAMPOLINE;
//
// /// 信号跳板地址
// pub const SIGNAL_TRAMPOLINE: usize = USR_VIRT_SPACE_END - PAGE_SIZE + 1;
//
// /// Trap 上下文保存区
// pub const TRAP_CONTEXT_BASE: usize = SIGNAL_TRAMPOLINE - PAGE_SIZE;
//
// /// mmap 区结束
// pub const USR_MMAP_END: usize = TRAP_CONTEXT_BASE - PAGE_SIZE;
//
// /// mmap 区起始
// pub const USR_MMAP_BASE: usize =
//     USR_MMAP_END - USR_SPACE_LEN / 8 + 0x3000;
//
// /// 任务可用最大虚拟地址
// pub const TASK_SIZE: usize =
//     USR_MMAP_BASE - USR_SPACE_LEN / 8;
//
// /// ELF 动态链接加载基址
// /// 位于用户空间高 2/3 区域
// pub const ELF_DYN_BASE: usize =
//     (((TASK_SIZE - LA_START) / 3 * 2) | LA_START)
//         & (!(PAGE_SIZE - 1));
//
// /// ========================
// /// 内核 mmap 区
// /// ========================
//
// /// 内核 mmap 起始（高半区）
// pub const MMAP_BASE: usize = 0xFFFF_FF80_0000_0000;
//
// /// 内核 mmap 结束
// pub const MMAP_END: usize = 0xFFFF_FFFF_FFFF_0000;
//
// /// 跳过页数（占位用）
// pub const SKIP_NUM: usize = 1;
//
// /// ========================
// /// 块设备与缓存
// /// ========================
//
// /// 磁盘镜像起始地址
// pub const DISK_IMAGE_BASE: usize = platform::DISK_IMAGE_BASE;
//
// /// 缓存块数量
// /// 推导：
// /// 256MB / 2KB * 4 / 2KB
// pub const BUFFER_CACHE_NUM: usize =
//     256 * 1024 * 1024 / 2048 * 4 / 2048;
//
// /// ========================
// /// 时钟
// /// ========================
//
// /// CPU 时钟频率（运行时初始化）
// pub static mut CLOCK_FREQ: usize = 0;
//
// /// ========================
// /// 信号类型
// /// ========================
//
// #[macro_export]
// macro_rules! signal_type {
//     () => {
//         u128
//     };
// }
//
// /// ========================
// /// CPU 配置寄存器访问宏
// /// ========================
//
// #[macro_export]
// macro_rules! def_cpu_cfg {
//     ($name:ident, $num: literal) => {
//         /// CPU 配置寄存器封装
//         pub struct $name {
//             bits: u32,
//         }
//
//         impl $name {
//             /// 读取 cpucfg[$num]
//             pub fn read() -> Self {
//                 let mut bits;
//                 bits = $num;
//                 unsafe {
//                     asm!(
//                         "cpucfg {},{}",
//                         out(reg) bits,
//                         in(reg) bits
//                     );
//                 }
//                 Self { bits }
//             }
//
//             /// 读取单个 bit
//             pub fn get_bit(&self, index: usize) -> bool {
//                 bit_field::BitField::get_bit(&self.bits, index)
//             }
//
//             /// 读取位段 [start, end]
//             pub fn get_bits(&self, start: usize, end: usize) -> u32 {
//                 bit_field::BitField::get_bits(&self.bits, start..=end)
//             }
//         }
//     };
// }
//
// /// CPU 配置寄存器实例
// def_cpu_cfg!(CPUCfg0, 0);
// def_cpu_cfg!(CPUCfg4, 4);
// def_cpu_cfg!(CPUCfg5, 5);
//
// impl CPUCfg0 {
//     /// 获取虚拟地址位宽
//     pub fn get_valen(&self) -> usize {
//         (self.get_bits(12, 19) + 1) as usize
//     }
//
//     /// 获取物理地址位宽
//     pub fn get_palen(&self) -> usize {
//         (self.get_bits(4, 11) + 1) as usize
//     }
// }
//
// /// ========================
// /// 工具宏
// /// ========================
//
// /// 换行符（CRLF）
// #[macro_export]
// macro_rules! newline {
//     () => {
//         "\r\n"
//     };
// }
//
// /// 是否映射 trampoline
// #[macro_export]
// macro_rules! should_map_trampoline {
//     () => {
//         false
//     };
// }
