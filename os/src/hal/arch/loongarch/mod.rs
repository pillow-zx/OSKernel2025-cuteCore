pub mod trap;
pub mod config;
pub mod sbi;
pub mod timer;
pub mod kernel_stack;
pub mod sync;
mod boot;
mod tlb;
mod merrera;
mod laflex;


use loongArch64::register::{cpuid, crmd, dmw2, ecfg, euen, misc, prcfg1, pwch, pwcl, rvacfg, stlbps, tcfg, ticlr, tlbrehi, tlbrentry, MemoryAccessType};
use loongArch64::register::ecfg::LineBasedInterrupt;
use config::{DIR_WIDTH, MMAP_BASE, PTE_WIDTH, PTE_WIDTH_BITS, SUC_DMW_VSEG, PAGE_SIZE_BITS};
use timer::get_timer_freq_first_time;
use trap::{set_kernel_trap_entry, set_machine_error_trap_entry};
use crate::hal::platform::UART_BASE;

extern "C" {
    pub fn srfill();
}

pub fn bootstrap_init() {
    if cpuid::read().core_id() != 0 {
        loop {}
    };

    ecfg::set_lie(LineBasedInterrupt::TIMER);

    euen::set_fpe(true);

    ticlr::clear_timer_interrupt();
    tcfg::set_en(false);

    crmd::set_we(false);
    crmd::set_pg(true);
    crmd::set_ie(false);

    set_kernel_trap_entry();
    set_machine_error_trap_entry();

    tlbrentry::set_tlbrentry(srfill as *const () as usize);

    dmw2::set_plv0(true);
    dmw2::set_plv1(false);
    dmw2::set_plv2(false);
    dmw2::set_plv3(false);
    dmw2::set_vseg(SUC_DMW_VSEG);
    dmw2::set_mat(MemoryAccessType::StronglyOrderedUnCached);

    // INFO: dmw3 npucore中实现了，但是新版LoongArch64库接口缺失

    stlbps::set_ps(PTE_WIDTH_BITS);
    tlbrehi::set_ps(PTE_WIDTH_BITS);

    pwcl::set_ptbase(PTE_WIDTH_BITS);
    pwcl::set_ptwidth(DIR_WIDTH);
    pwcl::set_dir1_base(PAGE_SIZE_BITS + DIR_WIDTH);
    pwcl::set_dir1_width(DIR_WIDTH);
    pwcl::set_dir2_base(0);
    pwcl::set_dir2_width(0);
    pwcl::set_ptwidth(PTE_WIDTH);

    pwch::set_dir3_base(PAGE_SIZE_BITS + DIR_WIDTH * 2);
    pwch::set_dir3_width(DIR_WIDTH);
    pwch::set_dir4_base(0);
    pwch::set_dir4_width(0);


    println!("[kernel] UART address: {:#x}", UART_BASE);
    println!("[bootstrap_init] {:?}", prcfg1::read());
}

pub fn machine_init() {
    trap::init();
    get_timer_freq_first_time();
    /* println!(
 *     "[machine_init] VALEN: {}, PALEN: {}",
 *     cfg0.get_valen(),
 *     cfg0.get_palen()
 * ); */
    for i in 0..=6 {
        let j: usize;
        unsafe { core::arch::asm!("cpucfg {0},{1}",out(reg) j,in(reg) i) };
        println!("[CPUCFG {:#x}] {}", i, j);
    }
    for i in 0x10..=0x14 {
        let j: usize;
        unsafe { core::arch::asm!("cpucfg {0},{1}",out(reg) j,in(reg) i) };
        println!("[CPUCFG {:#x}] {}", i, j);
    }
    println!("{:?}", misc::read());
    println!("{:?}", rvacfg::read());
    println!("[machine_init] MMAP_BASE: {:#x}", MMAP_BASE);

    trap::enable_timer_interrupt();
}


pub type PageTableEntryImpl = laflex::LAFlexPageTableEntry;
pub type PageTableImpl = laflex::LAFlexPageTable;








