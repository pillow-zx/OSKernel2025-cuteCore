use crate::hal::{HIGH_BASE_EIGHT, PAGE_SIZE};

pub const MMIO: &[(usize, usize)] = &[
    (0x400E_0000, 0x1_0000)
];

pub const BLOCK_SZ: usize = 4096;
// warning: 不能移除“ + HIGH_BASE_EIGHT”，会导致开发板上地址错误
pub const UART_BASE: usize = 0x1FE2_0000 + HIGH_BASE_EIGHT;
pub const ACPI_BASE: usize = 0x1FE2_7000 + HIGH_BASE_EIGHT;
pub const MEM_START: usize = 0x0000_0000_9000_0000;
pub const MEM_SIZE: usize = 0x3000_0000;
pub const DISK_IMAGE_BASE: usize = 0x2000_0000 + MEM_START;
pub const KERNEL_STACK_SIZE: usize = PAGE_SIZE * 0x20;
pub const KERNEL_HEAP_SIZE: usize = PAGE_SIZE * 0x20000; // 增加到512MB