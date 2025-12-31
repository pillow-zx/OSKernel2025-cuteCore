mod context;
mod mem_access;

use core::arch::{asm, global_asm};
use loongArch64::register::{badi, badv, ecfg, eentry, era, estat, pgdh, tcfg};
use loongArch64::register::ecfg::LineBasedInterrupt;
use loongArch64::register::estat::{Exception, Trap};
use crate::hal::arch::loongarch::timer::TICKS_PER_SEC;
use super::merrera;
use context::{GeneralRegs};
use mem_access::Instruction;
use crate::hal::get_clock_freq;

global_asm!(include_str!("trap.S"));

extern "C" {
    pub fn __alltraps();
    pub fn __restore();
    pub fn __call_sigreturn();
    pub fn strampoline();
    pub fn __kern_trap();
}



#[allow(unused)]
#[link_section = ".text.__rfill"]
// #[naked]
#[no_mangle]
pub extern "C" fn __rfill() {
    //crmd = 0b0_01_01_10_0_00;
    //         w_dm_df_pd_i_lv;
    // let i = 0xA8;
    unsafe {
        asm!(
        // PGD: 0x1b CRMD:0x0 PWCL:0x1c TLBRBADV:0x89 TLBERA:0x8a TLBRSAVE:0x8b SAVE:0x30
        // TLBREHi: 0x8e STLBPS: 0x1e MERRsave:0x95
        "
    csrwr  $t0, 0x8b



    csrrd  $t0, 0x1b
    lddir  $t0, $t0, 3
    andi   $t0, $t0, 1
    beqz   $t0, 1f

    csrrd  $t0, 0x1b
    lddir  $t0, $t0, 3
    addi.d $t0, $t0, -1
    lddir  $t0, $t0, 1
    andi   $t0, $t0, 1
    beqz   $t0, 1f
    csrrd  $t0, 0x1b
    lddir  $t0, $t0, 3
    addi.d $t0, $t0, -1
    lddir  $t0, $t0, 1
    addi.d $t0, $t0, -1

    ldpte  $t0, 0
    ldpte  $t0, 1
    csrrd  $t0, 0x8c
    csrrd  $t0, 0x8d
    csrrd  $t0, 0x0
2:
    tlbfill
    csrrd  $t0, 0x89
    srli.d $t0, $t0, 13
    slli.d $t0, $t0, 13
    csrwr  $t0, 0x11
    tlbsrch
    tlbrd
    csrrd  $t0, 0x12
    csrrd  $t0, 0x13
    csrrd  $t0, 0x8b
    ertn
1:
    csrrd  $t0, 0x8e
    ori    $t0, $t0, 0xC
    csrwr  $t0, 0x8e

    rotri.d $t0, $t0, 61
    ori    $t0, $t0, 3
    rotri.d $t0, $t0, 3

    csrwr  $t0, 0x8c
    csrrd  $t0, 0x8c
    csrwr  $t0, 0x8d
    b      2b
",
        options(noreturn)
        )
    }
}

pub fn init() {
    set_kernel_trap_entry();
}


pub fn set_kernel_trap_entry() {
    eentry::set_eentry(__kern_trap as *const() as usize);
}


pub fn set_machine_error_trap_entry() {
    todo!()
}

pub fn enable_timer_interrupt() {
    let timer_freq = get_clock_freq();
    tcfg::set_en(true);
    tcfg::set_periodic(false);
    tcfg::set_init_val(timer_freq / TICKS_PER_SEC);
    ecfg::set_lie(LineBasedInterrupt::TIMER);
}

pub type TrapImpl = Trap;
pub fn get_exception_cause() -> TrapImpl {
    estat::read().cause()
}

pub fn get_bad_addr() -> usize {
    match get_exception_cause() {
        Trap::Exception(_) => badv::read().vaddr(),
        // INFO: npucore在这里添加了 TLBReFill 异常处理, 这里先留空
        _ => 0,
    }
}

pub fn get_bad_instruction() -> usize {
    badi::read().inst() as usize
}

pub fn get_bad_ins_addr() -> usize {
    match get_exception_cause() {
        Trap::Interrupt(_) | Trap::Exception(_) => era::read().pc(),
        // INFO: npucore在这里添加了 TLBReFill 异常处理, 这里先留空
        Trap::MachineError(_) => merrera::read().pc(),
        Trap::Unknown => 0,
    }
}

#[no_mangle]
pub extern "C" fn trap_from_kernel(gr: &mut GeneralRegs) {
    let cause = get_exception_cause();

    let sub_code = estat::read().esubcode();

    match cause {
        // npucore 中添加了 TLBReFill 异常处理, 这里先留空
        Trap::Exception(Exception::AddressNotAligned) => {
            let pc = gr.pc;
            loop {
                let ins = Instruction::from(gr.pc as *const Instruction);
                let op = ins.get_op_code();
                if op.is_err()  {
                    break;
                }
                let op = op.unwrap();
                let addr = badv::read().vaddr();
                //debug!("{:#x}: {:?}, {:#x}", pc, op, addr);
                let sz = op.get_size();
                let is_aligned: bool = addr % sz == 0;
                if is_aligned {
                    break;
                }
                assert!([2, 4, 8].contains(&sz));
                if op.is_store() {
                    let mut rd = gr[ins.get_rd_num()];
                    for i in 0..sz {
                        unsafe { ((addr + i) as *mut u8).write_unaligned(rd as u8) };
                        rd >>= 8;
                    }
                } else {
                    let mut rd = 0;
                    for i in (0..sz).rev() {
                        rd <<= 8;
                        let read_byte =
                            (unsafe { ((addr + i) as *mut u8).read_unaligned() } as usize);
                        rd |= read_byte;
                        //debug!("{:#x}, {:#x}", rd, read_byte);
                    }
                    if !op.is_unsigned_ld() {
                        match sz {
                            2 => rd = (rd as u16) as i16 as isize as usize,
                            4 => rd = (rd as u32) as i32 as isize as usize,
                            8 => rd = rd,
                            _ => unreachable!(),
                        }
                    }
                    gr[ins.get_rd_num()] = rd;
                }
                gr.pc += 4;
                break;
            }
            if gr.pc == pc {
                panic!(
                    "Failed to execute the command. Bad Instruction: {}, PC:{}",
                    unsafe { *(gr.pc as *const u32) },
                    pc
                );
            }
            //debug!("{:?}", gr);
            return;
        }
        _ => {

        }
    }
    panic!(
        "a trap {:?} from kernel! bad addr = {:#x}, bad instruction = {:#x}, pc:{:#x}, (subcode:{}), PGDH: {:?}, PGDL: {:?}, {}",
        cause,
        get_bad_addr(),
        get_bad_instruction(),
        get_bad_ins_addr(),
        sub_code,
        pgdh::read(),
        pgdh::read(),
        if let Trap::Exception(ty) = cause {
            match ty {
                Exception::FetchInstructionAddressError | Exception::MemoryAccessAddressError => match sub_code {
                    0 => "Address error Exception for Fetching instructions",
                    1 => "Address error Exception for Memory access instructions",
                    _ => "Unknown",
                },
                _ => "",
            }
        } else {
            ""
        }
    );
}

#[no_mangle]
pub fn trap_handler() -> ! {
    trap_return();
    unreachable!()
}

#[no_mangle]
pub fn trap_return() {
    todo!()
}
