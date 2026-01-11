use crate::fs::inode::{create_dir, OSInode};
use crate::fs::{
    open_dir, open_file, open_file_at, resolve_path, File, LinuxDirent64, OpenFlags, UserStat,
};
use crate::mm::{copy_to_user, translated_byte_buffer, translated_str, UserBuffer};
use crate::task::{current_process, current_task, current_user_token};
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use bitflags::bitflags;
use log::info;

pub const AT_FDCWD: usize = 100usize.wrapping_neg();

// 已实现
// pub fn sys_getcwd(buf: *const u8, len: usize) -> *const u8 {
//     let token = current_user_token();
//     let process = current_process();
//     let inner = process.inner_exclusive_access();
//     let cwd = &inner.cwd;
//     if cwd.len() + 1 > len {
//         return core::ptr::null();
//     }
//     let mut buffer = UserBuffer::new(translated_byte_buffer(token, buf, len));
//     buffer.write_string(cwd);
//     buf
// }

pub fn sys_getcwd(buf: *const u8, len: usize) -> isize {
    let token = current_user_token();
    let process = current_process();
    let inner = process.inner_exclusive_access();
    let cwd = &inner.cwd;
    if cwd.len() + 1 > len {
        // return core::ptr::null();
        return -34;
    }
    let mut buffer = UserBuffer::new(translated_byte_buffer(token, buf, len));
    buffer.write_string(cwd);
    buf as isize
}

// cwd_inode更新逻辑，如果能打不开文件就崩溃，初始化为根目录
pub fn sys_chdir(path: *const u8) -> isize {
    let token = current_user_token();
    let path = translated_str(token, path);

    //  计算新的 cwd（不打开目录）
    let new_cwd: String = {
        let process = current_process();
        let inner = process.inner_exclusive_access();
        resolve_path(path.as_str(), inner.cwd.as_str())
    }; // inner 在这里自动 drop

    //  验证目录是否存在
    let inode = match open_dir(new_cwd.as_str()) {
        Ok(inode) => inode,
        Err(_) => return -1, // ENOENT / ENOTDIR
    };

    //  写回 PCB
    let process = current_process();
    let mut inner = process.inner_exclusive_access();
    inner.cwd = new_cwd;
    inner.cwd_inode = inode;

    0
}

pub fn sys_mkdirat(dirfd: isize, path: *const u8, mode: u32) -> isize {
    let token = current_user_token();
    let path = translated_str(token, path);

    let process = current_process();
    let inner = process.inner_exclusive_access();

    //  base path
    let base_path = if path.starts_with("/") {
        "/".to_string()
    } else if dirfd == AT_FDCWD as isize {
        inner.cwd.clone()
    } else {
        // dirfd 必须是合法 fd
        let fd = match inner.fd_table.get(dirfd as usize) {
            Some(Some(inode)) => inode.clone(),
            _ => return -1, // EBADF
        };

        // dirfd 必须指向目录
        if !fd.is_dir() {
            return -1; // ENOTDIR
        }

        fd.get_path()
    };
    drop(inner);
    //  拼接最终路径
    let full_path = resolve_path(&path, &base_path);

    // 创建目录
    match create_dir(&full_path) {
        Ok(_) => 0,
        Err(_) => {
            println!("[sys_mkdirat]Failed to create directory: {},Maybe existed", &full_path);
            -1
        },
    }
}
///复制文件描述符
pub fn sys_dup(fd: usize) -> isize {
    let process = current_process();
    let mut inner = process.inner_exclusive_access();

    // fd 合法性
    if fd >= inner.fd_table.len() {
        return -1;
    }

    let file = match inner.fd_table[fd].as_ref() {
        Some(f) => f.clone(), // Arc clone
        None => return -1,
    };

    // 找最小可用 fd
    let new_fd = inner
        .fd_table
        .iter()
        .position(|f| f.is_none())
        .unwrap_or(inner.fd_table.len());

    //  插入
    if new_fd == inner.fd_table.len() {
        inner.fd_table.push(Some(file));
    } else {
        inner.fd_table[new_fd] = Some(file);
    }

    new_fd as isize
}

