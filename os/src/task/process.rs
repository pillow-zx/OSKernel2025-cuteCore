//! # 进程控制块（ProcessControlBlock）模块
//!
//! ## Overview
//! 本模块实现了内核中 **进程控制块（PCB）** 的核心数据结构与操作。
//! 每个 PCB 管理一个进程的内存空间、文件描述符表、信号、任务（线程）、
//! 以及同步原语等资源。
//!
//! 功能包括：
//! - 创建新进程（`new`）
//! - 执行新程序（`exec`）
//! - 分叉子进程（`fork`）
//! - 管理任务、TID、文件描述符
//! - 管理信号、同步原语列表
//!
//! ## Assumptions
//! - 单线程 / 单处理器模型下安全访问
//! - 任务（线程）数量假定可控，`exec` 和 `fork` 仅支持单线程进程
//! - 内存空间管理由 `MemorySet` 提供
//!
//! ## Safety
//! - 内核栈、用户栈和 trap 上下文分配需正确映射到物理页
//! - `UPIntrFreeCell` 保护 PCB 内部可变状态
//! - 文件描述符、同步原语、任务等生命周期由 PCB 控制
//!
//! ## Invariants
//! - `tasks` 中的索引对应 TID
//! - `task_res_allocator` 保证 TID 唯一且回收正确
//! - 子进程的 `parent` 指向父进程弱引用，避免循环引用
//! - `memory_set` 包含完整用户程序映射，包括 trampoline / trap_cx / 用户栈
//! - 同步原语列表、fd_table、signals 的长度与状态保持一致
//!
//! ## Behavior
//! - `new`：
//!   - 创建新 PCB 与主线程（TaskControlBlock）
//!   - 分配用户栈与 trap 上下文
//!   - 初始化文件描述符表（stdin/stdout/stderr）
//! - `exec`：
//!   - 替换进程地址空间与 trap 上下文
//!   - 将参数压入用户栈
//!   - 更新 trap_cx 寄存器 a0/a1
//! - `fork`：
//!   - 完全复制父进程内存空间（包括用户栈/ trap_cx）
//!   - 复制文件描述符表
//!   - 分配新 PID 和内核栈
//!   - 将子进程加入父进程 children 列表
//! - `alloc_fd` / `alloc_tid` / `dealloc_tid`：
//!   - 管理文件描述符和线程 ID 分配
//! - 任务访问：通过 `get_task(tid)` 获取特定线程

use crate::fs::inode::OSInode;
use crate::fs::{current_root_inode, File, Stdin, Stdout};
use crate::hal::{trap_handler, PageTableImpl, TrapContext, UserStackBase};
use crate::mm::{translated_refmut, MemorySet, KERNEL_SPACE};
use crate::sync::{Condvar, Mutex, Semaphore, UPIntrFreeCell, UPIntrRefMut};
use crate::syscall::CloneFlags;
use crate::task::manager::{add_task, insert_into_pid2process};
use crate::task::pid::{pid_alloc, PidHandle, RecycleAllocator};
use crate::task::signal::SignalFlags;
use crate::task::task::TaskControlBlock;
use crate::timer::{ITimerVal, TimeVal};
use alloc::string::{String, ToString};
use alloc::sync::{Arc, Weak};
use alloc::vec;
use alloc::vec::Vec;

/// 进程控制块
///
/// ## Overview
/// 管理进程的资源、内存空间、任务和同步原语
pub struct ProcessControlBlock {
    /// 进程 PID
    pub pid: PidHandle,
    /// 内部可变状态
    inner: UPIntrFreeCell<ProcessControlBlockInner>,
}

