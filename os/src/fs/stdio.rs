use crate::fs::file::{Stat, UserStat};
use super::File;
use crate::hal::console_getchar;
use crate::mm::UserBuffer;

pub struct Stdin;
pub struct Stdout;

impl File for Stdin {
    fn readable(&self) -> bool {
        true
    }
    fn writable(&self) -> bool {
        false
    }
    fn read(&self, mut user_buf: UserBuffer) -> usize {
        assert_eq!(user_buf.len(), 1);

        // 根据 sbi 接口规定，若无输入则返回 usize::MAX
        let ch = loop {
            let c = console_getchar();
            if c != usize::MAX {
                break c;
            }
        };
        unsafe {
            user_buf.buffers[0].as_mut_ptr().write_volatile(ch as u8);
        }
        1
    }
    fn write(&self, _user_buf: UserBuffer) -> usize {
        panic!("Cannot write to stdin!");
    }

    fn get_stat(&self) -> UserStat {
        todo!()
    }
    fn read_at(&self, offset: usize, buf: &mut [u8]) -> Result<usize, isize> {
        todo!()
    }
    fn write_at(&self, offset: usize, buf: &[u8]) -> Result<usize, isize> {
        todo!()
    }
}

impl File for Stdout {
    fn readable(&self) -> bool {
        false
    }
    fn writable(&self) -> bool {
        true
    }
    fn read(&self, _user_buf: UserBuffer) -> usize {
        panic!("Cannot read from stdout!");
    }
    fn write(&self, user_buf: UserBuffer) -> usize {
        for buffer in user_buf.buffers.iter() {
            print!("{}", core::str::from_utf8(*buffer).unwrap());
        }
        user_buf.len()
    }

    fn get_stat(&self) -> UserStat {
        todo!()
    }
    fn read_at(&self, offset: usize, buf: &mut [u8]) -> Result<usize, isize> {
        todo!()
    }
    fn write_at(&self, offset: usize, buf: &[u8]) -> Result<usize, isize> {
        todo!()
    }
}
