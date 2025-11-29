// libuser/src/syscall.rs
//! Low-level system call interface
//!
//! This module provides direct wrappers around the kernel's system calls.
//! For higher-level APIs, see the parent modules (io, process, mem).

use crate::abi::error::SyscallError;

/// System call numbers
pub const SYS_WRITE: u64 = 0;
pub const SYS_READ: u64 = 1;
pub const SYS_EXIT: u64 = 2;
pub const SYS_GETPID: u64 = 3;


pub const SYS_SPAWN: u64 = 6;
pub const SYS_WAIT: u64 = 8;
pub const SYS_MMAP: u64 = 9;
pub const SYS_MUNMAP: u64 = 10;



// V2 io_uring syscalls
pub const SYS_IO_URING_SETUP: u64 = 2002;
pub const SYS_IO_URING_ENTER: u64 = 2003;
pub const SYS_CAPABILITY_DUP: u64 = 2004;
pub const SYS_CAPABILITY_REVOKE: u64 = 2005;

/// System call result type
pub type SyscallResult<T> = Result<T, SyscallError>;

/// Convert errno (negative) to SyscallError
fn errno_to_syscall_error(errno: i64) -> SyscallError {
    match errno {
        -1 => SyscallError::PermissionDenied,  // EPERM
        -2 => SyscallError::NotFound,          // ENOENT
        -3 => SyscallError::NoSuchProcess,     // ESRCH
        -4 => SyscallError::Interrupted,       // EINTR
        -5 => SyscallError::IoError,           // EIO
        -9 => SyscallError::InvalidCapability, // EBADF
        -11 => SyscallError::WouldBlock,       // EAGAIN
        -12 => SyscallError::OutOfMemory,      // ENOMEM
        -13 => SyscallError::PermissionDenied, // EACCES
        -14 => SyscallError::InvalidAddress,   // EFAULT
        -16 => SyscallError::Busy,             // EBUSY
        -17 => SyscallError::AlreadyExists,    // EEXIST
        -22 => SyscallError::InvalidArgument,  // EINVAL
        -24 => SyscallError::TooManyOpen,      // EMFILE
        -32 => SyscallError::BrokenPipe,       // EPIPE
        -34 => SyscallError::InvalidArgument,  // ERANGE
        -38 => SyscallError::NotImplemented,   // ENOSYS
        -104 => SyscallError::ConnectionReset, // ECONNRESET
        -110 => SyscallError::Timeout,         // ETIMEDOUT
        -111 => SyscallError::ConnectionRefused, // ECONNREFUSED
        _ => SyscallError::InternalError,      // Fallback
    }
}

/// Helper to convert syscall result to Result type
#[inline]
fn syscall_result(ret: i64) -> SyscallResult<i64> {
    if ret >= 0 {
        Ok(ret)
    } else {
        Err(errno_to_syscall_error(ret))
    }
}

/// Perform a system call with up to 6 arguments
///
/// # Safety
/// This function is unsafe because it performs a raw system call.
/// The caller must ensure that the arguments are valid for the given syscall number.
#[inline(always)]
pub unsafe fn syscall6(num: u64, arg1: u64, arg2: u64, arg3: u64, arg4: u64, arg5: u64, arg6: u64) -> i64 {
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
            // These registers are clobbered by syscall instruction
            lateout("rcx") _,
            lateout("r11") _,
            // IMPORTANT: RDI, RSI, RDX are caller-saved in System V AMD64 ABI
            // They may be clobbered by the kernel syscall handler
            clobber_abi("C"),
            options(nostack)
        );
    }
    ret
}

/// Perform a system call with 1 argument
#[inline]
#[must_use]
pub unsafe fn syscall1(num: u64, arg1: i64) -> i64 {
    unsafe { syscall6(num, arg1 as u64, 0, 0, 0, 0, 0) }
}

/// Perform a system call with 2 arguments
#[inline]
#[must_use]
pub unsafe fn syscall2(num: u64, arg1: i64, arg2: i64) -> i64 {
    unsafe { syscall6(num, arg1 as u64, arg2 as u64, 0, 0, 0, 0) }
}

/// Perform a system call with 3 arguments
#[inline]
#[must_use]
pub unsafe fn syscall3(num: u64, arg1: i64, arg2: i64, arg3: i64) -> i64 {
    unsafe { syscall6(num, arg1 as u64, arg2 as u64, arg3 as u64, 0, 0, 0) }
}

/// Perform a system call with 4 arguments
#[inline]
#[must_use]
pub unsafe fn syscall4(num: u64, arg1: i64, arg2: i64, arg3: i64, arg4: i64) -> i64 {
    unsafe { syscall6(num, arg1 as u64, arg2 as u64, arg3 as u64, arg4 as u64, 0, 0) }
}

// ============================================================================
// System Call Wrappers
// ============================================================================

/// sys_write - Write to file descriptor
pub fn write(fd: u64, buf: &[u8]) -> SyscallResult<usize> {
    let ret = unsafe {
        syscall6(SYS_WRITE, fd, buf.as_ptr() as u64, buf.len() as u64, 0, 0, 0)
    };
    syscall_result(ret).map(|n| n as usize)
}

/// sys_read - Read from file descriptor
pub fn read(fd: u64, buf: &mut [u8]) -> SyscallResult<usize> {
    let ret = unsafe {
        syscall6(SYS_READ, fd, buf.as_mut_ptr() as u64, buf.len() as u64, 0, 0, 0)
    };
    syscall_result(ret).map(|n| n as usize)
}

