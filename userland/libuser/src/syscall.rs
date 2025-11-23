//! Low-level system call interface
//!
//! This module provides direct wrappers around the kernel's system calls.
//! For higher-level APIs, see the parent modules (io, process, mem).

/// System call numbers
pub const SYS_WRITE: u64 = 0;
pub const SYS_READ: u64 = 1;
pub const SYS_EXIT: u64 = 2;
pub const SYS_GETPID: u64 = 3;
pub const SYS_ALLOC: u64 = 4;     // Deprecated - use MMAP
pub const SYS_DEALLOC: u64 = 5;   // Deprecated - use MUNMAP
pub const SYS_FORK: u64 = 6;
pub const SYS_EXEC: u64 = 7;
pub const SYS_WAIT: u64 = 8;
pub const SYS_MMAP: u64 = 9;
pub const SYS_MUNMAP: u64 = 10;
pub const SYS_PIPE: u64 = 11;

/// System call error codes (Linux-compatible)
pub mod errno {
    /// Operation not permitted
    pub const EPERM: i64 = -1;
    /// No such file or directory
    pub const ENOENT: i64 = -2;
    /// No such process
    pub const ESRCH: i64 = -3;
    /// Interrupted system call
    pub const EINTR: i64 = -4;
    /// I/O error
    pub const EIO: i64 = -5;
    /// Bad file descriptor
    pub const EBADF: i64 = -9;
    /// No child processes
    pub const ECHILD: i64 = -10;
    /// Try again
    pub const EAGAIN: i64 = -11;
    /// Out of memory
    pub const ENOMEM: i64 = -12;
    /// Bad address
    pub const EFAULT: i64 = -14;
    /// Invalid argument
    pub const EINVAL: i64 = -22;
    /// Broken pipe
    pub const EPIPE: i64 = -32;
    /// Function not implemented
    pub const ENOSYS: i64 = -38;
}

/// System call result type
pub type SyscallResult<T> = Result<T, SyscallError>;

/// System call error type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyscallError {
    code: i64,
}

impl SyscallError {
    /// Create a new syscall error from an error code
    pub const fn new(code: i64) -> Self {
        Self { code }
    }

    /// Get the error code
    pub const fn code(self) -> i64 {
        self.code
    }

    /// Check if this is a specific error
    pub const fn is(self, errno: i64) -> bool {
        self.code == errno
    }

    /// Get a human-readable description of the error
    pub const fn description(self) -> &'static str {
        use errno::*;
        match self.code {
            EPERM => "Operation not permitted",
            ENOENT => "No such file or directory",
            ESRCH => "No such process",
            EINTR => "Interrupted system call",
            EIO => "I/O error",
            EBADF => "Bad file descriptor",
            ECHILD => "No child processes",
            EAGAIN => "Resource temporarily unavailable",
            ENOMEM => "Out of memory",
            EFAULT => "Bad address",
            EINVAL => "Invalid argument",
            EPIPE => "Broken pipe",
            ENOSYS => "Function not implemented",
            _ => "Unknown error",
        }
    }
}

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

/// Helper to convert syscall result to Result type
#[inline]
fn syscall_result(ret: i64) -> SyscallResult<i64> {
    if ret >= 0 {
        Ok(ret)
    } else {
        Err(SyscallError::new(ret))
    }
}

// ============================================================================
// System Call Wrappers
// ============================================================================

/// sys_write - Write to file descriptor
///
/// # Arguments
/// * `fd` - File descriptor
/// * `buf` - Buffer to write
///
/// # Returns
/// Number of bytes written, or error
///
/// # Errors
/// * `EBADF` - Invalid file descriptor
/// * `EFAULT` - Invalid buffer pointer
/// * `EINVAL` - Invalid length
/// * `EIO` - I/O error
/// * `EPIPE` - Broken pipe
pub fn write(fd: u64, buf: &[u8]) -> SyscallResult<usize> {
    let ret = unsafe {
        syscall6(SYS_WRITE, fd, buf.as_ptr() as u64, buf.len() as u64, 0, 0, 0)
    };
    syscall_result(ret).map(|n| n as usize)
}

/// sys_read - Read from file descriptor
///
/// # Arguments
/// * `fd` - File descriptor
/// * `buf` - Buffer to read into
///
/// # Returns
/// Number of bytes read, or error (0 = EOF)
///
/// # Errors
/// * `EBADF` - Invalid file descriptor
/// * `EFAULT` - Invalid buffer pointer
/// * `EIO` - I/O error
/// * `EAGAIN` - Would block (non-blocking I/O)
pub fn read(fd: u64, buf: &mut [u8]) -> SyscallResult<usize> {
    let ret = unsafe {
        syscall6(SYS_READ, fd, buf.as_mut_ptr() as u64, buf.len() as u64, 0, 0, 0)
    };
    syscall_result(ret).map(|n| n as usize)
}

