#![allow(unused)]

use crate::fs::{open_file, OpenFlags};
use crate::mm::{
    copy_to_user, get_from_user, translated_byte_buffer, translated_ref, translated_refmut,
    translated_str, UserBuffer,
};
use crate::task::{
    block_current_and_run_next, current_process, current_task, current_user_token,
    exit_current_and_run_next, find_task_by_pid, pid2process, suspend_current_and_run_next,
    wake_blocked, Rusage, SignalFlags, TaskStatus,
};
use crate::timer::{add_timer, get_time_ms, TimeSpec, TimeVal, TimeZone, Tms};
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use bitflags::bitflags;
use core::mem::size_of;

pub fn sys_exit(exit_code: i32) -> ! {
    exit_current_and_run_next((exit_code & 0xff) << 8);
    panic!("Unreachable in sys_exit!");
}

pub fn sys_yield() -> isize {
    suspend_current_and_run_next();
    0
}

pub fn sys_getpid() -> isize {
    current_task().unwrap().process.upgrade().unwrap().getpid() as isize
}
/// brk 用于设置或获取当前进程的数据段（堆）的结束地址,成功返回新的堆顶地址，失败返回 -1
/// 如果传入的 addr 为 0，则返回当前堆顶地址
pub fn sys_brk(addr: usize) -> isize {
    let task = current_task().unwrap();
    let process = task.process.upgrade().unwrap();
    let mut inner = process.inner_exclusive_access();

    let memory_set = &mut inner.memory_set;
    //相当于查看当前堆顶是多少
    if addr == 0 {
        return memory_set.brk as isize;
    }

    if addr < memory_set.brk {
        return memory_set.brk as isize;
    }
    // 扩展堆
    let old_brk = memory_set.brk;
    if memory_set.expand_heap(addr).is_err() {
        return -1;
    }

    memory_set.brk = addr;
    addr as isize
}

/// unmap用来释放一段虚拟地址空间.成果返回0，失败返回-1
pub fn sys_munmap(start: usize, len: usize) -> isize {
    let process = current_process();
    let mut inner = process.inner_exclusive_access();
    match inner.memory_set.munmap(start, len) {
        Ok(()) => 0,
        Err(e) => e,
    }
}

pub fn sys_mmap(
    start: usize,
    len: usize,
    prot: usize,
    flags: usize,
    fd: isize,
    off: usize,
) -> isize {
    let process = current_process();
    let mut inner = process.inner_exclusive_access();
    let file = if fd >= 0 {
        inner
            .fd_table
            .get(fd as usize)
            .and_then(|f| f.as_ref())
            .cloned()
    } else {
        None
    };
    // 调用 MemorySet::mmap
    match inner.memory_set.mmap(start, len, prot, flags, file, off) {
        Ok(addr) => addr as isize, // 返回映射起始虚拟地址
        Err(e) => e,               // 返回 -1
    }
}

