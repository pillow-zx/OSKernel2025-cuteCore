use crate::fs::inode::{FatType, OSInode};
use crate::mm::UserBuffer;
use alloc::string::String;
use core::any::Any;
use core::cell::UnsafeCell;
use fatfs::SeekFrom;

pub trait File: Send + Sync {
    // TODO：先给默认值，后续在改，否则impl File for OSInode的时候会报错
    fn readable(&self) -> bool;
    fn writable(&self) -> bool;
    fn read(&self, buf: UserBuffer) -> usize;
    fn write(&self, buf: UserBuffer) -> usize;
    fn get_stat(&self) -> UserStat;
    // 默认返回，在impl File for OSInode里会覆盖
    fn is_dir(&self) -> bool;
    fn get_path(&self) -> String;
    /// 从 offset 读取文件内容
    fn read_at(&self, offset: usize, buf: &mut [u8]) -> Result<usize, isize>;
    fn write_at(&self, offset: usize, buf: &[u8]) -> Result<usize, isize>;
    ///可以获得OsInode结构体
    fn as_any(&self) -> &dyn Any;
}

pub const S_IFREG: u32 = 0o100000; //普通文件
pub const S_IFDIR: u32 = 0o040000; //目录
pub const BLK_SIZE: u32 = 512;

pub struct Stat {
    pub st_dev: u64,
    pub st_ino: u64,
    pub st_mode: u32,
    pub st_nlink: u32,
    pub st_uid: u32,
    pub st_gid: u32,
    pub st_rdev: u64,
    pub __pad: u64,
    pub st_size: UnsafeCell<i64>, // 文件大小
    pub st_blksize: u32,
    pub __pad2: i32,
    pub st_blocks: UnsafeCell<u64>, // 占用 512B 块数
}

///由于既需要修改Stat又需要Copy特性所以分成两个了
#[repr(C)]
#[derive(Copy, Clone)]
pub struct UserStat {
    pub st_dev: u64,
    pub st_ino: u64,
    pub st_mode: u32,
    pub st_nlink: u32,
    pub st_uid: u32,
    pub st_gid: u32,
    pub st_rdev: u64,
    pub st_size: i64,
    pub st_blksize: u32,
    pub st_blocks: u64,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct LinuxDirent64 {
    ///索引节点号
    pub d_ino: u64,
    ///到下一个dirent的偏移
    pub d_off: i64,
    ///当前dirent的长度
    pub d_reclen: u16,
    ///文件类型
    pub d_type: u8,
    ///名字
    pub d_name: [u8; 256], 
}

///仅仅作为dir_list()的返回值使用，字段还是比较少的
pub struct DirEntry {
    pub d_name: String,
    pub is_dir: bool,
}