/// sys_exit - Exit current process
pub fn exit(code: i32) -> ! {
    unsafe {
        syscall6(SYS_EXIT, code as u64, 0, 0, 0, 0, 0);
    }
    unreachable!()
}

/// sys_getpid - Get process ID
pub fn getpid() -> u64 {
    let ret = unsafe {
        syscall6(SYS_GETPID, 0, 0, 0, 0, 0, 0)
    };
    ret as u64
}

/// sys_spawn - Spawn a new process
pub fn spawn(path: &str, args: &[&str]) -> SyscallResult<u64> {
    use alloc::vec::Vec;
    use alloc::string::String;
    
    // Create null-terminated copies of args
    let mut args_store: Vec<String> = Vec::new();
    for arg in args {
        let mut s = String::from(*arg);
        s.push('\0');
        args_store.push(s);
    }
    
    // Create array of pointers
    let mut args_ptrs: Vec<u64> = Vec::new();
    for arg in &args_store {
        args_ptrs.push(arg.as_ptr() as u64);
    }
    
    let ret = unsafe {
        syscall6(
            SYS_SPAWN,
            path.as_ptr() as u64,
            path.len() as u64,
            args_ptrs.as_ptr() as u64,
            args_ptrs.len() as u64,
            0, 0
        )
    };
    syscall_result(ret).map(|pid| pid as u64)
}

/// sys_wait - Wait for child process
pub fn wait(pid: i64, status: Option<&mut i32>) -> SyscallResult<u64> {
    let status_ptr = status.map_or(0, |s| s as *mut i32 as u64);
    let ret = unsafe {
        syscall6(SYS_WAIT, pid as u64, status_ptr, 0, 0, 0, 0)
    };
    syscall_result(ret).map(|pid| pid as u64)
}

/// sys_mmap - Map memory
pub fn mmap(addr: u64, len: u64, prot: u64, flags: u64) -> SyscallResult<u64> {
    let ret = unsafe {
        syscall6(SYS_MMAP, addr, len, prot, flags, 0, 0)
    };
    syscall_result(ret).map(|addr| addr as u64)
}

/// sys_munmap - Unmap memory
pub fn munmap(addr: u64, len: u64) -> SyscallResult<()> {
    let ret = unsafe {
        syscall6(SYS_MUNMAP, addr, len, 0, 0, 0, 0)
    };
    syscall_result(ret).map(|_| ())
}



// ============================================================================
// V2 io_uring Wrappers
// ============================================================================



/// sys_io_uring_setup - Setup io_uring context
pub fn io_uring_setup(entries: u32, flags: u32) -> SyscallResult<u64> {
    let ret = unsafe {
        syscall6(SYS_IO_URING_SETUP, entries as u64, flags as u64, 0, 0, 0, 0)
    };
    syscall_result(ret).map(|addr| addr as u64)
}

/// sys_io_uring_enter - Submit and wait for completion
pub fn io_uring_enter(sqe_addr: u64, cqe_addr: u64) -> SyscallResult<()> {
    let ret = unsafe {
        syscall6(SYS_IO_URING_ENTER, sqe_addr, cqe_addr, 0, 0, 0, 0)
    };
    syscall_result(ret).map(|_| ())
}

/// sys_capability_dup - Duplicate capability
pub fn capability_dup(capability_id: u64, rights: u64) -> SyscallResult<u64> {
    let ret = unsafe {
        syscall6(SYS_CAPABILITY_DUP, capability_id, rights, 0, 0, 0, 0)
    };
    syscall_result(ret).map(|cap| cap as u64)
}

/// sys_capability_revoke - Revoke capability
pub fn capability_revoke(handle: u64) -> SyscallResult<()> {
    let ret = unsafe {
        syscall6(SYS_CAPABILITY_REVOKE, handle, 0, 0, 0, 0, 0)
    };
    syscall_result(ret).map(|_| ())
}

// ============================================================================
// Convenience Macros
// ============================================================================

/// Macro for performing system calls with cleaner syntax
#[macro_export]
macro_rules! syscall {
    ($num:expr) => {
        unsafe { $crate::syscall::syscall6($num, 0, 0, 0, 0, 0, 0) }
    };
    ($num:expr, $arg1:expr) => {
        unsafe { $crate::syscall::syscall6($num, $arg1, 0, 0, 0, 0, 0) }
    };
    ($num:expr, $arg1:expr, $arg2:expr) => {
        unsafe { $crate::syscall::syscall6($num, $arg1, $arg2, 0, 0, 0, 0) }
    };
    ($num:expr, $arg1:expr, $arg2:expr, $arg3:expr) => {
        unsafe { $crate::syscall::syscall6($num, $arg1, $arg2, $arg3, 0, 0, 0) }
    };
    ($num:expr, $arg1:expr, $arg2:expr, $arg3:expr, $arg4:expr) => {
        unsafe { $crate::syscall::syscall6($num, $arg1, $arg2, $arg3, $arg4, 0, 0) }
    };
    ($num:expr, $arg1:expr, $arg2:expr, $arg3:expr, $arg4:expr, $arg5:expr) => {
        unsafe { $crate::syscall::syscall6($num, $arg1, $arg2, $arg3, $arg4, $arg5, 0) }
    };
    ($num:expr, $arg1:expr, $arg2:expr, $arg3:expr, $arg4:expr, $arg5:expr, $arg6:expr) => {
        unsafe { $crate::syscall::syscall6($num, $arg1, $arg2, $arg3, $arg4, $arg5, $arg6) }
    };
}