///复制文件描述符，并指定新的文件描述符
/// 后续需要添加flags相关的操作，目前测试文件只有dup2
pub fn sys_dup3(old_fd: usize, new_fd: usize, flags: usize) -> isize {
    //  flags 校验（最小实现）
    if flags != 0 {
        return -1;
    }

    let process = current_process();
    let mut inner = process.inner_exclusive_access();

    //  old_fd 合法性
    if old_fd >= inner.fd_table.len() {
        return -1;
    }

    let file = match inner.fd_table[old_fd].as_ref() {
        Some(f) => f.clone(),
        None => return -1,
    };

    //  dup3 特有规则：old == new → EINVAL
    if old_fd == new_fd {
        return -1;
    }

    //  扩展 fd_table
    if new_fd >= inner.fd_table.len() {
        inner.fd_table.resize(new_fd + 1, None);
    }

    //  若 new_fd 已打开，先 close
    inner.fd_table[new_fd] = None;

    //  复制 fd
    inner.fd_table[new_fd] = Some(file);

    new_fd as isize
}

pub fn sys_getdents64(fd: usize, buf: *mut u8, len: usize) -> isize {
    let process = current_process();
    let inner = process.inner_exclusive_access();
    // fd 校验
    if fd >= inner.fd_table.len() {
        return -1;
    }
    let file = match inner.fd_table[fd].as_ref() {
        Some(f) => f.clone(),
        None => return -1,
    };
    // 必须是目录
    if !file.is_dir() {
        return -1;
    }
    // 至少能放下一个 dirent
    if len < core::mem::size_of::<LinuxDirent64>() {
        return -1;
    }
    drop(inner);
    //  读取目录
    let dir = file.as_any().downcast_ref::<OSInode>(); // Vec<String>
    let dir_inode = match dir {
        Some(dir) => dir,
        None => return -1,
    };
    let entries = match dir_inode.list_dir() {
        Ok(entries) => entries,
        Err(_) => return -1,
    };

    if entries.is_empty() {
        return 0;
    }

    let entry = &entries[0];
    let name = entry.d_name.as_bytes();

    let mut dirent = LinuxDirent64 {
        d_ino: 1,
        d_off: 0,
        d_reclen: core::mem::size_of::<LinuxDirent64>() as u16,
        d_type: 4,
        d_name: [0; 256],
    };

    let copy_len = name.len().min(255);
    dirent.d_name[..copy_len].copy_from_slice(&name[..copy_len]);

    // 拷贝到用户态
    let token = current_user_token();
    if copy_to_user(token, &dirent, buf as *mut LinuxDirent64).is_err() {
        log::error!("[sys_fstat] Failed to copy to {:?}", buf);
        -1;
    }
    core::mem::size_of::<LinuxDirent64>() as isize
}

pub fn sys_read(fd: usize, buf: *const u8, len: usize) -> isize {
    let token = current_user_token();
    let process = current_process();
    let inner = process.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file) = &inner.fd_table[fd] {
        let file = file.clone();
        if !file.readable() {
            return -1;
        }
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        file.read(UserBuffer::new(translated_byte_buffer(token, buf, len))) as isize
    } else {
        -1
    }
}

pub fn sys_write(fd: usize, buf: *const u8, len: usize) -> isize {
    let token = current_user_token();
    let process = current_process();
    let inner = process.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file) = &inner.fd_table[fd] {
        if !file.writable() {
            return -1;
        }
        let file = file.clone();
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        file.write(UserBuffer::new(translated_byte_buffer(token, buf, len))) as isize
    } else {
        -1
    }
}

pub fn sys_close(fd: usize) -> isize {
    let process = current_process();
    let mut inner = process.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if inner.fd_table[fd].is_none() {
        return -1;
    }
    inner.fd_table[fd].take();
    0
}

// 目前文件可能会因为输入none而发生panic,下面这个版本可以不发生pinic继续执行
pub fn sys_open(path: *const u8, flags: u32) -> isize {
    let process = current_process();
    let token = current_user_token();
    let path = translated_str(token, path);
    let flags = match OpenFlags::from_bits(flags) {
        Some(f) => f,
        None => return -1,
    };
    if let Some(inode) = open_file(path.as_str(), flags) {
        let mut inner = process.inner_exclusive_access();
        let fd = inner.alloc_fd();
        inner.fd_table[fd] = Some(inode);
        fd as isize
    } else {
        -1
    }
}

