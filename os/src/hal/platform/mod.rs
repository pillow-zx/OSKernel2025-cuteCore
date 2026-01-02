//! PlatForm 统一管理模块
//! # Overview
//! 本模块根据编译特性选择实现底层运行平台（rvqemu laqemu 或 2k1000）
//!
//! # Design
//! - 使用 `#[cfg(feature = "...")]` 根据运行平台选择模块
//!
//! # Assumptions
//! - 编译时必须指定运行凭他（`rvqemu` `laqemu` 或 `2k1000`）
//!
//! # Safety
//! - 被使用的模块设计底层 MMIO，时钟等关键数据，必须保证数据真实可靠


#[cfg(feature = "riscv")]
pub mod riscv;

#[cfg(feature = "board_rvqemu")]
pub use riscv::qemu::*;

#[cfg(feature = "loongarch")]
pub mod loongarch;

#[cfg(feature = "board_laqemu")]
pub use loongarch::qemu::*;

#[cfg(feature = "board_2k1000")]
pub use loongarch::la2k1000::*;
