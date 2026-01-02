//! 上下文切换汇编模块
//! # Overview
//! 本模块封装了内核任务上下文切换功能。
//! 使用汇编实现对 CPU 寄存器的保存和恢复，以实现任务切换。
//!
//! # Design
//! - 汇编文件 `switch.S` 提供底层实现，保存当前任务上下文到内存并加载下一个任务上下文。
//! - Rust 通过 `extern "C"` 声明函数接口，使汇编函数可在 Rust 代码中调用。
//! - 上下文切换保存的内容包括寄存器、栈指针、返回地址等，封装在 `TaskContext` 中。
//!
//! # Assumptions
//! - `TaskContext` 已正确初始化，包含完整的 CPU 寄存器状态。
//! - 上下文切换仅在允许的内核态或任务调度上下文中调用。
//!
//! # Safety
//! - `__switch` 是裸函数，直接操作寄存器和栈指针。
//! - 调用者必须保证传入的指针有效且生命周期合法。
//! - 不应在中断处理期间或未保存重要状态的情况下调用。
//!
//! # Invariants
//! - 每个任务的上下文在切换前后保持一致。
//! - 调用 `__switch` 后，当前任务上下文存储在 `current_task_cx_ptr`，下一个任务上下文加载到 CPU。
//! - 汇编实现保证寄存器和栈状态完整恢复，不破坏内核内存安全。


use crate::task::TaskContext;
use core::arch::global_asm;

// 引入汇编实现
global_asm!(include_str!("switch.S"));

extern "C" {
    /// 切换任务上下文
    ///
    /// # Arguments
    /// - `current_task_cx_ptr`：指向当前任务上下文保存位置
    /// - `next_task_cx_ptr`：指向下一个任务上下文加载位置
    ///
    /// # Safety
    /// - 直接操作寄存器和栈指针，调用者必须保证指针有效。
    /// - 不应在不安全上下文或中断中调用。
    ///
    /// # Invariants
    /// - 当前任务上下文被正确保存
    /// - 下一个任务上下文被正确加载
    pub fn __switch(current_task_cx_ptr: *mut TaskContext, next_task_cx_ptr: *const TaskContext);
}
