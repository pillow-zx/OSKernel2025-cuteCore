use lazy_static::lazy_static;
use riscv::register::sstatus;
use crate::sync::UPSafeCellRaw;

lazy_static! {
    pub static ref INTR_MASKING_INFO: UPSafeCellRaw<IntrMaskingInfo> =
        unsafe { UPSafeCellRaw::new(IntrMaskingInfo::new()) };
}

pub struct IntrMaskingInfo {
    nested_level: usize,
    sie_before_masking: bool,
}


impl IntrMaskingInfo {
    pub fn new() -> Self {
        Self {
            nested_level: 0,
            sie_before_masking: false,
        }
    }

    pub fn enter(&mut self) {
        let sie = sstatus::read().sie();
        unsafe {
            sstatus::clear_sie();
        }
        if self.nested_level == 0 {
            self.sie_before_masking = sie;
        }
        self.nested_level += 1;
    }

    pub fn exit(&mut self) {
        self.nested_level -= 1;
        if self.nested_level == 0 && self.sie_before_masking {
            unsafe {
                sstatus::set_sie();
            }
        }
    }
}
