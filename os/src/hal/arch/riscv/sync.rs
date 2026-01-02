//! 中断屏蔽管理模块
//! # Overview
//! 本模块提供内核中断屏蔽管理功能，允许在嵌套的内核操作中安全屏蔽和恢复 S 模式下的中断。
//! 通过 `IntrMaskingInfo` 保存当前中断状态，支持嵌套屏蔽（nested masking）。
//!
//! # Design
//! - 使用 `nested_level` 记录嵌套屏蔽层数。
//! - `sie_before_masking` 记录第一次屏蔽前的 SIE（Supervisor Interrupt Enable）状态。
//! - 屏蔽中断通过清除 `sstatus.sie` 实现，恢复中断在嵌套退出最外层时按原状态恢复。
//! - 全局静态实例 `INTR_MASKING_INFO` 通过 `UPSafeCellRaw` 提供单核独占访问。
//!
//! # Assumptions
//! - 内核运行在单核（UP，Uniprocessor）模式下。
//! - 屏蔽和恢复中断操作在允许上下文执行，不会导致死锁或非法访问。
//!
//! # Safety
//! - `sstatus` 寄存器操作直接影响 CPU 中断状态，必须保证正确读写。
//! - 屏蔽中断时必须保证临界区的安全性。
//!
//! # Invariants
//! - `nested_level` 永远 >= 0。
//! - 第一次屏蔽前的 SIE 状态在嵌套退出最外层时恢复。
//! - 多次嵌套 enter/exit 保证中断状态一致。


use crate::sync::UPSafeCellRaw;
use lazy_static::lazy_static;
use riscv::register::sstatus;

lazy_static! {
    /// 全局中断屏蔽管理信息实例
    ///
    /// # Safety
    /// - 使用 `UPSafeCellRaw` 保证单核环境下的独占访问。
    pub static ref INTR_MASKING_INFO: UPSafeCellRaw<IntrMaskingInfo> =
        unsafe { UPSafeCellRaw::new(IntrMaskingInfo::new()) };
}

/// 内核中断屏蔽信息
///
/// # Fields
/// - `nested_level`：嵌套屏蔽层数
/// - `sie_before_masking`：第一次屏蔽前 SIE 寄存器状态
pub struct IntrMaskingInfo {
    nested_level: usize,
    sie_before_masking: bool,
}

impl IntrMaskingInfo {
    /// 创建新的中断屏蔽信息
    pub fn new() -> Self {
        Self {
            nested_level: 0,
            sie_before_masking: false,
        }
    }

    /// 屏蔽中断，支持嵌套
    ///
    /// # Behavior
    /// - 保存第一次屏蔽前的 SIE 状态
    /// - 清除 SIE，屏蔽中断
    /// - 嵌套调用时只增加层数，不重复保存状态
    pub fn enter(&mut self) {
        let sie = sstatus::read().sie();
        unsafe {
            sstatus::clear_sie();
        }
        if self.nested_level == 0 {
            self.sie_before_masking = sie;
        }
        self.nested_level += 1;
    }

    /// 恢复中断
    ///
    /// # Behavior
    /// - 减少嵌套层数
    /// - 当嵌套层数归零且第一次屏蔽前 SIE 为 true 时恢复中断
    pub fn exit(&mut self) {
        self.nested_level -= 1;
        if self.nested_level == 0 && self.sie_before_masking {
            unsafe {
                sstatus::set_sie();
            }
        }
    }
}
