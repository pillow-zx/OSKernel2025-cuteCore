//! ARCH 统一接口模块
//! # Overview
//! 本模块根据编译特性选择底层架构实现（RISC-V 或 LoongArch），
//! 并统一导出内核需要的接口，如启动初始化、页表、内核栈、SBI、定时器和 Trap 等。
//! 上层内核可以通过本模块访问统一的架构接口，而无需关心具体架构细节。
//!
//! # Design
//! - 使用 `#[cfg(feature = "...")]` 根据架构特性选择模块。
//! - 导出统一接口，包含以下功能模块：
//!     - `bootstrap_init` / `machine_init`：内核启动和机器初始化
//!     - `config`：页表、堆、栈、内存边界等常量
//!     - `kernel_stack`：内核栈分配和管理接口
//!     - `sbi`：控制台、关机等系统调用接口
//!     - `switch`：任务上下文切换函数
//!     - `sync`：中断屏蔽信息
//!     - `timer`：时钟和定时器接口
//!     - `trap`：TrapContext 和中断处理
//!     - 页表类型别名：`PageTableImpl` / `PageTableEntryImpl`
//!
//! # Assumptions
//! - 编译时必须指定架构特性（`riscv` 或 `loongarch`）
//! - 对应架构模块已实现完整接口，保证上层内核可透明调用
//!
//! # Safety
//! - 导出的函数涉及硬件寄存器、上下文切换、SBI 调用等操作，调用者需保证上下文合法
//! - 页表、内核栈、定时器和 Trap 操作需在允许上下文中调用
//!
//! # Invariants
//! - 上层内核调用的接口在不同架构下行为一致
//! - 导出常量和类型别名保持统一，确保代码可移植
//! - 初始化函数和中断/定时器配置按架构要求生效

#[cfg(feature = "riscv")]
pub mod riscv;

#[cfg(feature = "riscv")]
pub use riscv::{
    // 启动与初始化
    bootstrap_init,
    // 配置常量
    config::{
        BLOCK_SZ, KERNEL_HEAP_SIZE, KERNEL_STACK_SIZE, MEMORY_END, PAGE_SIZE, PAGE_SIZE_BITS,
        TRAMPOLINE, TRAP_CONTEXT_BASE, USER_STACK_SIZE,UserStackBase,
    },
    // 内核栈管理
    kernel_stack::{kstack_alloc, trap_cx_bottom_from_tid, ustack_bottom_from_tid, KernelStack},
    machine_init,
    // SBI 系统调用
    sbi::{console_flush, console_getchar, console_putchar, shutdown},
    // 任务上下文切换
    switch::__switch,
    // 中断屏蔽管理
    sync::INTR_MASKING_INFO,
    // 时钟与定时器
    timer::{get_clock_freq, get_time},
    // Trap 相关
    trap::{context::TrapContext, trap_handler, trap_return},
    // 页表类型别名
    PageTableEntryImpl,
    PageTableImpl,
};

#[cfg(feature = "loongarch")]
pub mod loongarch;

#[cfg(feature = "loongarch")]
pub use loongarch::{
    // 启动与初始化
    bootstrap_init,
    // 配置常量
    config::{
        HIGH_BASE_EIGHT, KERNEL_HEAP_SIZE, KERNEL_STACK_SIZE, MEMORY_END, MEMORY_HIGH_BASE,
        MEMORY_HIGH_BASE_VPN, MEMORY_SIZE, PAGE_SIZE, PAGE_SIZE_BITS, PALEN, TRAMPOLINE,
        TRAP_CONTEXT_BASE, USER_STACK_SIZE, VA_MASK, VPN_SEG_MASK,UserStackBase,
    },
    // 内核栈管理
    kernel_stack::{kstack_alloc, KernelStack},
    machine_init,
    // SBI 系统调用
    sbi::{console_flush, console_getchar, console_putchar, shutdown},
    // 中断屏蔽管理
    sync::INTR_MASKING_INFO,
    // 时钟与定时器
    timer::{get_clock_freq, get_time},
    // Trap 相关
    trap::{context::TrapContext, trap_handler, trap_return},
    // 页表类型别名
    PageTableEntryImpl,
    PageTableImpl,
};