// pub fn sys_fork() -> isize {
//     let current_process = current_process();
//     let new_process = current_process.fork();
//     let new_pid = new_process.getpid();
//     // modify trap context of new_task, because it returns immediately after switching
//     let new_process_inner = new_process.inner_exclusive_access();
//     let task = new_process_inner.tasks[0].as_ref().unwrap();
//     let trap_cx = task.inner_exclusive_access().get_trap_cx();
//     // we do not have to move to next instruction since we have done it before
//     // for child process, fork returns 0
//     trap_cx.general_regs.a0 = 0;
//     new_pid as isize
// }
pub fn sys_clone(
    flags: u32,
    stack: *const u8,
    ptid: *mut u32,
    tls: usize,
    ctid: *mut u32,
) -> isize {
    let parent_task = current_task().unwrap();
    let parent_token = parent_task.get_user_token();
    let parent = parent_task.process.upgrade().unwrap();
    // let parent_inner = parent.inner_exclusive_access();
    // 只取低八位，防止误解
    let copy_flags = CloneFlags::from_bits_truncate(flags & !0xff);
    let exit_signal = SignalFlags::from_bits_truncate(flags & 0xff);
    let flags = CloneFlags::from_bits(flags & !0xff).unwrap();
    let child = parent.sys_clone(flags, stack, tls, exit_signal);
    let child_pid = child.pid.0;
    if copy_flags.contains(CloneFlags::CLONE_PARENT_SETTID) {
        *translated_refmut(parent_token, ptid) = child.pid.0 as u32
    }
    // if copy_flags.contains(CloneFlags::CLONE_CHILD_SETTID) {
    //     *translated_refmut(parent_token, ctid) = child.pid.0 as u32
    // }
    // if copy_flags.contains(CloneFlags::CLONE_CHILD_CLEARTID) {
    //     child.inner_exclusive_access().clear_child_tid = ctid as usize;
    // }
    let child_inner = child.inner_exclusive_access();
    let task = child_inner.tasks[0].as_ref().unwrap();
    let trap_cx = task.inner_exclusive_access().get_trap_cx();
    // we do not have to move to next instruction since we have done it before
    // for child process, fork returns 0
    trap_cx.general_regs.a0 = 0;
    // print!("child: {}", trap_cx.general_regs.a0) ;
    child_pid as isize
}
// pub fn sys_exec(path: *const u8, mut args: *const usize) -> isize {
//     let token = current_user_token();
//     let path = translated_str(token, path);
//     let mut args_vec: Vec<String> = Vec::new();
//     loop {
//         let arg_str_ptr = *translated_ref(token, args);
//         if arg_str_ptr == 0 {
//             break;
//         }
//         args_vec.push(translated_str(token, arg_str_ptr as *const u8));
//         unsafe {
//             args = args.add(1);
//         }
//     }
//     if let Some(app_inode) = open_file(path.as_str(), OpenFlags::RDONLY) {
//         let all_data = app_inode.read_all();
//         let process = current_process();
//         let argc = args_vec.len();
//         process.exec(all_data.as_slice(), args_vec);
//         // return argc because cx.x[10] will be covered with it later
//         argc as isize
//     } else {
//         -1
//     }
// }
pub fn sys_execve(
    path: *const u8,
    mut argv: *const *const u8,
    mut envp: *const *const u8,
) -> isize {
    let token = current_user_token();
    let path = translated_str(token, path);
    let mut argv_vec: Vec<String> = Vec::new();
    let mut envp_vec: Vec<String> = Vec::new();
    if !argv.is_null() {
        loop {
            let arg_str_ptr = *translated_ref(token, argv);
            if arg_str_ptr.is_null() {
                break;
            }
            argv_vec.push(translated_str(token, arg_str_ptr));
            unsafe {
                argv = argv.add(1);
            }
        }
    }
    if !envp.is_null() {
        loop {
            let envp_str_ptr = *translated_ref(token, envp);
            if envp_str_ptr.is_null() {
                break;
            }
            envp_vec.push(translated_str(token, envp_str_ptr));
            unsafe {
                envp = envp.add(1);
            }
        }
    }
    if let Some(app_inode) = open_file(path.as_str(), OpenFlags::RDONLY) {
        let all_data = app_inode.read_all();
        let process = current_process();
        let argv = argv_vec.len();
        let envp = envp_vec.len();
        process.exec(all_data.as_slice(), argv_vec);
        0
    } else {
        -1
    }
}

/// If there is not a child process whose pid is same as given, return -1.
/// Else if there is a child process but it is still running, return -2.
// pub fn sys_waitpid(pid: isize, exit_code_ptr: *mut i32) -> isize {
//     let process = current_process();
//     // find a child process
//
//     let mut inner = process.inner_exclusive_access();
//     if !inner
//         .children
//         .iter()
//         .any(|p| pid == -1 || pid as usize == p.getpid())
//     {
//         return -1;
//         // ---- release current PCB
//     }
//     let pair = inner.children.iter().enumerate().find(|(_, p)| {
//         // ++++ temporarily access child PCB exclusively
//         p.inner_exclusive_access().is_zombie && (pid == -1 || pid as usize == p.getpid())
//         // ++++ release child PCB
//     });
//     if let Some((idx, _)) = pair {
//         let child = inner.children.remove(idx);
//         // confirm that child will be deallocated after being removed from children list
//         assert_eq!(Arc::strong_count(&child), 1);
//         let found_pid = child.getpid();
//         // ++++ temporarily access child PCB exclusively
//         let exit_code = child.inner_exclusive_access().exit_code;
//         // ++++ release child PCB
//         *translated_refmut(inner.memory_set.token(), exit_code_ptr) = exit_code;
//         found_pid as isize
//     } else {
//         -2
//     }
//     // ---- release current PCB automatically
// }
bitflags! {
    struct WaitOption: u32 {
        const WNOHANG    = 1;
        const WSTOPPED   = 2;
        const WEXITED    = 4;
        const WCONTINUED = 8;
        const WNOWAIT    = 0x1000000;
    }
}
pub fn sys_wait4(pid: isize, status: *mut u32, option: u32, _ru: *mut Rusage) -> isize {
    let option = WaitOption::from_bits(option).unwrap();
    let task = current_task().unwrap();
    let token = current_user_token();
    let process = task.process.upgrade().unwrap();
    // find a child process
    loop {
        let mut inner = process.inner_exclusive_access();

        if !inner
            .children
            .iter()
            .any(|p| pid == -1 || pid as usize == p.getpid())
        {
            return -1;
            // ---- release current PCB
        }
        let pair = inner.children.iter().enumerate().find(|(_, p)| {
            // ++++ temporarily access child PCB exclusively
            p.inner_exclusive_access().is_zombie && (pid == -1 || pid as usize == p.getpid())
            // ++++ release child PCB
        });
        if let Some((idx, _)) = pair {
            let child = inner.children.remove(idx);
            // confirm that child will be deallocated after being removed from children list
            assert_eq!(Arc::strong_count(&child), 1);
            let child_inner = child.inner_exclusive_access();
            if child.pid.0 == child_inner.tgid {
                let found_pid = child.getpid();
                // ++++ temporarily hold child lock
                let exit_code = child_inner.exit_code;
                if !status.is_null() {
                    *translated_refmut(token, status) = exit_code as u32;
                }
                return found_pid as isize;
            }
        } else {
            drop(inner);
            if option.contains(WaitOption::WNOHANG) {
                return 0;
            } else {
                suspend_current_and_run_next();
            }
        }
    }
}

