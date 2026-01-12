use super::UserStat;
use crate::mm::UserBuffer;
use crate::sync::UPIntrFreeCell;
use alloc::string::String;
use alloc::sync::{Arc, Weak};

use crate::fs::file::BLK_SIZE;
use crate::task::suspend_current_and_run_next;

pub struct Pipe {
    readable: bool,
    writable: bool,
    buffer: Arc<UPIntrFreeCell<PipeRingBuffer>>,
    nonblocking: UPIntrFreeCell<bool>,
}

impl Pipe {
    pub fn read_end_with_buffer(buffer: Arc<UPIntrFreeCell<PipeRingBuffer>>) -> Self {
        Self {
            readable: true,
            writable: false,
            buffer,
            nonblocking: unsafe { UPIntrFreeCell::new(false) },
        }
    }
    pub fn write_end_with_buffer(buffer: Arc<UPIntrFreeCell<PipeRingBuffer>>) -> Self {
        Self {
            readable: false,
            writable: true,
            buffer,
            nonblocking: unsafe { UPIntrFreeCell::new(false) },
        }
    }

    pub fn set_nonblocking(&self, nb: bool) {
        *self.nonblocking.exclusive_access() = nb;
    }
}

const RING_BUFFER_SIZE: usize = 32;

#[derive(Copy, Clone, PartialEq)]
enum RingBufferStatus {
    Full,
    Empty,
    Normal,
}

pub struct PipeRingBuffer {
    arr: [u8; RING_BUFFER_SIZE],
    head: usize,
    tail: usize,
    status: RingBufferStatus,
    write_end: Option<Weak<Pipe>>,
}

impl PipeRingBuffer {
    pub fn new() -> Self {
        Self {
            arr: [0; RING_BUFFER_SIZE],
            head: 0,
            tail: 0,
            status: RingBufferStatus::Empty,
            write_end: None,
        }
    }
    pub fn set_write_end(&mut self, write_end: &Arc<Pipe>) {
        self.write_end = Some(Arc::downgrade(write_end));
    }
    pub fn write_byte(&mut self, byte: u8) {
        self.status = RingBufferStatus::Normal;
        self.arr[self.tail] = byte;
        self.tail = (self.tail + 1) % RING_BUFFER_SIZE;
        if self.tail == self.head {
            self.status = RingBufferStatus::Full;
        }
    }
    pub fn read_byte(&mut self) -> u8 {
        self.status = RingBufferStatus::Normal;
        let c = self.arr[self.head];
        self.head = (self.head + 1) % RING_BUFFER_SIZE;
        if self.head == self.tail {
            self.status = RingBufferStatus::Empty;
        }
        c
    }
    pub fn available_read(&self) -> usize {
        if self.status == RingBufferStatus::Empty {
            0
        } else if self.tail > self.head {
            self.tail - self.head
        } else {
            self.tail + RING_BUFFER_SIZE - self.head
        }
    }
    pub fn available_write(&self) -> usize {
        if self.status == RingBufferStatus::Full {
            0
        } else {
            RING_BUFFER_SIZE - self.available_read()
        }
    }
    pub fn all_write_ends_closed(&self) -> bool {
        self.write_end.as_ref().unwrap().upgrade().is_none()
    }
}

/// Return (read_end, write_end)
pub fn make_pipe() -> (Arc<Pipe>, Arc<Pipe>) {
    let buffer = Arc::new(unsafe { UPIntrFreeCell::new(PipeRingBuffer::new()) });
    let read_end = Arc::new(Pipe::read_end_with_buffer(buffer.clone()));
    let write_end = Arc::new(Pipe::write_end_with_buffer(buffer.clone()));
    buffer.exclusive_access().set_write_end(&write_end);
    (read_end, write_end)
}

impl super::File for Pipe {
    fn readable(&self) -> bool {
        self.readable
    }
    fn writable(&self) -> bool {
        self.writable
    }
    fn read(&self, buf: UserBuffer) -> usize {
        assert!(self.readable());
        let want_to_read = buf.len();
        let mut buf_iter = buf.into_iter();
        let mut already_read = 0usize;
        loop {
            let mut ring_buffer = self.buffer.exclusive_access();
            let loop_read = ring_buffer.available_read();
            if loop_read == 0 {
                // nonblocking: return immediately
                if *self.nonblocking.exclusive_access() {
                    return already_read;
                }
                if ring_buffer.all_write_ends_closed() {
                    return already_read;
                }
                drop(ring_buffer);
                suspend_current_and_run_next();
                continue;
            }
            for _ in 0..loop_read {
                if let Some(byte_ref) = buf_iter.next() {
                    unsafe {
                        *byte_ref = ring_buffer.read_byte();
                    }
                    already_read += 1;
                    if already_read == want_to_read {
                        return want_to_read;
                    }
                } else {
                    return already_read;
                }
            }
        }
    }
    fn write(&self, buf: UserBuffer) -> usize {
        assert!(self.writable());
        let want_to_write = buf.len();
        let mut buf_iter = buf.into_iter();
        let mut already_write = 0usize;
        loop {
            let mut ring_buffer = self.buffer.exclusive_access();
            let loop_write = ring_buffer.available_write();
            if loop_write == 0 {
                // nonblocking: return immediately
                if *self.nonblocking.exclusive_access() {
                    return already_write;
                }
                drop(ring_buffer);
                suspend_current_and_run_next();
                continue;
            }
            // write at most loop_write bytes
            for _ in 0..loop_write {
                if let Some(byte_ref) = buf_iter.next() {
                    ring_buffer.write_byte(unsafe { *byte_ref });
                    already_write += 1;
                    if already_write == want_to_write {
                        return want_to_write;
                    }
                } else {
                    return already_write;
                }
            }
        }
    }

    fn get_stat(&self) -> UserStat {
        // Return a minimal but valid stat for FIFO/pipe
        UserStat {
            st_dev: 0,
            st_ino: 0,
            // FIFO type
            st_mode: 0o010000,
            st_nlink: 1,
            st_uid: 0,
            st_gid: 0,
            st_rdev: 0,
            st_size: 0,
            st_blksize: BLK_SIZE,
            st_blocks: 0,
        }
    }

    fn is_dir(&self) -> bool {
        false
    }

    fn get_path(&self) -> String {
        String::from("pipe")
    }

    fn read_at(&self, offset: usize, buf: &mut [u8]) -> Result<usize, isize> {
        if offset != 0 { /* pipes do not support offset */ }
        let mut read_cnt = 0usize;
        let mut ring_buffer = self.buffer.exclusive_access();
        let avail = ring_buffer.available_read();
        let to_read = core::cmp::min(avail, buf.len());
        for i in 0..to_read {
            buf[i] = ring_buffer.read_byte();
            read_cnt += 1;
        }
        Ok(read_cnt)
    }

    fn write_at(&self, offset: usize, buf: &[u8]) -> Result<usize, isize> {
        if offset != 0 { /* pipes do not support offset */ }
        let mut write_cnt = 0usize;
        let mut ring_buffer = self.buffer.exclusive_access();
        let avail = ring_buffer.available_write();
        let to_write = core::cmp::min(avail, buf.len());
        for i in 0..to_write {
            ring_buffer.write_byte(buf[i]);
            write_cnt += 1;
        }
        Ok(write_cnt)
    }
}
