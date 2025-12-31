#![no_std]
#![no_main]
#![feature(panic_info_message)]
#![feature(alloc_error_handler)]
#![feature(asm_const)]

extern crate alloc;
extern crate core;

use crate::hal::shutdown;

#[macro_use]
pub mod console;
mod hal;
mod lang_items;
mod task;
mod timer;

fn clear_bss() {
    extern "C" {
        fn sbss();
        fn ebss();
    }
    unsafe {
        core::slice::from_raw_parts_mut(
            sbss as *const () as usize as *mut u8,
            ebss as *const () as usize - sbss as usize,
        )
        .fill(0);
    }
}

mod fs;
mod mm;
mod sync;
mod syscall;
mod drivers;

#[no_mangle]
pub fn rust_main() -> ! {
    clear_bss();
    hal::bootstrap_init();
    console::init();
    println!("Welcome to RustOS!");
    mm::init();
    println!("Memory management initialized.");
    hal::machine_init();
    println!("machine init completed.");
    shutdown();
}