pub fn sys_nanosleep(req: *const TimeSpec, rem: *mut TimeSpec) -> isize {
    if req.is_null() {
        return -1; // EINVAL;
    }
    let task = current_task().unwrap();
    let token = task.get_user_token();
    let req = get_from_user(token, req);
    let end = TimeSpec::now() + req;
    // 精度会缺失一点
    let expire_ms = end.to_ms();
    add_timer(expire_ms, task.clone());
    drop(task);

    block_current_and_run_next();
    // let task = current_task().unwrap();
    // let inner = task.inner_exclusive_access();
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let now = TimeSpec::now();
    if process_inner.signals.is_empty() {
        assert!(end <= now);
        if !rem.is_null() {
            copy_to_user(token, &TimeSpec::new(), rem).unwrap();
        }
        0 //SUCCESS
    } else {
        if !rem.is_null() {
            copy_to_user(token, &(end - now), rem).unwrap();
        }
        -1 // EINTR
    }
    // ---- release current PCB automatically
}
// pub fn sys_kill(pid: usize, signal: u32) -> isize {
//     if let Some(process) = pid2process(pid) {
//         if let Some(flag) = SignalFlags::from_bits(signal) {
//             process.inner_exclusive_access().signals |= flag;
//             0
//         } else {
//             -1
//         }
//     } else {
//         -1
//     }
// }
pub fn sys_kill(pid: usize, sig: usize) -> isize {
    let signal = match SignalFlags::from_signum(sig) {
        Ok(signal) => signal,
        Err(_) => return -1, //EINVAL,
    };
    if pid > 0 {
        // [Warning] in current implementation,
        // signal will be sent to an arbitrary task with target `pid` (`tgid` more precisely).
        // But manual also require that the target task should not mask this signal.
        if let Some(task) = find_task_by_pid(pid) {
            if !signal.is_empty() {
                let mut task_inner = task.inner_exclusive_access();
                let mut process = task.process.upgrade().unwrap();
                let mut process_inner = process.inner_exclusive_access();
                process_inner.add_signal(signal);
                // wake up target process if it is sleeping
                if task_inner.task_status == TaskStatus::Blocked {
                    task_inner.task_status = TaskStatus::Ready;
                    drop(task_inner);
                    wake_blocked(task);
                }
            }
            0 // SUCCESS
        } else {
            -1 // ESRCH
        }
    } else if pid == 0 {
        todo!()
    } else if (pid as isize) == -1 {
        todo!()
    } else {
        // (pid as isize) < -1
        todo!()
    }
}
pub fn sys_getppid() -> isize {
    let task = current_task().unwrap();
    let process = task.process.upgrade().unwrap();
    let inner = process.inner_exclusive_access();
    let parent_weak = match inner.parent.as_ref() {
        Some(w) => w,
        None => return 0, // 如果没有父进程，返回 -1
    };
    let parent_arc = match parent_weak.upgrade() {
        Some(arc) => arc,
        None => return 0, // 如果父进程已被释放，返回 -1
    };
    let parent_inner = parent_arc.inner_exclusive_access();
    // process.pid.0 as isize
    parent_arc.pid.0 as isize
}

pub fn sys_times(tms_ptr: *mut Tms) -> isize {
    // let current_process = current_process();
    // let mut inner = current_process.inner_exclusive_access();
    // let task = match inner.tasks.pop(){
    //     Some(t) => t,
    //     None => return -1, // 失败时返回 -1
    // };
    let task = current_task().unwrap();
    let user_token = task.get_user_token();
    let process = task.process.upgrade().unwrap();
    let mut inner = process.inner_exclusive_access();
    let tms = translated_refmut(user_token, tms_ptr);

    let times = Tms {
        utime: inner.rusage.ru_utime.to_tick(),
        stime: inner.rusage.ru_stime.to_tick(),
        cutime: 0,
        cstime: 0,
    };
    // TODO: copy date to user space
    copy_to_user(user_token, &times, tms_ptr);
    crate::hal::get_time() as isize
}

