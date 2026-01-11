use crate::fs::fat32::FAT_FS;
use crate::fs::file::{Stat, UserStat, BLK_SIZE};
use crate::fs::{DirEntry, FatFsBlockDevice};
use crate::mm::UserBuffer;
use crate::sync::UPIntrFreeCell;
use crate::syscall::StatMode;
use crate::task::current_process;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use bitflags::bitflags;
use core::any::Any;
use core::cell::UnsafeCell;
use fatfs::{
    DefaultTimeProvider, Dir, File, FileSystem, LossyOemCpConverter, Read, Seek, SeekFrom, Write,
};
use lazy_static::lazy_static;

pub struct OSInode {
    readable: bool,
    writable: bool,
    stat: Stat,
    // 未来如果需要支持多核，则需要改用更强的同步机制（如 spin::Mutex）。
    file: UPIntrFreeCell<FatType>,
    pub is_directory: bool, // 是否是目录
    path: String,           // 文件的完整路径
}

pub enum FatType {
    //底层通过 FatFsBlockDevice 访问磁盘
    // 使用 DefaultTimeProvider 提供时间
    // 使用 LossyOemCpConverter 处理文件名
    File(File<'static, FatFsBlockDevice, DefaultTimeProvider, LossyOemCpConverter>),
    Dir(Dir<'static, FatFsBlockDevice, DefaultTimeProvider, LossyOemCpConverter>),
}

// 理由：在单核环境下，UPIntrFreeCell 通过屏蔽中断保证了原子性。
// 即使 fatfs::File 本身不是 Send/Sync，但由于保证了同一时间
// 只有一个内核任务能通过该 Cell 访问它，所以可以安全地在任务间转移它。
// 单核 + 中断屏蔽 + 同一时间只有一个任务访问
unsafe impl Send for OSInode {}
unsafe impl Sync for OSInode {}

impl OSInode {
    pub fn new(readable: bool, writable: bool, file: FatType, is_dir: bool, path: String) -> Self {
        let mut st_mode = if is_dir { 0o040000 } else { 0o100000 }; // S_IFDIR / S_IFREG
        if readable {
            st_mode |= 0o444
        } // r--
        if writable {
            st_mode |= 0o222
        } // -w-

        let st_size = 0;
        let st_blocks = ((st_size + 511) / 512) as u64;
        let is_directory = is_dir;
        Self {
            readable,
            writable,
            stat: Stat {
                st_dev: 0,
                st_ino: 0, // 或者生成伪 inode
                st_mode,
                st_nlink: 1,
                st_uid: 0,
                st_gid: 0,
                st_rdev: 0,
                __pad: 0,
                st_size: UnsafeCell::new(st_size),
                st_blksize: BLK_SIZE,
                __pad2: 0,
                st_blocks: UnsafeCell::new(st_blocks),
            },
            file: unsafe { UPIntrFreeCell::new(file) },
            is_directory,
            path,
        }
    }

    /// 当前 read_all 时从 offset 到 EOF 而不是从文件开始到 EOF
    /// 把注释部分取消则从文件开始到 EOF
    pub fn read_all(&self) -> Vec<u8> {
        let mut inner = self.file.exclusive_access();
        let mut buffer = [0u8; 512];
        let mut v: Vec<u8> = Vec::new();
        match &mut *inner {
            FatType::File(file) => {
                // file.seek(SeekFrom::Start(0)).unwrap();
                loop {
                    let len = file.read(&mut buffer);
                    let size = len.unwrap();
                    if size == 0 {
                        break;
                    }
                    v.extend_from_slice(&buffer[..size]);
                }
            }
            FatType::Dir(_) => {
                log::debug!("Get a Dir to read, which is not supported");
            }
        }
        v
    }
    pub fn is_dir(&self) -> bool {
        let inner = self.file.exclusive_access();
        match *inner {
            FatType::Dir(_) => true,
            FatType::File(_) => false,
        }
    }
}

lazy_static! {
    pub static ref ROOT_DIR: UPIntrFreeCell<Dir<'static, FatFsBlockDevice, DefaultTimeProvider, LossyOemCpConverter>> = {
        // 获取文件系统的锁
        let fs_guard = FAT_FS.lock();
        // 关键点：fatfs 的 root_dir() 会借用 FileSystem。
        // 在 static 初始化块中，需要确保引用的合法性。
        let fs_static: &'static FileSystem<FatFsBlockDevice, DefaultTimeProvider, LossyOemCpConverter> =
            unsafe { &*(fs_guard.deref() as *const _) };

        let root_dir = fs_static.root_dir();
        unsafe {
            UPIntrFreeCell::new(root_dir)
        }
    };
}

pub fn list_apps() {
    println!("List of applications:");
    for entry in ROOT_DIR.exclusive_access().iter() {
        let entry = entry.expect("Failed to read directory entry");
        let file_name = entry.file_name();
        let attributes = if entry.is_dir() { "DIR" } else { "FILE" };
        let size = entry.len();
        println!(
            "[[{}]], FileName: {}, Size: {}",
            attributes, file_name, size
        );
    }
}

bitflags! {
    pub struct OpenFlags: u32 {
        // 只读
        const RDONLY = 0;
        // 只写
        const WRONLY = 1 << 0;
        // 读写
        const RDWR = 1 << 1;
        // 创建
        const CREATE = 1 << 6;
        // 截断（若存在则以可写方式打开，但是长度清空为0）
        const TRUNC = 1 << 10;
        //
        const DIRECTORY = 1 << 21;
    }
}

impl OpenFlags {
    pub fn read_write(&self) -> (bool, bool) {
        if self.contains(Self::WRONLY) {
            (false, true)
        } else if self.contains(Self::RDWR) {
            (true, true)
        } else {
            (true, false)
        }
    }
}

impl super::File for OSInode {
    fn readable(&self) -> bool {
        self.readable
    }

