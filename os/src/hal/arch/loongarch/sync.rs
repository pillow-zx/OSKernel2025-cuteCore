use lazy_static::lazy_static;
use loongArch64::register::crmd;
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
        let ie = crmd::read().ie();

        crmd::set_ie(false);

        if self.nested_level == 0 {
            self.sie_before_masking = ie;
        }

        self.nested_level += 1;
    }


    pub fn exit(&mut self) {
        self.nested_level -= 1;

        if self.nested_level == 0 && self.sie_before_masking {
            crmd::set_ie(true);
        }
    }
}