/// sys_exit - Exit current process
///
/// This function never returns.
///
/// # Arguments
/// * `code` - Exit code
pub fn exit(code: i32) -> ! {
    unsafe {
        syscall6(SYS_EXIT, code as u64, 0, 0, 0, 0, 0);
    }
    unreachable!()
}

/// sys_getpid - Get process ID
///
/// # Returns
/// Current process ID
pub fn getpid() -> u64 {
    let ret = unsafe {
        syscall6(SYS_GETPID, 0, 0, 0, 0, 0, 0)
    };
    ret as u64
}

/// sys_fork - Fork process
///
/// # Returns
/// * In parent: Child PID
/// * In child: 0
/// * On error: Error code
///
/// # Errors
/// * `ENOMEM` - Out of memory
pub fn fork() -> SyscallResult<u64> {
    let ret = unsafe {
        syscall6(SYS_FORK, 0, 0, 0, 0, 0, 0)
    };
    syscall_result(ret).map(|pid| pid as u64)
}

/// sys_exec - Execute program
///
/// This function only returns on error.
///
/// # Arguments
/// * `path` - Path to program (currently ignored)
///
/// # Returns
/// Only returns on error
///
/// # Errors
/// * `ENOMEM` - Out of memory
pub fn exec(path: &str) -> SyscallError {
    let ret = unsafe {
        syscall6(SYS_EXEC, path.as_ptr() as u64, path.len() as u64, 0, 0, 0, 0)
    };
    // exec should not return on success
    SyscallError::new(ret)
}

/// sys_wait - Wait for child process
///
/// # Arguments
/// * `pid` - Process ID to wait for (currently ignored, waits for any child)
/// * `status` - Optional pointer to store exit status
///
/// # Returns
/// PID of terminated child
///
/// # Errors
/// * `ECHILD` - No child processes
/// * `ESRCH` - Process not found
/// * `EFAULT` - Invalid status pointer
pub fn wait(pid: i64, status: Option<&mut i32>) -> SyscallResult<u64> {
    let status_ptr = status.map_or(0, |s| s as *mut i32 as u64);
    let ret = unsafe {
        syscall6(SYS_WAIT, pid as u64, status_ptr, 0, 0, 0, 0)
    };
    syscall_result(ret).map(|pid| pid as u64)
}

/// sys_mmap - Map memory
///
/// # Arguments
/// * `addr` - Desired address (0 = kernel chooses)
/// * `len` - Length in bytes
/// * `prot` - Protection flags (currently ignored)
/// * `flags` - Mapping flags (currently ignored)
///
/// # Returns
/// Address of mapped memory
///
/// # Errors
/// * `EINVAL` - Invalid arguments
/// * `ENOMEM` - Out of memory
pub fn mmap(addr: u64, len: u64, prot: u64, flags: u64) -> SyscallResult<u64> {
    let ret = unsafe {
        syscall6(SYS_MMAP, addr, len, prot, flags, 0, 0)
    };
    syscall_result(ret).map(|addr| addr as u64)
}

/// sys_munmap - Unmap memory
///
/// # Arguments
/// * `addr` - Address to unmap
/// * `len` - Length in bytes
///
/// # Returns
/// Success (0) or error
///
/// # Errors
/// * `EINVAL` - Invalid arguments
pub fn munmap(addr: u64, len: u64) -> SyscallResult<()> {
    let ret = unsafe {
        syscall6(SYS_MUNMAP, addr, len, 0, 0, 0, 0)
    };
    syscall_result(ret).map(|_| ())
}

/// sys_pipe - Create a pipe
///
/// # Arguments
/// * `pipefd` - Array to store file descriptors [read_fd, write_fd]
///
/// # Returns
/// Success (0) or error
///
/// # Errors
/// * `EFAULT` - Invalid pipefd pointer
/// * `ESRCH` - Process not found
pub fn pipe(pipefd: &mut [u64; 2]) -> SyscallResult<()> {
    let ret = unsafe {
        syscall6(SYS_PIPE, pipefd.as_mut_ptr() as u64, 0, 0, 0, 0, 0)
    };
    syscall_result(ret).map(|_| ())
}

// ============================================================================
// Convenience Macros
// ============================================================================

/// Macro for performing system calls with cleaner syntax
///
/// # Examples
/// ```
/// use libuser::syscall;
///
/// // Write to stdout
/// let result = syscall!(WRITE, 1, message.as_ptr(), message.len());
///
/// // Get process ID
/// let pid = syscall!(GETPID);
/// ```
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