/// PCB 内部状态
pub struct ProcessControlBlockInner {
    pub is_zombie: bool,
    pub memory_set: MemorySet<PageTableImpl>,
    pub parent: Option<Weak<ProcessControlBlock>>,
    pub children: Vec<Arc<ProcessControlBlock>>,
    pub exit_code: i32,
    pub cwd: String,
    //由于fat32每次打开都会开一个新inode，所以需要记录当前的inode是什么
    pub cwd_inode: Arc<dyn File + Send + Sync>,
    pub fd_table: Vec<Option<Arc<dyn File + Send + Sync>>>,
    pub signals: SignalFlags,
    pub tasks: Vec<Option<Arc<TaskControlBlock>>>,
    pub task_res_allocator: RecycleAllocator,
    pub mutex_list: Vec<Option<Arc<dyn Mutex>>>,
    pub semaphore_list: Vec<Option<Arc<Semaphore>>>,
    pub condvar_list: Vec<Option<Arc<Condvar>>>,
    pub rusage: Rusage,
    pub clock: ProcClock,
    pub timer: [ITimerVal; 1],
    pub tgid: usize,
}

impl ProcessControlBlock {
    /// 获取 PCB 内部独占访问
    pub fn inner_exclusive_access(&self) -> UPIntrRefMut<'_, ProcessControlBlockInner> {
        self.inner.exclusive_access()
    }

    /// 创建新进程
    ///
    /// ## Parameters
    /// - `elf_data`：用户程序 ELF 文件数据
    ///
    /// ## Returns
    /// - `Arc<Self>`：新建进程 PCB
    pub fn new(elf_data: &[u8]) -> Arc<Self> {
        // memory_set with elf program headers/trampoline/trap context/user stack
        let (memory_set, entry_point) = MemorySet::from_elf(elf_data);
        // allocate a pid
        let pid_handle = pid_alloc();
        let pid = pid_handle.0;
        let tgid = pid;
        let Root_Ionde = current_root_inode();
        let process = Arc::new(Self {
            pid: pid_handle,
            inner: unsafe {
                UPIntrFreeCell::new(ProcessControlBlockInner {
                    is_zombie: false,
                    memory_set,
                    parent: None,
                    children: Vec::new(),
                    exit_code: 0,
                    cwd_inode: Root_Ionde,
                    cwd: "/".to_string(),
                    fd_table: vec![
                        // 0 -> stdin
                        Some(Arc::new(Stdin)),
                        // 1 -> stdout
                        Some(Arc::new(Stdout)),
                        // 2 -> stderr
                        Some(Arc::new(Stdout)),
                    ],
                    signals: SignalFlags::empty(),
                    tasks: Vec::new(),
                    task_res_allocator: RecycleAllocator::new(),
                    mutex_list: Vec::new(),
                    semaphore_list: Vec::new(),
                    condvar_list: Vec::new(),
                    rusage: Rusage::new(),
                    clock: ProcClock::new(),
                    timer: [ITimerVal::new(); 1],
                    tgid,
                })
            },
        });

        /// 创建主线程
        let task = Arc::new(TaskControlBlock::new(
            Arc::clone(&process),
            UserStackBase,
            true,
        ));

        // 初始化 trap context
        let task_inner = task.inner_exclusive_access();
        let trap_cx = task_inner.get_trap_cx();
        let ustack_top = task_inner.res.as_ref().unwrap().ustack_top();
        let kstack_top = task.kstack.get_top();
        drop(task_inner);
        *trap_cx = TrapContext::app_init_context(
            entry_point,
            ustack_top,
            KERNEL_SPACE.exclusive_access().token(),
            kstack_top,
            trap_handler as usize,
        );

        // 添加线程到 PCB
        let mut process_inner = process.inner_exclusive_access();
        process_inner.tasks.push(Some(Arc::clone(&task)));
        drop(process_inner);
        insert_into_pid2process(process.getpid(), Arc::clone(&process));

        // 添加线程到调度器
        add_task(task);
        process
    }

    /// 执行新程序（仅支持单线程进程）
    pub fn exec(self: &Arc<Self>, elf_data: &[u8], args: Vec<String>) {
        assert_eq!(self.inner_exclusive_access().thread_count(), 1);
        // 通过 ELF 数据创建新的地址空间，获得新的用户栈基址和程序入口点
        let (memory_set, entry_point) = MemorySet::from_elf(elf_data);
        let new_token = memory_set.token();
        // 更新进程地址空间
        self.inner_exclusive_access().memory_set = memory_set;

        // 因为地址空间已经更改，需要重新为主线程分配用户资源
        let task = self.inner_exclusive_access().get_task(0);
        let mut task_inner = task.inner_exclusive_access();
        // 更新用户栈基址
        // 用户栈基地址已经被写死了，所以不再需要更新
        // task_inner.res.as_mut().unwrap().ustack_base = ustack_base;
        // 分配用户资源（用户栈 + trap 上下文）
        task_inner.res.as_mut().unwrap().alloc_user_res();
        task_inner.trap_cx_ppn = task_inner.res.as_mut().unwrap().trap_cx_ppn();
        // 把参数压入用户栈
        let mut user_sp = task_inner.res.as_mut().unwrap().ustack_top();
        user_sp -= (args.len() + 1) * core::mem::size_of::<usize>();
        let argv_base = user_sp;
        let mut argv: Vec<_> = (0..=args.len())
            .map(|arg| {
                translated_refmut(
                    new_token,
                    (argv_base + arg * core::mem::size_of::<usize>()) as *mut usize,
                )
            })
            .collect();
        *argv[args.len()] = 0;
        for i in 0..args.len() {
            user_sp -= args[i].len() + 1;
            *argv[i] = user_sp;
            let mut p = user_sp;
            for c in args[i].as_bytes() {
                *translated_refmut(new_token, p as *mut u8) = *c;
                p += 1;
            }
            *translated_refmut(new_token, p as *mut u8) = 0;
        }
        // 让 user_sp 对齐到 8 字节（k210 平台要求）
        user_sp -= user_sp % core::mem::size_of::<usize>();
        // 初始化 trap 上下文
        let mut trap_cx = TrapContext::app_init_context(
            entry_point,
            user_sp,
            KERNEL_SPACE.exclusive_access().token(),
            task.kstack.get_top(),
            trap_handler as usize,
        );
        trap_cx.general_regs.a0 = args.len();
        trap_cx.general_regs.a1 = argv_base;
        *task_inner.get_trap_cx() = trap_cx;
    }

    /// 分叉子进程（仅支持单线程父进程）
    // pub fn fork(self: &Arc<Self>) -> Arc<Self> {
    //     let mut parent = self.inner_exclusive_access();
    //     assert_eq!(parent.thread_count(), 1);
    //     // clone parent's memory_set completely including trampoline/ustacks/trap_cxs
    //     let memory_set = MemorySet::from_existed_user(&parent.memory_set);
    //     // alloc a pid
    //     let pid = pid_alloc();
    //     // copy fd table
    //     let mut new_fd_table: Vec<Option<Arc<dyn File + Send + Sync>>> = Vec::new();
    //     for fd in parent.fd_table.iter() {
    //         if let Some(file) = fd {
    //             new_fd_table.push(Some(file.clone()));
    //         } else {
    //             new_fd_table.push(None);
    //         }
    //     }
    //     let mut memory_set = memory_set;
    //     memory_set.heap_start = parent.memory_set.heap_start;
    //     memory_set.brk = parent.memory_set.brk;
    //     // create child process pcb
    //     let child = Arc::new(Self {
    //         pid,
    //         inner: unsafe {
    //             UPIntrFreeCell::new(ProcessControlBlockInner {
    //                 is_zombie: false,
    //                 memory_set,
    //                 parent: Some(Arc::downgrade(self)),
    //                 children: Vec::new(),
    //                 exit_code: 0,
    //                 cwd_inode:parent.cwd_inode.clone(),
    //                 cwd: parent.cwd.clone(),
    //                 fd_table: new_fd_table,
    //                 signals: SignalFlags::empty(),
    //                 tasks: Vec::new(),
    //                 task_res_allocator: RecycleAllocator::new(),
    //                 mutex_list: Vec::new(),
    //                 semaphore_list: Vec::new(),
    //                 condvar_list: Vec::new(),
    //             })
    //         },
    //     });
    //     // add child
    //     parent.children.push(Arc::clone(&child));
    //     let parent_task = parent.get_task(0);
    //     let (ustack_base, ustack_top) = {
    //         let task_inner = parent_task.inner_exclusive_access();
    //         let res = task_inner.res.as_ref().unwrap();
    //         (res.ustack_base(), res.ustack_top())
    //     };
    //     // create main thread of child process
    //     let task = Arc::new(TaskControlBlock::new(
    //         Arc::clone(&child),
    //         parent
    //             .get_task(0)
    //             .inner_exclusive_access()
    //             .res
    //             .as_ref()
    //             .unwrap()
    //             .ustack_base(),
    //         // here we do not allocate trap_cx or ustack again
    //         // but mention that we allocate a new kstack here
    //         false,
    //     ));
    //     // attach task to child process
    //     let mut child_inner = child.inner_exclusive_access();
    //     child_inner.tasks.push(Some(Arc::clone(&task)));
    //     drop(child_inner);
    //     // modify kstack_top in trap_cx of this thread
    //     let task_inner = task.inner_exclusive_access();
    //     let trap_cx = task_inner.get_trap_cx();
    //     trap_cx.kernel_sp = task.kstack.get_top();
    //     drop(task_inner);
    //     insert_into_pid2process(child.getpid(), Arc::clone(&child));
    //     // add this thread to scheduler
    //     add_task(task);
    //     child
    // }
    pub fn sys_clone(
        self: &Arc<Self>,
        flags: CloneFlags,
        stack: *const u8,
        tls: usize,
        exit_signal: SignalFlags,
    ) -> Arc<ProcessControlBlock> {
        let mut parent = self.inner_exclusive_access();
        assert_eq!(parent.thread_count(), 1);
        // clone parent's memory_set completely including trampoline/ustacks/trap_cxs
        let memory_set = MemorySet::from_existed_user(&parent.memory_set);
        let mut memory_set = memory_set;
        memory_set.heap_start = parent.memory_set.heap_start;
        memory_set.brk = parent.memory_set.brk;
        // alloc a pid
        let pid_handle = pid_alloc(); // 分配PID
                                      // copy fd table
        let mut new_fd_table: Vec<Option<Arc<dyn File + Send + Sync>>> = Vec::new();
        for fd in parent.fd_table.iter() {
            if let Some(file) = fd {
                new_fd_table.push(Some(file.clone()));
            } else {
                new_fd_table.push(None);
            }
        }
        let tgid = pid_handle.0;
        // create child process pcb
        let child = Arc::new(Self {
            pid: pid_handle,
            inner: unsafe {
                UPIntrFreeCell::new(ProcessControlBlockInner {
                    is_zombie: false,
                    memory_set,
                    parent: Some(Arc::downgrade(self)),
                    children: Vec::new(),
                    exit_code: exit_signal.bits() as i32,
                    cwd_inode: parent.cwd_inode.clone(),
                    cwd: parent.cwd.clone(),
                    fd_table: new_fd_table,
                    signals: SignalFlags::empty(),
                    tasks: Vec::new(),
                    task_res_allocator: RecycleAllocator::new(),
                    mutex_list: Vec::new(),
                    semaphore_list: Vec::new(),
                    condvar_list: Vec::new(),
                    rusage: Rusage::new(),
                    clock: ProcClock::new(),
                    timer: [ITimerVal::new(); 1],
                    tgid,
                })
            },
        });
        // add child
        parent.children.push(Arc::clone(&child));
        let parent_task = parent.get_task(0);

        let (ustack_base, ustack_top) = {
            let task_inner = parent_task.inner_exclusive_access();
            let res = task_inner.res.as_ref().unwrap();
            (res.ustack_base(), res.ustack_top())
        };
        // create main thread of child process
        let task = Arc::new(TaskControlBlock::new(
            Arc::clone(&child),
            parent
                .get_task(0)
                .inner_exclusive_access()
                .res
                .as_ref()
                .unwrap()
                .ustack_base(),
            // here we do not allocate trap_cx or ustack again
            // but mention that we allocate a new kstack here
            false,
        ));
        // attach task to child process
        let mut child_inner = child.inner_exclusive_access();
        child_inner.tasks.push(Some(Arc::clone(&task)));
        drop(child_inner);
        // modify kstack_top in trap_cx of this thread
        let task_inner = task.inner_exclusive_access();
        let trap_cx = task_inner.get_trap_cx();
        trap_cx.kernel_sp = task.kstack.get_top();
        drop(task_inner);
        insert_into_pid2process(child.getpid(), Arc::clone(&child));
        // add this thread to scheduler
        add_task(task);
        child
    }
    /// 获取 PID
    pub fn getpid(&self) -> usize {
        self.pid.0
    }
}

