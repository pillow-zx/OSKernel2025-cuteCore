pub const CLOCK_FREQ: usize = 12500000;

pub const MMIO: &[(usize, usize)] = &[
    // 前者为地址，后者为大小
    (0x1000_0000, 0x1000),
    (0x1000_1000, 0x1000),
    (0xC00_0000, 0x40_0000),
];
