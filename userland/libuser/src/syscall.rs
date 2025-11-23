//! Low-level system call interface
//!
//! This module provides direct wrappers around the kernel's system calls.

/// System call numbers
pub const SYS_WRITE: u64 = 0;
pub const SYS_READ: u64 = 1;
pub const SYS_EXIT: u64 = 2;
pub const SYS_GETPID: u64 = 3;
pub const SYS_ALLOC: u64 = 4;
pub const SYS_DEALLOC: u64 = 5;
pub const SYS_FORK: u64 = 6;
pub const SYS_EXEC: u64 = 7;
pub const SYS_WAIT: u64 = 8;
pub const SYS_MMAP: u64 = 9;
pub const SYS_MUNMAP: u64 = 10;
pub const SYS_PIPE: u64 = 11;

/// Perform a system call with up to 6 arguments
///
/// # Safety
/// This function is unsafe because it performs a raw system call.
/// The caller must ensure that the arguments are valid for the given syscall number.
#[inline(always)]
unsafe fn syscall6(num: u64, arg1: u64, arg2: u64, arg3: u64, arg4: u64, arg5: u64, arg6: u64) -> i64 {
    let ret: i64;
    unsafe {
        core::arch::asm!(
            "syscall",
            inlateout("rax") num => ret,
            in("rdi") arg1,
            in("rsi") arg2,
            in("rdx") arg3,
            in("r10") arg4,
            in("r8") arg5,
            in("r9") arg6,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret
}

/// sys_write - Write to file descriptor
pub fn write(fd: u64, buf: &[u8]) -> i64 {
    unsafe {
        syscall6(SYS_WRITE, fd, buf.as_ptr() as u64, buf.len() as u64, 0, 0, 0)
    }
}

/// sys_read - Read from file descriptor
pub fn read(fd: u64, buf: &mut [u8]) -> i64 {
    unsafe {
        syscall6(SYS_READ, fd, buf.as_mut_ptr() as u64, buf.len() as u64, 0, 0, 0)
    }
}

/// sys_exit - Exit current process
pub fn exit(code: i32) -> ! {
    unsafe {
        syscall6(SYS_EXIT, code as u64, 0, 0, 0, 0, 0);
    }
    unreachable!()
}

/// sys_getpid - Get process ID
pub fn getpid() -> i64 {
    unsafe {
        syscall6(SYS_GETPID, 0, 0, 0, 0, 0, 0)
    }
}

/// sys_fork - Fork process
pub fn fork() -> i64 {
    unsafe {
        syscall6(SYS_FORK, 0, 0, 0, 0, 0, 0)
    }
}

/// sys_exec - Execute program
pub fn exec(path: &str) -> i64 {
    unsafe {
        syscall6(SYS_EXEC, path.as_ptr() as u64, path.len() as u64, 0, 0, 0, 0)
    }
}

/// sys_wait - Wait for child process
///
/// Returns the PID of the terminated child, or negative error code
pub fn wait(pid: i64, status: Option<&mut i32>) -> i64 {
    let status_ptr = status.map_or(0, |s| s as *mut i32 as u64);
    unsafe {
        syscall6(SYS_WAIT, pid as u64, status_ptr, 0, 0, 0, 0)
    }
}

/// sys_mmap - Map memory
pub fn mmap(addr: u64, len: u64, prot: u64, flags: u64) -> i64 {
    unsafe {
        syscall6(SYS_MMAP, addr, len, prot, flags, 0, 0)
    }
}

/// sys_munmap - Unmap memory
pub fn munmap(addr: u64, len: u64) -> i64 {
    unsafe {
        syscall6(SYS_MUNMAP, addr, len, 0, 0, 0, 0)
    }
}

/// sys_pipe - Create a pipe
///
/// Returns 0 on success, negative error code on failure
pub fn pipe(pipefd: &mut [u64; 2]) -> i64 {
    unsafe {
        syscall6(SYS_PIPE, pipefd.as_mut_ptr() as u64, 0, 0, 0, 0, 0)
    }
}
