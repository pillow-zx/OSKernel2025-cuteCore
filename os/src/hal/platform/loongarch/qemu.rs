use crate::hal::{HIGH_BASE_EIGHT, PAGE_SIZE};

pub const MMIO: &[(usize, usize)] = &[
    (0x400E_0000, 0x1_0000),
    // (0x100E_0000, 0x0000_1000), // GED?
    // (0x1FE0_0000, 0x0000_1000), // UART
    // (0x2000_0000, 0x1000_0000), // PCI
    // (0x4000_0000, 0x0002_0000), // PCI RANGES
];

// pub const BLOCK_SZ: usize = 2048;
pub const BLOCK_SZ: usize = 4096;
pub const UART_BASE: usize = 0x1FE0_01E0 + HIGH_BASE_EIGHT;
pub const ACPI_BASE: usize = 0x100E_0000 + HIGH_BASE_EIGHT;
pub const MEM_START: usize = 0x0000_0000_8000_0000;
pub const MEM_SIZE: usize = 0x3000_0000;
pub const DISK_IMAGE_BASE: usize = 0x1800_0000 + MEM_START;
pub const KERNEL_STACK_SIZE: usize = PAGE_SIZE * 0x20;
pub const KERNEL_HEAP_SIZE: usize = PAGE_SIZE * 0x20000; // 增加到512MB