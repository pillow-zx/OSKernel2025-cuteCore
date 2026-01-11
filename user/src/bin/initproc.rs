#![no_std]
#![no_main]

extern crate user;

use user::{exec, fork, println, wait, yield_};

#[no_mangle]
fn main() -> i32 {
    let tests = [
        "brk\0",
        "clone\0",
        "dup\0",
        "execve\0",
        "fork\0",
        "getcwd\0",
        "getpid\0",
        "gettimeofday\0",
        "mmap\0",
        // "mount\0",
        "open\0",
        // "pipe\0",
        "times\0",  //好像有点问题？
        "uname\0",
        "wait\0",
        "write\0",
        "chdir\0",
        "close\0",
        "dup2\0",
        "exit\0",
        "fstat\0",
        "getdents\0",
        "getppid\0",
        "mkdir_\0",
        "munmap\0",
        "openat\0",
        "read\0",
        "sleep\0",
        // "umount\0",
        // "unlink\0",
        "waitpid\0",
        "yield\0",
    ];

    for prog in tests {
        let pid = fork();
        if pid == 0 {
            println!("Running {}", prog);
            exec(prog, &[core::ptr::null()]);
            panic!("exec failed");
        } else {
            let mut exit_code: i32 = 0;
            wait(&mut exit_code);
            if pid == -1 {
                yield_();
                continue;
            }
        }
    }

    if fork() == 0 {
        println!("Exiting main...");
        exec("user_shell\0", &[core::ptr::null::<u8>()]);
    } else {
        loop {
            let mut exit_code: i32 = 0;
            let pid = wait(&mut exit_code);
            if pid == -1 {
                yield_();
                continue;
            }
            /*
            println!(
                "[initproc] Released a zombie process, pid={}, exit_code={}",
                pid,
                exit_code,
            );
            */
        }
    }
    0
}
