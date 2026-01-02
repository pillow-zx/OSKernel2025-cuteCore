//! 陷阱（Trap）处理模块。
//!
//! 本模块负责处理所有 CPU 异常（Exceptions）和中断（Interrupts）。
//! 它是内核与用户态、内核与硬件之间交互的核心通道，主要功能包括：
//! - 用户态系统调用（Syscall）的分发
//! - 用户态异常（如缺页、非法指令）的捕捉与处理
//! - 时钟中断（Timer Interrupt）的调度
//! - 内核态陷阱（Kernel Trap）的保护性处理
//!
//! # Overview
//! - `trap_handler`: 用户态进入内核态后的统一 C 入口。
//! - `trap_from_kernel`: 处理在内核态执行过程中发生的陷阱。
//! - `trap_return`: 从内核态返回用户态，执行上下文恢复及地址空间切换。
//!
//! # Control Flow
//! 1. 硬件发生 Trap，根据 `stvec` 跳转至 `trap.S` 中的汇编入口。
//! 2. 汇编代码保存寄存器现场到 `TrapContext`。
//! 3. 跳转至本模块的 `trap_handler` 或 `trap_from_kernel` 执行具体逻辑。
//! 4. 处理完成后，通过 `trap_return` 恢复现场并执行 `sret` 返回。
//!
//! # Safety
//! - 涉及大量 `unsafe` 操作，包括 CSR 寄存器读写（`stvec`, `sscratch`, `sstatus`）。
//! - 依赖 `TRAMPOLINE` 虚拟地址进行代码跳转，必须保证该内存区域在所有页表中正确映射。
//! - 处理 Trap 期间必须严格管理中断嵌套（SIE 位）。


pub mod context;

use crate::hal::TRAMPOLINE;
use crate::syscall::syscall;
use crate::task::{
    check_signals_of_current, current_add_signal, current_trap_cx, current_trap_cx_user_va,
    current_user_token, exit_current_and_run_next, suspend_current_and_run_next, SignalFlags,
};
use core::arch::{asm, global_asm};
use riscv::register::mtvec::TrapMode;
use riscv::register::scause::{Exception, Interrupt, Trap};
use riscv::register::{scause, sie, sscratch, sstatus, stval, stvec};

use crate::hal::arch::riscv::timer::set_next_trigger;
use crate::timer::check_timer;
pub use context::TrapContext;

// 引入汇编代码，包含寄存器保存与恢复的具体实现。
global_asm!(include_str!("trap.S"));


/// 初始化 Trap 模块。
///
/// 设置内核态的 Trap 入口，确保内核在执行时如果发生异常能被正确捕捉。
pub fn init() {
    set_kernel_trap_entry();
}


/// 设置内核态陷阱入口。
///
/// 将 `stvec` 指向 `__alltraps_k`，并将 `sscratch` 设置为内核 Trap 处理函数的地址。
/// 使用 Direct 模式，即所有陷阱都跳转到同一个地址。
fn set_kernel_trap_entry() {
    extern "C" {
        fn __alltraps();
        fn __alltraps_k();
    }
    let __alltraps_k_va =
        __alltraps_k as *const () as usize - __alltraps as *const () as usize + TRAMPOLINE;
    unsafe {
        stvec::write(__alltraps_k_va, TrapMode::Direct);
        sscratch::write(trap_from_kernel as usize);
    }
}


/// 处理来自内核态的陷阱。
///
/// 目前内核态仅预期处理外部中断和时钟中断。
/// 如果发生页错误或非法指令，将触发 panic。
#[no_mangle]
pub fn trap_from_kernel(_trap_cx: &TrapContext) {
    let scause = scause::read();
    let stval = stval::read();
    match scause.cause() {
        Trap::Interrupt(Interrupt::SupervisorExternal) => {
            // crate::board::irq_handler();
            // 外部中断处理逻辑（待实现）
            todo!()
        }
        Trap::Interrupt(Interrupt::SupervisorTimer) => {
            // 时钟中断：更新下次触发时间，但不立即触发调度
            set_next_trigger();
            check_timer();
            // do not schedule now
        }
        _ => {
            panic!(
                "Unsupported trap from kernel: {:?}, stval = {:#x}!",
                scause.cause(),
                stval
            );
        }
    }
}

