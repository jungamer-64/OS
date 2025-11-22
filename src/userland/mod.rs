// src/userland/mod.rs
//! Userland library for Ring 3 programs
//!
//! Provides system call wrappers and basic utilities for user-space programs.

#![no_std]

use core::arch::asm;

pub mod test_syscall;
pub mod ring3_test;

/// System call numbers (must match kernel/syscall/mod.rs)
#[repr(u64)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Syscall {
    Write = 0,
    Read = 1,
    Exit = 2,
    Getpid = 3,
    Alloc = 4,
    Dealloc = 5,
}

/// System call with 0 arguments
#[inline(always)]
pub unsafe fn syscall0(num: u64) -> i64 {
    let result: i64;
    asm!(
        "syscall",
        inlateout("rax") num => result,
        out("rcx") _,  // Clobbered by syscall
        out("r11") _,  // Clobbered by syscall
        options(nostack, preserves_flags)
    );
    result
}

/// System call with 1 argument
#[inline(always)]
pub unsafe fn syscall1(num: u64, arg1: u64) -> i64 {
    let result: i64;
    asm!(
        "syscall",
        inlateout("rax") num => result,
        in("rdi") arg1,
        out("rcx") _,
        out("r11") _,
        options(nostack, preserves_flags)
    );
    result
}

/// System call with 2 arguments
#[inline(always)]
pub unsafe fn syscall2(num: u64, arg1: u64, arg2: u64) -> i64 {
    let result: i64;
    asm!(
        "syscall",
        inlateout("rax") num => result,
        in("rdi") arg1,
        in("rsi") arg2,
        out("rcx") _,
        out("r11") _,
        options(nostack, preserves_flags)
    );
    result
}

/// System call with 3 arguments
#[inline(always)]
pub unsafe fn syscall3(num: u64, arg1: u64, arg2: u64, arg3: u64) -> i64 {
    let result: i64;
    asm!(
        "syscall",
        inlateout("rax") num => result,
        in("rdi") arg1,
        in("rsi") arg2,
        in("rdx") arg3,
        out("rcx") _,
        out("r11") _,
        options(nostack, preserves_flags)
    );
    result
}

/// Write to stdout
/// 
/// # Returns
/// 
/// - Ok(n): Number of bytes written
/// - Err(e): Error code (negative)
pub fn write(buf: &[u8]) -> Result<usize, i64> {
    let result = unsafe {
        syscall2(
            Syscall::Write as u64,
            buf.as_ptr() as u64,
            buf.len() as u64,
        )
    };
    
    if result >= 0 {
        Ok(result as usize)
    } else {
        Err(result)
    }
}

/// Print a string to stdout
pub fn print(s: &str) -> Result<usize, i64> {
    write(s.as_bytes())
}

/// Get current process ID
/// 
/// # Returns
/// 
/// - Ok(pid): Process ID
/// - Err(e): Error code (negative)
pub fn getpid() -> Result<u64, i64> {
    let result = unsafe { syscall0(Syscall::GetPid as u64) };
    
    if result >= 0 {
        Ok(result as u64)
    } else {
        Err(result)
    }
}

/// Exit current process
/// 
/// This function does not return.
pub fn exit(code: i32) -> ! {
    unsafe {
        syscall1(Syscall::Exit as u64, code as u64);
    }
    // Should never reach here
    loop {
        core::hint::spin_loop();
    }
}

/// Allocate memory (like brk)
/// 
/// # Returns
/// 
/// - Ok(ptr): Pointer to allocated memory
/// - Err(e): Error code (negative)
pub fn alloc(size: usize, align: usize) -> Result<*mut u8, i64> {
    let result = unsafe {
        syscall2(
            Syscall::Alloc as u64,
            size as u64,
            align as u64,
        )
    };
    
    if result >= 0 {
        Ok(result as *mut u8)
    } else {
        Err(result)
    }
}

/// Deallocate memory
/// 
/// # Returns
/// 
/// - Ok(()): Success
/// - Err(e): Error code (negative)
pub fn dealloc(ptr: *mut u8, size: usize) -> Result<(), i64> {
    let result = unsafe {
        syscall2(
            Syscall::Dealloc as u64,
            ptr as u64,
            size as u64,
        )
    };
    
    if result >= 0 {
        Ok(())
    } else {
        Err(result)
    }
}

// Error codes (must match kernel/syscall/mod.rs)
pub const EPERM: i64 = -1;
pub const ENOENT: i64 = -2;
pub const EINTR: i64 = -4;
pub const EIO: i64 = -5;
pub const EBADF: i64 = -9;
pub const ENOMEM: i64 = -12;
pub const EFAULT: i64 = -14;
pub const EINVAL: i64 = -22;
pub const ENOSYS: i64 = -38;

/// Convert error code to string
pub fn strerror(errno: i64) -> &'static str {
    match errno {
        EPERM => "Operation not permitted",
        ENOENT => "No such file or directory",
        EINTR => "Interrupted system call",
        EIO => "I/O error",
        EBADF => "Bad file descriptor",
        ENOMEM => "Out of memory",
        EFAULT => "Bad address",
        EINVAL => "Invalid argument",
        ENOSYS => "Function not implemented",
        _ => "Unknown error",
    }
}

/// println! macro for user space
#[macro_export]
macro_rules! println {
    () => ($crate::print("\n"));
    ($($arg:tt)*) => ({
        use core::fmt::Write;
        let mut writer = $crate::StdoutWriter;
        let _ = writeln!(writer, $($arg)*);
    })
}

/// print! macro for user space
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ({
        use core::fmt::Write;
        let mut writer = $crate::StdoutWriter;
        let _ = write!(writer, $($arg)*);
    })
}

/// Stdout writer for format! macros
pub struct StdoutWriter;

impl core::fmt::Write for StdoutWriter {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        print(s).map_err(|_| core::fmt::Error)?;
        Ok(())
    }
}
