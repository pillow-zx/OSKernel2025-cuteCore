//! Trap Context 模块
//!
//! 本模块负责管理 RISC-V 内核中任务的异常/中断上下文（Trap Context）。
//!
//! # Overview
//! - 定义通用寄存器（`GeneralRegs`）结构，保存任务寄存器状态。
//! - 定义异常上下文（`TrapContext`）结构，保存完整 CPU 状态。
//! - 提供初始化函数 `app_init_context` 用于创建用户任务上下文。
//! - 支持设置用户栈指针 (`set_sp`)。
//!
//! # Design
//! - 在发生 trap（异常或中断）时保存用户任务状态，便于异常返回。
//! - 初始化用户任务上下文，使任务可以从指定入口地址开始执行。
//! - 通过 `TrapContext` 封装寄存器、程序状态寄存器（`sstatus`）、内核页表信息和内核栈信息。
//!
//! # Assumptions
//! - `TrapContext` 中暂未包含浮点寄存器保存，如需支持需修改汇编保存/恢复逻辑。
//! - `app_init_context` 假设入口地址合法，用户栈空间已分配。
//! - `sstatus` 中 SPP 位会被设置为用户态，确保 `sret` 返回用户态。
//! - 本模块仅保存寄存器和 CPU 状态，不直接管理内存或页表。
//!
//! # Fields
//! - `GeneralRegs`：用户/内核通用寄存器状态。
//! - `TrapContext.general_regs`：保存 PC、ra、sp、t0-t6、s0-s11、a0-a7 等寄存器。
//! - `TrapContext.sstatus`：保存当前特权级及中断使能状态。
//! - `TrapContext.sepc`：异常发生的程序计数器（用户态入口地址或返回地址）。
//! - `TrapContext.kernel_satp`：内核页表基地址，用于切换页表。
//! - `TrapContext.kernel_sp`：内核栈顶地址，用于 trap 处理。
//! - `TrapContext.trap_handler`：内核异常/中断处理函数入口地址。




use riscv::register::sstatus::{read, Sstatus, SPP};


/// 通用寄存器（General Purpose Registers）
///
/// 按照 RISC-V 调用约定排列，用于保存用户/内核态的 CPU 寄存器状态。
/// 这些寄存器会在上下文切换、异常/中断处理时被保存或恢复。
///
/// 索引仅供参考，方便理解寄存器在数组或汇编代码中的顺序。
#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct GeneralRegs {
    pub pc: usize,  // 0
    pub ra: usize,  // 1
    pub sp: usize,  // 2
    pub gp: usize,  // 3
    pub tp: usize,  // 4
    pub t0: usize,  // 5
    pub t1: usize,  // 6
    pub t2: usize,  // 7
    pub s0: usize,  // 8
    pub s1: usize,  // 9
    pub a0: usize,  // 10
    pub a1: usize,  // 11
    pub a2: usize,  // 12
    pub a3: usize,  // 13
    pub a4: usize,  // 14
    pub a5: usize,  // 15
    pub a6: usize,  // 16
    pub a7: usize,  // 17
    pub s2: usize,  // 18
    pub s3: usize,  // 19
    pub s4: usize,  // 20
    pub s5: usize,  // 21
    pub s6: usize,  // 22
    pub s7: usize,  // 23
    pub s8: usize,  // 24
    pub s9: usize,  // 25
    pub s10: usize, // 26
    pub s11: usize, // 27
    pub t3: usize,  // 28
    pub t4: usize,  // 29
    pub t5: usize,  // 30
    pub t6: usize,  // 31
}

// TODO: 因为实现浮点寄存器需要修改整个汇编代码，所以暂时注释掉
//
// #[repr(C)]
// #[derive(Debug, Default, Clone, Copy)]
// pub struct FloatRegs {
//     pub f: [usize; 32],
//     pub fcsr: usize,
// }


/// 异常/中断上下文（TrapContext）
///
/// TrapContext 保存了一个任务在发生 trap（异常或中断）时的全部 CPU 状态，
/// 包括通用寄存器、程序状态寄存器、程序计数器、内核栈信息等。
///
/// 主要用途：
/// - 异常返回（sret）恢复用户态执行
/// - 用户任务上下文初始化
/// - 内核中断/异常处理时保存和恢复寄存器状态
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct TrapContext {
    /// 通用寄存器状态
    pub general_regs: GeneralRegs,

    // 如果需要保存浮点寄存器，可以启用
    // pub float_regs: FloatRegs,

    /// sstatus CSR，用于保存中断状态、特权级等
    pub sstatus: Sstatus,

    /// 异常发生时的程序计数器（用户态入口地址或异常返回地址）
    pub sepc: usize,

    /// 内核页表 SATP，用于 trap 返回时切换页表
    pub kernel_satp: usize,

    /// 内核栈顶地址
    pub kernel_sp: usize,

    /// 内核 trap 处理入口
    pub trap_handler: usize,
}

impl TrapContext {

    /// 设置用户态栈指针
    pub fn set_sp(&mut self, sp: usize) {
        self.general_regs.sp = sp;
    }


    /// 初始化用户任务上下文
    ///
    /// # 参数
    /// - `entry`：用户程序入口地址
    /// - `sp`：用户栈顶
    /// - `kernel_satp`：内核页表
    /// - `kernel_sp`：内核栈顶
    /// - `trap_handler`：内核 trap 入口
    ///
    /// # 返回
    /// 一个可用于用户任务的 `TrapContext`，已设置 sstatus 为用户态
    pub fn app_init_context(
        entry: usize,
        sp: usize,
        kernel_satp: usize,
        kernel_sp: usize,
        trap_handler: usize,
    ) -> Self {
        // 读取当前 sstatus
        let mut sstatus = read();

        // 设置 SPP 为用户态
        sstatus.set_spp(SPP::User);

        // 构造 TrapContext
        let mut cx = Self {
            general_regs: GeneralRegs::default(),
            // float_regs: FloatRegs::default(),
            sstatus,
            sepc: entry,
            kernel_satp,
            kernel_sp,
            trap_handler,
        };

        // 设置用户栈
        cx.set_sp(sp);
        cx
    }
}