/// 开启 S 态时钟中断
pub fn enable_timer_interrupt() {
    unsafe {
        sie::set_stimer();
    }
}

/// 开启 S 态全局中断（设置 sstatus.sie）
fn enable_supervisor_interrupt() {
    unsafe {
        sstatus::set_sie();
    }
}

/// 关闭 S 态全局中断（清除 sstatus.sie）
fn disable_supervisor_interrupt() {
    unsafe {
        sstatus::clear_sie();
    }
}

/// 设置用户态陷阱入口。
///
/// 当 CPU 运行在用户态时，`stvec` 应指向映射在 `TRAMPOLINE` 地址处的汇编入口。
fn set_user_trap_entry() {
    unsafe {
        stvec::write(TRAMPOLINE, TrapMode::Direct);
    }
}


/// 用户态 Trap 的总调度器。
///
/// 处理过程：
/// 1. 切换 `stvec` 为内核陷阱入口，防止内核异常丢失。
/// 2. 读取 `scause` 分析陷阱原因。
/// 3. 根据原因进行分发（系统调用、内存错误、时钟中断等）。
/// 4. 检查当前进程的信号状态，决定是否退出。
/// 5. 调用 `trap_return` 回到用户态。
#[no_mangle]
pub fn trap_handler() -> ! {
    set_kernel_trap_entry();
    let scause = scause::read();
    let stval = stval::read();
    match scause.cause() {
        // 系统调用
        Trap::Exception(Exception::UserEnvCall) => {
            let mut cx = current_trap_cx();
            cx.sepc += 4; // 系统调用返回后执行下一条指令

            // 系统调用处理期间允许嵌套中断（提高内核响应性）
            enable_supervisor_interrupt();

            let result = syscall(
                cx.general_regs.a7,
                [cx.general_regs.a0, cx.general_regs.a1, cx.general_regs.a2],
            );

            // 重新获取上下文，因为任务可能在 syscall 期间被调度
            cx = current_trap_cx();
            cx.general_regs.a0 = result as usize;
        }
        // 内存访问违例
        Trap::Exception(Exception::StoreFault)
        | Trap::Exception(Exception::StorePageFault)
        | Trap::Exception(Exception::LoadFault)
        | Trap::Exception(Exception::LoadPageFault)
        | Trap::Exception(Exception::InstructionFault)
        | Trap::Exception(Exception::InstructionPageFault) => {
            current_add_signal(SignalFlags::SIGSEGV);
        }
        // 非法指令
        Trap::Exception(Exception::IllegalInstruction) => {
            current_add_signal(SignalFlags::SIGILL);
        }
        // 时钟中断
        Trap::Interrupt(Interrupt::SupervisorTimer) => {
            set_next_trigger();
            check_timer();
            suspend_current_and_run_next();
        }
        _ => {
            panic!(
                "Unsupported trap from user: {:?}, stval = {:#x}!",
                scause.cause(),
                stval
            );
        }
    }
    // 检查并处理信号，如进程因异常需要退出
    if let Some((errno, msg)) = check_signals_of_current() {
        println!("[kernel] {}", msg);
        exit_current_and_run_next(errno);
    }
    trap_return();
}


/// 返回用户态。
///
/// 该函数完成最后的环境切换：
/// 1. 设置 `stvec` 指向用户态入口。
/// 2. 关闭内核中断。
/// 3. 获取用户态页表 token 和 Trap 上下文在用户空间的虚地址。
/// 4. 跳转至汇编代码 `__restore` 进行寄存器恢复、页表切换及 `sret`。
pub fn trap_return() -> ! {
    disable_supervisor_interrupt();
    set_user_trap_entry();
    let trap_cx_user_va = current_trap_cx_user_va();
    let user_satp = current_user_token();
    extern "C" {
        fn __alltraps();
        fn __restore();
    }

    // 计算 __restore 的虚拟地址
    let restore_va =
        __restore as *const () as usize - __alltraps as *const () as usize + TRAMPOLINE;
    unsafe {
        asm!(
            "fence.i",                          // 清刷指令缓存，确保指令一致性
            "jr {restore_va}",                  // 跳转到恢复现场的代码位置
            restore_va = in(reg) restore_va,
            in("a0") trap_cx_user_va,
            in("a1") user_satp,
            options(noreturn)
        )
    }
}