    fn writable(&self) -> bool {
        self.writable
    }

    fn read(&self, mut buf: UserBuffer) -> usize {
        let mut inner = self.file.exclusive_access();
        let mut total_read_size = 0usize;
        for slice in buf.buffers.iter_mut() {
            match &mut *inner {
                FatType::File(file) => {
                    let read_size = file.read(slice).unwrap();
                    total_read_size += read_size;
                    if read_size < slice.len() {
                        break;
                    }
                }
                FatType::Dir(_) => {
                    log::debug!("Get a Dir to read, which is not supported");
                }
            }
        }
        total_read_size
    }

    fn write(&self, buf: UserBuffer) -> usize {
        let mut inner = self.file.exclusive_access();
        let mut total_write_size = 0usize;
        for slice in buf.buffers.iter() {
            match &mut *inner {
                FatType::File(file) => {
                    let write_size = file.write(slice).unwrap();
                    total_write_size += write_size;
                    if write_size < slice.len() {
                        break;
                    }
                }
                FatType::Dir(_) => {
                    log::debug!("Get a Dir to write, which is not supported");
                }
            }
        }

        self.stat.update_after_write(total_write_size);
        total_write_size
    }
    fn get_stat(&self) -> UserStat {
        unsafe {
            UserStat {
                st_dev: self.stat.st_dev,
                st_ino: self.stat.st_ino,
                st_mode: self.stat.st_mode,
                st_nlink: self.stat.st_nlink,
                st_uid: self.stat.st_uid,
                st_gid: self.stat.st_gid,
                st_rdev: self.stat.st_rdev,
                st_size: *self.stat.st_size.get(),
                st_blksize: self.stat.st_blksize,
                st_blocks: *self.stat.st_blocks.get(),
            }
        }
    }

    fn is_dir(&self) -> bool {
        self.is_directory
    }

    fn get_path(&self) -> String {
        self.path.clone()
    }

    /// 从 offset 读取文件内容
    fn read_at(&self, offset: usize, buf: &mut [u8]) -> Result<usize, isize> {
        // 获取内部可变访问权限
        let mut inner = self.file.exclusive_access();
        match &mut *inner {
            FatType::File(file) => {
                // fatfs::File 需要 &mut 来读写
                let mut file_ref = file; // File 类型本身可能在 UPIntrFreeCell 内部

                // seek 到 offset
                file_ref
                    .seek(SeekFrom::Start(offset as u64))
                    .map_err(|_| -1isize)?;
                // 读取数据
                let n = file_ref.read(buf).map_err(|_| -1isize)?;
                Ok(n)
            }
            FatType::Dir(_) => Err(-1),
        }
    }

