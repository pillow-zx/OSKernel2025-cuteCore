//! 内核架构初始化模块(RISC-V)
//! # Overview
//! 本模块负责在操作系统内核启动阶段进行架构相关的初始化工作。
//! 包含任务如中断初始化、时钟中断配置，以及类型别名定义以统一页表接口。
//!
//! # Design
//! - `bootstrap_init()`：在 kernel 启动阶段根据架构特点进行初始化。RISC-V 架构不需特殊处理，函数为空。
//! - `machine_init()`：初始化机器相关部分，设置中断处理函数和定时器中断。
//! - 通过 `trap::init()` 初始化中断向量。
//! - 通过 `trap::enable_timer_interrupt()` 启用时钟中断。
//! - 通过 `set_next_trigger()` 设置下一次定时器触发。
//! - 提供类型别名 `PageTableImpl` 和 `PageTableEntryImpl`，统一上层内核页表接口。
//!
//! # Assumptions
//! - 系统运行在 RISC-V 架构。
//! - 中断初始化函数和定时器函数可正确配置硬件。
//! - 页表类型 `SV39PageTable` 与条目 `PageTableEntry` 符合上层内核接口。
//!
//! # Safety
//! - 中断初始化和定时器触发涉及硬件寄存器操作，必须在允许上下文调用。
//! - bootstrap_init() 为空函数，不产生副作用。
//!
//! # Invariants
//! - 初始化完成后，内核能够正确接收时钟中断。
//! - 页表类型别名保持与 SV39 页表实现一致，保证统一接口。

use crate::hal::arch::riscv::timer::set_next_trigger;

pub mod boot;
pub mod config;
pub mod kernel_stack;
pub mod sbi;
pub mod sv39;
pub mod switch;
pub mod sync;
pub mod timer;
pub mod trap;

/// 内核启动阶段架构相关初始化
///
/// # Overview
/// - RISC-V 架构无需特殊处理，故为空函数
pub fn bootstrap_init() {}

/// 初始化机器相关部分
///
/// # Overview
/// - 初始化中断处理函数
/// - 启用时钟中断
/// - 设置下一次定时器触发
pub fn machine_init() {
    trap::init();
    trap::enable_timer_interrupt();
    set_next_trigger();
}

/// 页表实现类型别名
///
/// # Overview
/// - 统一内核上层使用接口，实际使用 SV39 页表
pub type PageTableImpl = sv39::SV39PageTable;

/// 页表条目实现类型别名
///
/// # Overview
/// - 统一内核上层使用接口，实际使用 SV39 页表条目
pub type PageTableEntryImpl = sv39::PageTableEntry;
