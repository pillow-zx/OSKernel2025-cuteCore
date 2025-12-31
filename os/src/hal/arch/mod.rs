#[cfg(feature = "riscv")]
pub mod riscv;

#[cfg(feature = "riscv")]
pub use riscv::{
    bootstrap_init, machine_init,
    sbi::{console_getchar, console_putchar, console_flush, shutdown},
    timer::{get_time, get_clock_freq},
    config::{USER_STACK_SIZE, KERNEL_HEAP_SIZE, KERNEL_STACK_SIZE, PAGE_SIZE, PAGE_SIZE_BITS, TRAMPOLINE, TRAP_CONTEXT_BASE, MEMORY_END},
    PageTableImpl, PageTableEntryImpl,
    kernel_stack::{KernelStack, kstack_alloc},
    trap::{trap_return, trap_handler},
    sync::INTR_MASKING_INFO,
};



#[cfg(feature = "loongarch")]
pub mod loongarch;

#[cfg(feature = "loongarch")]
pub use loongarch::{
    bootstrap_init, machine_init, PageTableImpl, PageTableEntryImpl,
    config::{
        USER_STACK_SIZE, KERNEL_HEAP_SIZE, KERNEL_STACK_SIZE, PAGE_SIZE, PAGE_SIZE_BITS, TRAMPOLINE, TRAP_CONTEXT_BASE, MEMORY_END, HIGH_BASE_EIGHT,
        MEMORY_HIGH_BASE, MEMORY_HIGH_BASE_VPN, MEMORY_SIZE, PALEN, VA_MASK, VPN_SEG_MASK
    },
    sbi::{console_getchar, console_putchar, console_flush, shutdown},
    timer::{get_time, get_clock_freq},
    kernel_stack::{kstack_alloc, KernelStack},
    trap::{trap_return, trap_handler},
    sync::INTR_MASKING_INFO,
};