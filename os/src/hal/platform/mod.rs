
#[cfg(feature = "board_rvqemu")]
pub mod riscv;

#[cfg(feature = "board_rvqemu")]
pub use riscv::qemu::*;


#[cfg(feature = "loongarch")]
pub mod loongarch;

#[cfg(feature = "board_laqemu")]
pub use loongarch::qemu::*;

#[cfg(feature = "board_la2k1000")]
pub use loongarch::la2k1000::*;