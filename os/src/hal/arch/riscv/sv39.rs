use crate::mm::{FrameTracker, PhysAddr, PhysPageNum, StepByOne, VirtAddr, VirtPageNum, frame_alloc, PageTable, MapPermission};
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::arch::asm;
use bitflags::*;
use riscv::register::satp;
use crate::hal::PageTableImpl;

bitflags! {
    #[derive(Eq, PartialEq)]
    pub struct PTEFlags: u8 {
        const V = 1 << 0;
        const R = 1 << 1;
        const W = 1 << 2;
        const X = 1 << 3;
        const U = 1 << 4;
        const G = 1 << 5;
        const A = 1 << 6;
        const D = 1 << 7;
    }
}

#[derive(Copy, Clone)]
#[repr(C)]
pub struct PageTableEntry {
    pub bits: usize,
}

impl PageTableEntry {
    pub fn new(ppn: PhysPageNum, flags: PTEFlags) -> Self {
        PageTableEntry {
            bits: ppn.0 << 10 | flags.bits() as usize,
        }
    }
    pub fn empty() -> Self {
        PageTableEntry { bits: 0 }
    }
    pub fn ppn(&self) -> PhysPageNum {
        (self.bits >> 10 & ((1usize << 44) - 1)).into()
    }
    pub fn flags(&self) -> PTEFlags {
        PTEFlags::from_bits(self.bits as u8).unwrap()
    }
    pub fn is_valid(&self) -> bool {
        (self.flags() & PTEFlags::V) != PTEFlags::empty()
    }
    pub fn readable(&self) -> bool {
        (self.flags() & PTEFlags::R) != PTEFlags::empty()
    }
    pub fn writable(&self) -> bool {
        (self.flags() & PTEFlags::W) != PTEFlags::empty()
    }
    pub fn executable(&self) -> bool {
        (self.flags() & PTEFlags::X) != PTEFlags::empty()
    }
}

pub struct SV39PageTable {
    root_ppn: PhysPageNum,
    frames: Vec<FrameTracker>,
}


/// Assume that it won't oom when creating/mapping.
impl PageTable for SV39PageTable {

    fn new() -> Self {
        let frame = frame_alloc().unwrap();
        Self {
            root_ppn: frame.ppn,
            frames: vec![frame],
        }
    }

    /// Temporarily used to get arguments from user space.
    fn from_token(satp: usize) -> Self {
        Self {
            root_ppn: PhysPageNum::from(satp & ((1usize << 44) - 1)),
            frames: Vec::new(),
        }
    }

    fn find_pte_create(&mut self, vpn: VirtPageNum) -> Option<&mut PageTableEntry> {
        let idxs = vpn.indexes::<3>();
        let mut ppn = self.root_ppn;
        let mut result: Option<&mut PageTableEntry> = None;
        for (i, idx) in idxs.iter().enumerate() {
            let pte = &mut ppn.get_pte_array::<PageTableEntry>()[*idx];
            if i == 2 {
                result = Some(pte);
                break;
            }
            if !pte.is_valid() {
                let frame = frame_alloc().unwrap();
                *pte = PageTableEntry::new(frame.ppn, PTEFlags::V);
                self.frames.push(frame);
            }
            ppn = pte.ppn();
        }
        result
    }
    fn find_pte(&self, vpn: VirtPageNum) -> Option<&mut PageTableEntry> {
        let idxs = vpn.indexes::<3>();
        let mut ppn = self.root_ppn;
        let mut result: Option<&mut PageTableEntry> = None;
        for (i, idx) in idxs.iter().enumerate() {
            let pte = &mut ppn.get_pte_array::<PageTableEntry>()[*idx];
            if i == 2 {
                result = Some(pte);
                break;
            }
            if !pte.is_valid() {
                return None;
            }
            ppn = pte.ppn();
        }
        result
    }
    #[allow(unused)]
    fn map(&mut self, vpn: VirtPageNum, ppn: PhysPageNum, flags: MapPermission) {
        let pte = self.find_pte_create(vpn).unwrap();
        assert!(!pte.is_valid(), "vpn {:?} is mapped before mapping", vpn);
        *pte = PageTableEntry::new(ppn, PTEFlags::from_bits(flags.bits()).unwrap() | PTEFlags::V);
    }
    #[allow(unused)]
    fn unmap(&mut self, vpn: VirtPageNum) {
        let pte = self.find_pte(vpn).unwrap();
        assert!(pte.is_valid(), "vpn {:?} is invalid before unmapping", vpn);
        *pte = PageTableEntry::empty();
    }
    fn translate(&self, vpn: VirtPageNum) -> Option<PageTableEntry> {
        self.find_pte(vpn).map(|pte| *pte)
    }
    fn translate_va(&self, va: VirtAddr) -> Option<PhysAddr> {
        self.find_pte(va.clone().floor()).map(|pte| {
            let aligned_pa: PhysAddr = pte.ppn().into();
            let offset = va.page_offset();
            let aligned_pa_usize: usize = aligned_pa.into();
            (aligned_pa_usize + offset).into()
        })
    }
    fn activate(&self) {
        let satp = self.token();
        unsafe {
            satp::write(satp);
            asm!("sfence.vma");
        }
    }

    fn token(&self) -> usize {
        8usize << 60 | self.root_ppn.0
    }

    fn translated_byte_buffer(token: usize, ptr: *const u8, len: usize) -> Vec<&'static mut [u8]> {
        let page_table: PageTableImpl = PageTable::from_token(token);
        let mut start = ptr as usize;
        let end = start + len;
        let mut v = Vec::new();
        while start < end {
            let start_va = VirtAddr::from(start);
            let mut vpn = start_va.floor();
            let ppn = page_table.translate(vpn).unwrap().ppn();
            vpn.step();
            let mut end_va: VirtAddr = vpn.into();
            end_va = end_va.min(VirtAddr::from(end));
            if end_va.page_offset() == 0 {
                v.push(&mut ppn.get_bytes_array()[start_va.page_offset()..]);
            } else {
                v.push(&mut ppn.get_bytes_array()[start_va.page_offset()..end_va.page_offset()]);
            }
            start = end_va.into();
        }
        v
    }

    /// Load a string from other address spaces into kernel space without an end `\0`.
    fn translated_str(token: usize, ptr: *const u8) -> String {
        let page_table: PageTableImpl = PageTable::from_token(token);
        let mut string = String::new();
        let mut va = ptr as usize;
        loop {
            let ch: u8 = *(page_table
                .translate_va(VirtAddr::from(va))
                .unwrap()
                .get_mut());
            if ch == 0 {
                break;
            }
            string.push(ch as char);
            va += 1;
        }
        string
    }

    fn translated_ref<T>(token: usize, ptr: *const T) -> &'static T {
        let page_table: PageTableImpl = PageTable::from_token(token);
        page_table
            .translate_va(VirtAddr::from(ptr as usize))
            .unwrap()
            .get_ref()
    }

    fn translated_refmut<T>(token: usize, ptr: *mut T) -> &'static mut T {
        let page_table: PageTableImpl = PageTable::from_token(token);
        let va = ptr as usize;
        page_table
            .translate_va(VirtAddr::from(va))
            .unwrap()
            .get_mut()
    }
}

