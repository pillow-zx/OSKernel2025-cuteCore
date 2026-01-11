use core::arch::asm;

const SYSCALL_DUP: usize = 23;
const SYSCALL_DUP3: usize = 24;
const SYSCALL_MKDIRAT: usize = 34;
const SYSCALL_OPEN: usize = 56;
const SYSCALL_CLOSE: usize = 57;
const SYSCALL_PIPE: usize = 59;
const SYSCALL_GETDENTS: usize = 61;
const SYSCALL_READ: usize = 63;
const SYSCALL_WRITE: usize = 64;
const SYSCALL_FSTAT: usize = 80;
const SYSCALL_EXIT: usize = 93;
const SYSCALL_YIELD: usize = 124;
const SYSCALL_KILL: usize = 129;
const SYSCALL_GETPID: usize = 172;
const SYSCALL_BRK: usize = 214;
const SYSCALL_MUNMAP: usize = 215;
const SYSCALL_FORK: usize = 220;
const SYSCALL_EXEC: usize = 221;
const SYSCALL_MMAP: usize = 222;
const SYSCALL_WAITPID: usize = 260;
const SYSCALL_GETCWD: usize = 17;
const SYSCALL_CHDIR: usize = 49;

fn syscall(id: usize, args: [usize; 6]) -> isize {
    let mut ret: isize;
    unsafe {
        asm!(
        "ecall",
        inlateout("x10") args[0] => ret,
        in("x11") args[1],
        in("x12") args[2],
        in("x13") args[3],
        in("x14") args[4],
        in("x15") args[5],
        in("x17") id
        );
    }
    ret
}

// pub fn sys_dup(fd: usize) -> isize {
//     syscall(SYSCALL_DUP, [fd, 0, 0, 0, 0, 0])
// }

pub fn sys_open(path: &str, flags: u32) -> isize {
    syscall(SYSCALL_OPEN, [path.as_ptr() as usize, flags as usize, 0, 0, 0, 0])
}

pub fn sys_close(fd: usize) -> isize {
    syscall(SYSCALL_CLOSE, [fd, 0, 0, 0, 0, 0])
}

pub fn sys_pipe(pipe: &mut [usize]) -> isize {
    syscall(SYSCALL_PIPE, [pipe.as_mut_ptr() as usize, 0, 0, 0, 0, 0])
}

pub fn sys_read(fd: usize, buffer: &mut [u8]) -> isize {
    syscall(
        SYSCALL_READ,
        [fd, buffer.as_mut_ptr() as usize, buffer.len(), 0, 0, 0],
    )
}

pub fn sys_write(fd: usize, buffer: &[u8]) -> isize {
    syscall(SYSCALL_WRITE, [fd, buffer.as_ptr() as usize, buffer.len(), 0, 0, 0])
}

pub fn sys_exit(exit_code: i32) -> ! {
    syscall(SYSCALL_EXIT, [exit_code as usize, 0, 0, 0, 0, 0]);
    panic!("sys_exit never returns!");
}

pub fn sys_yield() -> isize {
    syscall(SYSCALL_YIELD, [0, 0, 0, 0, 0, 0])
}

pub fn sys_kill(pid: usize, signal: i32) -> isize {
    syscall(SYSCALL_KILL, [pid, signal as usize, 0, 0, 0, 0])
}

pub fn sys_getpid() -> isize {
    syscall(SYSCALL_GETPID, [0, 0, 0, 0, 0, 0])
}

pub fn sys_fork() -> isize {
    syscall(SYSCALL_FORK, [0, 0, 0, 0, 0, 0])
}

pub fn sys_mmap(start:usize,len:usize,prot:usize,flags:usize,fd:usize,off:usize) -> isize {
    syscall(SYSCALL_MMAP, [start, len, prot, flags , fd, off])
}

pub fn sys_exec(path: &str, args: &[*const u8]) -> isize {
    syscall(
        SYSCALL_EXEC,
        [path.as_ptr() as usize, args.as_ptr() as usize, 0, 0, 0, 0],
    )
}

pub fn sys_waitpid(pid: isize, exit_code: *mut i32) -> isize {
    syscall(SYSCALL_WAITPID, [pid as usize, exit_code as usize, 0, 0, 0, 0])
}

pub fn sys_getcwd(buf: &mut [u8]) -> isize {
    syscall(SYSCALL_GETCWD, [buf.as_mut_ptr() as usize, buf.len(), 0, 0, 0, 0])
}

pub fn sys_chdir(path: &str) -> isize {
    syscall(SYSCALL_CHDIR, [path.as_ptr() as usize, 0, 0, 0, 0, 0])
}

pub fn sys_brk(addr:usize) -> isize {
    syscall(SYSCALL_BRK, [addr, 0, 0, 0, 0, 0])
}

pub fn sys_munmap(start:usize, len:usize) -> isize {
    syscall(SYSCALL_MUNMAP, [start, len, 0, 0, 0, 0])
}

pub fn sys_fstat(fd: usize, statbuff: *mut u8) -> isize {
    syscall(SYSCALL_FSTAT, [fd, statbuff as usize,  0, 0, 0, 0])
}

pub fn sys_mkdirat(dirfd: isize,path: *const u8,mode: u8) -> isize {
    syscall(SYSCALL_MKDIRAT, [dirfd as usize, path as usize, mode as usize, 0, 0, 0])
}

pub fn sys_dup(fd: usize) -> isize {
    syscall(SYSCALL_DUP, [fd, 0, 0, 0, 0, 0])
}

pub fn sys_dup3(old:isize, new:isize, flags:usize) -> isize {
    syscall(SYSCALL_DUP3, [old as usize, new as usize, flags, 0, 0, 0])
}

pub fn sys_getdents(fd:usize, buf:*mut u8, len:usize) -> isize {
    syscall(SYSCALL_GETDENTS,[fd, buf as usize, len, 0, 0, 0])
}