impl ProcessControlBlockInner {
    /// 获取用户页表 token
    #[allow(unused)]
    pub fn get_user_token(&self) -> usize {
        self.memory_set.token()
    }

    /// 分配新的文件描述符
    pub fn alloc_fd(&mut self) -> usize {
        if let Some(fd) = (0..self.fd_table.len()).find(|fd| self.fd_table[*fd].is_none()) {
            fd
        } else {
            self.fd_table.push(None);
            self.fd_table.len() - 1
        }
    }

    /// 分配新的线程 ID
    pub fn alloc_tid(&mut self) -> usize {
        self.task_res_allocator.alloc()
    }

    /// 回收线程 ID
    pub fn dealloc_tid(&mut self, tid: usize) {
        self.task_res_allocator.dealloc(tid)
    }

    /// 返回线程数量
    pub fn thread_count(&self) -> usize {
        self.tasks.len()
    }

    /// 获取指定线程
    pub fn get_task(&self, tid: usize) -> Arc<TaskControlBlock> {
        self.tasks[tid].as_ref().unwrap().clone()
    }
    pub fn add_signal(&mut self, signal: SignalFlags) {
        self.signals.insert(signal);
    }

    /// 在进入陷阱时更新进程时间
    pub fn update_process_times_enter_trap(&mut self) {
        // 获取当前时间
        let now = TimeVal::now();
        // 更新上次进入内核态的时间
        self.clock.last_enter_s_mode = now;
        // 计算时间差
        let diff = now - self.clock.last_enter_u_mode;
        // 更新用户CPU时间
        self.rusage.ru_utime = self.rusage.ru_utime + diff;
    }
    /// 在离开陷阱时更新进程时间
    pub fn update_process_times_leave_trap(&mut self) {
        let now = TimeVal::now();
        self.update_itimer_real_if_exists(now - self.clock.last_enter_u_mode);
        let diff = now - self.clock.last_enter_s_mode;
        // println!("DEBUG: diff={:?}, is_timer={}", diff, is_timer); // 调试日志

        self.rusage.ru_stime = self.rusage.ru_stime + diff;

        self.clock.last_enter_u_mode = now;
    }
    /// 更新实时定时器
    pub fn update_itimer_real_if_exists(&mut self, diff: TimeVal) {
        // 如果当前定时器不为0
        if !self.timer[0].it_value.is_zero() {
            // 更新定时器
            self.timer[0].it_value = self.timer[0].it_value - diff;
            // 如果定时器为0
            if self.timer[0].it_value.is_zero() {
                // 添加信号
                self.add_signal(SignalFlags::SIGALRM);
                // 重置定时器
                self.timer[0].it_value = self.timer[0].it_interval;
            }
        }
    }
}

#[allow(unused)]
#[derive(Clone, Copy)]
#[repr(C)]
pub struct Rusage {
    // user CPU time used
    pub ru_utime: TimeVal,
    // system CPU time used
    pub ru_stime: TimeVal,
}
impl Rusage {
    pub fn new() -> Self {
        Self {
            ru_utime: TimeVal::new(),
            ru_stime: TimeVal::new(),
        }
    }
}
#[repr(C)]
/// 进程时钟
/// 表示任务的时钟信息
pub struct ProcClock {
    /// 上次进入用户态的时间
    last_enter_u_mode: TimeVal,
    /// 上次进入内核态的时间
    last_enter_s_mode: TimeVal,
}
impl ProcClock {
    /// 构造函数
    pub fn new() -> Self {
        // 获取当前时间
        let now = TimeVal::now();
        Self {
            last_enter_u_mode: now,
            last_enter_s_mode: now,
        }
    }
}