    fn write_at(&self, offset: usize, buf: &[u8]) -> Result<usize, isize> {
        let mut inner = self.file.exclusive_access();
        match &mut *inner {
            FatType::File(file) => {
                let mut file_ref = file;
                // seek 到 offset
                file_ref
                    .seek(SeekFrom::Start(offset as u64))
                    .map_err(|_| -1isize)?;
                // 写入数据
                let n = file_ref.write(buf).map_err(|_| -1isize)?;

                // 更新 stat
                let file_size = file_ref.seek(SeekFrom::End(0)).map_err(|_| -1isize)? as i64;
                unsafe {
                    *self.stat.st_size.get() = file_size;
                }
                unsafe {
                    *self.stat.st_blocks.get() = ((file_size as usize + 511) / 512) as u64;
                }
                drop(self.file.exclusive_access());
                Ok(n)
            }
            FatType::Dir(_) => Err(-1),
        }
    }
    ///可以直接获得OsInode结构体
    fn as_any(&self) -> &dyn Any {
        self
    }
}
impl OSInode {
    pub fn list_dir(&self) -> Result<Vec<DirEntry>, isize> {
        if !self.is_directory {
            return Err(-1); // ENOTDIR
        }

        let inner = self.file.exclusive_access();

        match &*inner {
            FatType::Dir(dir) => {
                let mut v = Vec::new();
                for entry in dir.iter() {
                    let entry = entry.map_err(|_| -1isize)?;
                    v.push(DirEntry {
                        d_name: entry.file_name(),
                        is_dir: entry.is_dir(),
                    });
                }
                Ok(v)
            }
            _ => Err(-1),
        }
    }
}

impl DirEntry {}

impl Stat {
    pub fn update_after_write(&self, written: usize) {
        unsafe {
            // 累加写入字节数
            *self.st_size.get() += written as i64;
            let size = *self.st_size.get();
            // 向上取整 512B 块
            *self.st_blocks.get() = ((size as usize + 511) / 512) as u64;
        }
    }
}

///返回绝对路径，支持相对路径
pub fn resolve_path(relative: &str, base: &str) -> String {
    let mut stack: Vec<&str> = Vec::new();

    let is_absolute = relative.starts_with("/");

    if !is_absolute {
        for component in base.split("/") {
            match component {
                "" | "." => continue,
                ".." => {
                    if !stack.is_empty() {
                        stack.pop();
                    }
                }
                _ => stack.push(component),
            }
        }
    }

    for component in relative.split("/") {
        match component {
            "" | "." => continue,
            ".." => {
                if !stack.is_empty() {
                    stack.pop();
                }
            }
            _ => stack.push(component),
        }
    }

    let mut result = String::from("/");
    result.push_str(&stack.join("/"));
    result
}

pub fn open_initproc(flags: OpenFlags) -> Option<Arc<OSInode>> {
    let (readable, writable) = flags.read_write();
    let root_dir = ROOT_DIR.exclusive_access();
    root_dir.open_file("initproc").ok().map(|inode| {
        Arc::new(OSInode::new(
            readable,
            writable,
            FatType::File(inode),
            false,
            String::from("/initproc"),
        ))
    })
}

// 实现不完整，还未支持文件的所有权描述
pub fn open_file(path: &str, flags: OpenFlags) -> Option<Arc<OSInode>> {
    let (readable, writable) = flags.read_write();

    let full_path = {
        let proc = current_process();
        let inner = proc.inner_exclusive_access();
        let cwd = &inner.cwd;
        resolve_path(path, &cwd)
    };

    let path_in_fs = full_path.strip_prefix("/").unwrap_or(&full_path);

    let root_dir = ROOT_DIR.exclusive_access();

    let maybe_inode = if flags.contains(OpenFlags::CREATE) {
        root_dir
            .open_file(path_in_fs)
            .or_else(|_| root_dir.create_file(path_in_fs))
            .ok()
    } else {
        root_dir.open_file(path_in_fs).ok()
    };

    maybe_inode.map(|mut inode| {
        if flags.contains(OpenFlags::TRUNC) {
            inode.truncate().expect("Truncation failed");
        }
        Arc::new(OSInode::new(
            readable,
            writable,
            FatType::File(inode),
            false,
            full_path, // 传入完整路径
        ))
    })
}

/// 在指定目录下打开文件
pub fn open_file_at(
    base_dir: &str,
    path: &str,
    flags: OpenFlags,
    mode: StatMode,
) -> Option<Arc<OSInode>> {
    let full_path = resolve_path(path, base_dir);
    if full_path == "/" {
        return Some(current_root_inode());
    }
    let root_dir = ROOT_DIR.exclusive_access();

    // 尝试打开目录
    if let Ok(dir) = root_dir.open_dir(&full_path) {
        return Some(Arc::new(OSInode::new(
            true,  // 可读
            false, // 不可写
            FatType::Dir(dir),
            true, // 是目录
            full_path,
        )));
    }

    // 尝试打开或创建文件
    let file_result = if flags.contains(OpenFlags::CREATE) {
        root_dir
            .create_file(&full_path)
            .or_else(|_| root_dir.open_file(&full_path))
    } else {
        root_dir.open_file(&full_path)
    };

    file_result.ok().map(|file| {
        Arc::new(OSInode::new(
            flags.contains(OpenFlags::RDONLY) || flags.contains(OpenFlags::RDWR),
            flags.contains(OpenFlags::WRONLY) || flags.contains(OpenFlags::RDWR),
            FatType::File(file),
            false, // 不是目录
            full_path,
        ))
    })
}

///创建目录，如果存在就返回err(-1)
pub fn create_dir(path: &str) -> Result<Arc<OSInode>, isize> {
    // 1. 解析完整路径（基于 cwd）
    let full_path = {
        let proc = current_process();
        let inner = proc.inner_exclusive_access();
        resolve_path(path, &inner.cwd)
    };

    // 去掉前导 '/'
    let path_in_fs = full_path.strip_prefix("/").unwrap_or(&full_path);

    // 2. 拆分 parent 和 dir name
    let (parent_path, dir_name) = match path_in_fs.rsplit_once('/') {
        Some((p, n)) => (p, n),
        None => ("", path_in_fs), // 位于根目录
    };

    if dir_name.is_empty() {
        return Err(-1);
    }

    let root_dir = ROOT_DIR.exclusive_access();

    // 3. 打开父目录
    let mut parent_dir = if parent_path.is_empty() {
        root_dir.clone()
    } else {
        root_dir.open_dir(parent_path).map_err(|_| -1isize)?
    };

    // 4. 如果已存在，报错
    if parent_dir.open_dir(dir_name).is_ok() {
        return Err(-1); // EEXIST
    }

    // 5. 创建目录
    let dir = parent_dir.create_dir(dir_name).map_err(|_| -1isize)?;

    // 6. 封装成 OSInode
    Ok(Arc::new(OSInode::new(
        true,  // readable（目录可读）
        false, // writable（fatfs 不支持写目录内容）
        FatType::Dir(dir),
        true, // is_directory
        full_path,
    )))
}

/// 打开目录，返回 OSInode
/// path 可以是绝对路径或相对路径
/// 返回 Err(-1) 表示打开失败
pub fn open_dir(path: &str) -> Result<Arc<OSInode>, isize> {
    let full_path = {
        let proc = current_process();
        let inner = proc.inner_exclusive_access();
        resolve_path(path, &inner.cwd)
    };

    let path_in_fs = full_path.strip_prefix("/").unwrap_or(&full_path);
    let root_dir = ROOT_DIR.exclusive_access();

    root_dir
        .open_dir(path_in_fs)
        .map(|dir| {
            Arc::new(OSInode::new(
                true,
                false,
                FatType::Dir(dir),
                true,
                full_path,
            ))
        })
        .map_err(|_| -1)
}

pub fn get_size<IO: fatfs::ReadWriteSeek, TP: fatfs::TimeProvider, OCC: fatfs::OemCpConverter>(
    f: &mut fatfs::File<IO, TP, OCC>,
) -> i64 {
    // 保存当前文件偏移
    let cur = f.seek(SeekFrom::Current(0)).unwrap();
    // 跳到文件末尾，返回的位置就是文件大小
    let size = f.seek(SeekFrom::End(0)).unwrap();
    // 恢复原来的偏移
    f.seek(SeekFrom::Start(cur)).unwrap();
    size as i64
}

pub fn current_root_inode() -> Arc<OSInode> {
    let root_dir = ROOT_DIR.exclusive_access();
    Arc::new(OSInode::new(
        true,
        false,
        FatType::Dir(root_dir.clone()),
        true,
        String::from("/"),
    ))
}
