//! 硬件抽象层 (HAL) 与板级支持包 (BSP)
//!
//! # Overview
//! - **内存布局**：定义了物理内存终点 `MEMORY_END`、页大小 `PAGE_SIZE` 以及内核/用户栈大小。
//! - **地址空间**：定义了 `TRAMPOLINE`（跳板页）和 `TRAP_CONTEXT_BASE` 等关键虚拟地址。
//! - **硬件交互**：导出串口输入输出 (`console`)、时钟管理和关机等原语。
//! - **进程切换**：导出上下文切换函数 `__switch` 和中断上下文结构 `TrapContext`。
//! # Design
//! - **体系结构解耦**：通过 `arch` 子模块隐藏不同指令集（如 RISC-V, LoongArch）在寄存器、页表结构和中断处理上的差异。
//! - **硬件平台适配**：通过 `platform` 子模块管理不同物理单板（如 QEMU 模拟器、龙芯 2K1000 开发板）的特定参数，如内存布局和外设 (MMIO) 地址。
//! - **统一接口导出**：通过 `pub use` 将常用的内核常量、类型和函数重命名并统一导出，使得上层内核模块（如内存管理、进程调度）无需关心具体底层实现。

// 导入体系结构相关的模块（如 riscv, loongarch 等）
pub mod arch;
// 导入具体平台相关的模块（如 qemu, real_board 等）
mod platform;

// --- 进程与上下文切换 ---
pub use arch::__switch; // 核心函数：实现 CPU 寄存器上下文的切换
pub use arch::kstack_alloc; // 内核栈分配函数
pub use arch::KernelStack; // 内核栈结构体类型定义
pub use arch::TrapContext; // 中断上下文结构体（保存通用寄存器等）

// --- 中断与陷阱处理 ---
pub use arch::INTR_MASKING_INFO; // 中断屏蔽相关信息（用于处理中断嵌套或优先级）
pub use arch::{bootstrap_init, machine_init}; // 系统的早期初始化和硬件初始化
pub use arch::{trap_handler, trap_return}; // 中断处理入口函数及返回函数

// --- 内存管理相关 ---
pub use arch::{PageTableEntryImpl, PageTableImpl}; // 页表项和页表的具体实现
pub use arch::{
    BLOCK_SZ,          // 磁盘块大小
    KERNEL_HEAP_SIZE,  // 内核堆空间大小
    KERNEL_STACK_SIZE, // 每个线程内核栈的大小
    MEMORY_END,        // 物理内存结束地址
    PAGE_SIZE,         // 内存页大小（通常 4KB）
    PAGE_SIZE_BITS,    // 页面大小对应的位数（如 12 位）
};

// --- 地址空间布局常量 ---
pub use arch::{
    TRAMPOLINE,        // 跳板页地址（用于用户态/内核态转换代码的映射）
    TRAP_CONTEXT_BASE, // 中断上下文在虚拟地址空间中的基地址
    USER_STACK_SIZE,   // 用户栈大小
    UserStackBase,     // 用户栈基地址
};

// --- 控制台与系统操作 ---
pub use arch::{console_flush, console_getchar, console_putchar, shutdown}; // 串口输入输出及关机
pub use arch::{get_clock_freq, get_time}; // 获取时钟频率和当前时间戳

// --- 进程地址计算助手 ---
pub use arch::{trap_cx_bottom_from_tid, ustack_bottom_from_tid}; // 根据进程 ID 计算其 Trap 上下文和用户栈的位置

// TODO：之后需要优化底层接口，去除这里的 `#[cfg(feature = "...")` 语句
// --- 针对特定架构：LoongArch (龙芯) 的额外定义 ---
#[cfg(feature = "loongarch")]
pub use arch::{
    HIGH_BASE_EIGHT,      // 高位地址映射相关常量
    MEMORY_HIGH_BASE,     // 高位内存基地址
    MEMORY_HIGH_BASE_VPN, // 高位内存基地址对应的虚页号
    MEMORY_SIZE,          // 内存总量
    PALEN,                // 物理地址长度
    VA_MASK,              // 虚拟地址掩码
    VPN_SEG_MASK,         // 虚页号分段掩码
};

// --- 针对特定板卡：LoongArch QEMU ---
#[cfg(feature = "board_laqemu")]
pub use platform::{MEM_SIZE, MMIO}; // 内存大小和内存映射 I/O 地址

// --- 针对特定板卡：RISC-V QEMU ---
#[cfg(feature = "board_rvqemu")]
pub use platform::{CLOCK_FREQ, MMIO}; // 时钟频率和内存映射 I/O 地址

// --- 针对特定板卡：龙芯 2K1000 开发板 ---
#[cfg(feature = "board_2k1000")]
pub use platform::{MEM_SIZE, MMIO};