pub fn sys_openat(dirfd: usize, path: *const u8, flags: u32, mode: u32) -> isize {
    let task = current_task().unwrap();
    let token = task.get_user_token();
    let process = task.process.upgrade().unwrap();
    let mut inner = process.inner_exclusive_access();
    let path = translated_str(token, path);
    let flags = match OpenFlags::from_bits(flags) {
        Some(flags) => flags,
        None => {
            return -1; //EINVAL;
        }
    };
    let mode = StatMode::from_bits(mode);

    // let file_descriptor = inner.cwd ;
    //let base_dir = inner.cwd.clone();
    let base_dir = if dirfd == AT_FDCWD {
        inner.cwd.clone()
    } else {
        // 从 fd_table 查找 dirfd 对应的目录
        match inner.fd_table.get(dirfd) {
            Some(Some(file)) if file.is_dir() => {
                // 假设 File trait 有 get_path 方法
                file.get_path()
            }
            _ => return -1, // EBADF
        }
    };
    // 调用 open_file_at 打开文件
    // 判断是否是 O_DIRECTORY
    if flags.contains(OpenFlags::DIRECTORY) {
        // 假设 OpenFlags 有 DIRECTORY 标志
        // 如果是 O_DIRECTORY，调用 open_dir_at 或类似逻辑
        // 但由于 open_file_at 已经能返回目录的 OSInode，可以直接调用
        match open_file_at(&base_dir, &path, flags, mode.unwrap()) {
            Some(inode) if inode.is_dir() => {
                // 如果是目录，分配 fd 并返回
                let fd = inner.alloc_fd();
                let file: Arc<dyn File + Send + Sync> = inode;
                inner.fd_table[fd] = Some(file);
                fd as isize
            }
            _ => -1, // 不是目录或打开失败
        }
    } else {
        // 不是 O_DIRECTORY，按文件处理
        match open_file_at(&base_dir, &path, flags, mode.unwrap()) {
            Some(inode) => {
                let fd = inner.alloc_fd();
                let file: Arc<dyn File + Send + Sync> = inode;
                inner.fd_table[fd] = Some(file);
                fd as isize
            }
            None => -1,
        }
    }
}

// pub fn sys_pipe2(pipefd: usize, flags: u32) -> isize {
//     const VALID_FLAGS: OpenFlags = OpenFlags::from_bits_truncate(
//

pub fn sys_fstat(fd: usize, statbuf: *mut u8) -> isize {
    let proc = current_process();
    let token = current_user_token();
    info!("[sys_fstat] fd:{}", fd);

    let inode = match fd {
        AT_FDCWD => proc.inner_exclusive_access().cwd_inode.clone(),
        fd => {
            let fd_table = &proc.inner_exclusive_access().fd_table;
            match &fd_table[fd] {
                Some(OSInote) => OSInote.clone(),
                None => return -1,
            }
        }
    };
    if copy_to_user(token, &inode.get_stat(), statbuf as *mut UserStat).is_err() {
        log::error!("[sys_fstat] Failed to copy to {:?}", statbuf);
        return -1;
    }
    0
}

bitflags! {
    pub struct StatMode: u32 {
        ///bit mask for the file type bit field
        const S_IFMT    =   0o170000;
        ///socket
        const S_IFSOCK  =   0o140000;
        ///symbolic link
        const S_IFLNK   =   0o120000;
        ///regular file
        const S_IFREG   =   0o100000;
        ///block device
        const S_IFBLK   =   0o060000;
        ///directory
        const S_IFDIR   =   0o040000;
        ///character device
        const S_IFCHR   =   0o020000;
        ///FIFO
        const S_IFIFO   =   0o010000;

        ///set-user-ID bit (see execve(2))
        const S_ISUID   =   0o4000;
        ///set-group-ID bit (see below)
        const S_ISGID   =   0o2000;
        ///sticky bit (see below)
        const S_ISVTX   =   0o1000;

        ///owner has read, write, and execute permission
        const S_IRWXU   =   0o0700;
        ///owner has read permission
        const S_IRUSR   =   0o0400;
        ///owner has write permission
        const S_IWUSR   =   0o0200;
        ///owner has execute permission
        const S_IXUSR   =   0o0100;

        ///group has read, write, and execute permission
        const S_IRWXG   =   0o0070;
        ///group has read permission
        const S_IRGRP   =   0o0040;
        ///group has write permission
        const S_IWGRP   =   0o0020;
        ///group has execute permission
        const S_IXGRP   =   0o0010;

        ///others (not in group) have read, write,and execute permission
        const S_IRWXO   =   0o0007;
        ///others have read permission
        const S_IROTH   =   0o0004;
        ///others have write permission
        const S_IWOTH   =   0o0002;
        ///others have execute permission
        const S_IXOTH   =   0o0001;
    }
}