// TODO：根据实际修改,新增loongarch64之后需要分隔开
pub fn sys_uname(utsname_ptr: *mut u8) -> isize {
    let token = current_user_token();
    let mut buffer = UserBuffer::new(translated_byte_buffer(
        token,
        utsname_ptr,
        size_of::<UTSName>(),
    ));
    const FIELD_OFFSET: usize = 65;
    buffer.write_buffer(Some(FIELD_OFFSET * 0), b"cutecore\0");
    buffer.write_buffer(Some(FIELD_OFFSET * 1), b"xeinnious\0");
    #[cfg(target_arch = "riscv64")]
    buffer.write_buffer(Some(FIELD_OFFSET * 2), b"5.0.0-riscv64\0");
    #[cfg(target_arch = "loongarch64")]
    buffer.write(Some(FIELD_OFFSET * 2), b"5.0.0-loongarch64\0");
    buffer.write_buffer(
        Some(FIELD_OFFSET * 3),
        b"#1 SMP Xein-Revo 6.15.2-arch1-1 (2025-06-10)\0",
    );
    #[cfg(target_arch = "riscv64")]
    buffer.write_buffer(Some(FIELD_OFFSET * 4), b"riscv64\0");
    #[cfg(target_arch = "loongarch64")]
    buffer.write(Some(FIELD_OFFSET * 4), b"loongarch64\0");
    buffer.write_buffer(Some(FIELD_OFFSET * 5), b"\0");
    0
    //SUCCESS
}
pub fn sys_gettimeofday(tv: *mut TimeVal, _tz: *mut TimeZone) -> isize {
    let token = current_user_token();
    if !tv.is_null() {
        let time_val = &TimeVal::now();
        if copy_to_user(token, time_val, tv).is_err() {
            log::error!("[sys_gettimeofday] Failed to copy to {:?}", tv);
            return -1; // EFAULT;
        }
    }
    0 // SUCCESS
}

// new add:sys_uname()需要将NTSName结构体写到UseBuffer中
#[allow(unused)]
#[repr(C)]
pub struct UTSName {
    sysname: [u8; 65],
    nodename: [u8; 65],
    release: [u8; 65],
    version: [u8; 65],
    machine: [u8; 65],
    domainname: [u8; 65],
}

bitflags! {
    pub struct CloneFlags: u32 {
        //const CLONE_NEWTIME         =   0x00000080;
        /// 决定是否共享虚拟内存空间
        const CLONE_VM              =   0x00000100;
        /// 决定是否共享文件系统信息（如当前工作目录和根目录）
        const CLONE_FS              =   0x00000200;
        /// 使新进程共享打开的文件描述符表，但不共享文件描述符的状态
        const CLONE_FILES           =   0x00000400;
        /// 使新进程共享信号处理
        const CLONE_SIGHAND         =   0x00000800;
        const CLONE_PIDFD           =   0x00001000;
        const CLONE_PTRACE          =   0x00002000;
        const CLONE_VFORK           =   0x00004000;
        const CLONE_PARENT          =   0x00008000;
        const CLONE_THREAD          =   0x00010000;
        const CLONE_NEWNS           =   0x00020000;
        const CLONE_SYSVSEM         =   0x00040000;
        const CLONE_SETTLS          =   0x00080000;
        const CLONE_PARENT_SETTID   =   0x00100000;
        const CLONE_CHILD_CLEARTID  =   0x00200000;
        const CLONE_DETACHED        =   0x00400000;
        const CLONE_UNTRACED        =   0x00800000;
        const CLONE_CHILD_SETTID    =   0x01000000;
        const CLONE_NEWCGROUP       =   0x02000000;
        /// 使新进程拥有一个新的、独立的UTS命名空间，可以隔离主机名和域名
        const CLONE_NEWUTS          =   0x04000000;
        /// 使新进程拥有一个新的、独立的IPC命名空间，可以隔离System V IPC和POSIX消息队列
        const CLONE_NEWIPC          =   0x08000000;
        /// 使新进程拥有一个新的、独立的用户命名空间，可以隔离用户和用户组ID
        const CLONE_NEWUSER         =   0x10000000;
        /// 使新进程拥有一个新的、独立的PID命名空间，可以隔离进程ID
        const CLONE_NEWPID          =   0x20000000;
        /// 使新进程拥有一个新的、独立的网络命名空间，可以隔离网络设备、协议栈和端口
        const CLONE_NEWNET          =   0x40000000;
        const CLONE_IO              =   0x80000000;
    }
}
