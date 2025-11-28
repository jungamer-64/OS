//! Low-level system call interface
//!
//! This module provides direct wrappers around the kernel's system calls.
//! For higher-level APIs, see the parent modules (io, process, mem).

/// System call numbers
/// System call number for write operation
pub const SYS_WRITE: u64 = 0;
/// System call number for read operation
pub const SYS_READ: u64 = 1;
/// System call number for exit operation
pub const SYS_EXIT: u64 = 2;
/// System call number for getpid operation
pub const SYS_GETPID: u64 = 3;
/// System call number for alloc operation (deprecated - use MMAP)
pub const SYS_ALLOC: u64 = 4;     // Deprecated - use MMAP
/// System call number for dealloc operation (deprecated - use MUNMAP)
pub const SYS_DEALLOC: u64 = 5;   // Deprecated - use MUNMAP
/// System call number for fork operation
pub const SYS_FORK: u64 = 6;
/// System call number for exec operation
pub const SYS_EXEC: u64 = 7;
/// System call number for wait operation
pub const SYS_WAIT: u64 = 8;
/// System call number for mmap operation
pub const SYS_MMAP: u64 = 9;
/// System call number for munmap operation
pub const SYS_MUNMAP: u64 = 10;
/// System call number for pipe operation
pub const SYS_PIPE: u64 = 11;
/// System call number for `io_uring_setup`
pub const SYS_IO_URING_SETUP: u64 = 12;
/// System call number for `io_uring_enter`
pub const SYS_IO_URING_ENTER: u64 = 13;
/// System call number for `io_uring_register`
pub const SYS_IO_URING_REGISTER: u64 = 14;

// ============================================================================
// Fast IPC Syscalls (Strategy 1-3)
// ============================================================================

/// Benchmark syscall (minimal overhead measurement)
pub const SYS_BENCHMARK: u64 = 1000;
/// Fast ring poll (SQPOLL kick)
pub const SYS_FAST_POLL: u64 = 1001;
/// Fast I/O setup (syscall-less rings)
pub const SYS_FAST_IO_SETUP: u64 = 1002;

// ============================================================================
// Ring-based Syscall System (Revolutionary Architecture)
// ============================================================================

/// Ring enter syscall (doorbell-only, no register arguments)
pub const SYS_RING_ENTER: u64 = 2000;
/// Ring register buffer syscall
pub const SYS_RING_REGISTER: u64 = 2001;
/// Ring setup syscall
pub const SYS_RING_SETUP: u64 = 2002;

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
    /// Try again / Would block
    pub const EAGAIN: i64 = -11;
    /// Out of memory
    pub const ENOMEM: i64 = -12;
    /// Bad address
    pub const EFAULT: i64 = -14;
    /// Invalid argument
    pub const EINVAL: i64 = -22;
    /// No space left on device
    pub const ENOSPC: i64 = -28;
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
    #[must_use]
    pub const fn new(code: i64) -> Self {
        Self { code }
    }
    
    /// Create a syscall error from raw value (for `ring_io` compatibility)
    #[must_use]
    pub const fn from_raw(code: i64) -> Self {
        Self { code }
    }

    /// Get the error code
    #[must_use]
    pub const fn code(self) -> i64 {
        self.code
    }

    /// Check if this is a specific error
    #[must_use]
    pub const fn is(self, errno: i64) -> bool {
        self.code == errno
    }

    /// Get a human-readable description of the error
    #[must_use]
    #[allow(clippy::wildcard_imports)]
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
            ENOSPC => "No space left on device",
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
///
/// # Safety
/// This function is unsafe because it performs a raw system call.
#[inline]
#[must_use]
#[allow(clippy::cast_sign_loss)]
pub unsafe fn syscall1(num: u64, arg1: i64) -> i64 {
    unsafe { syscall6(num, arg1 as u64, 0, 0, 0, 0, 0) }
}

/// Perform a system call with 2 arguments
///
/// # Safety
/// This function is unsafe because it performs a raw system call.
#[inline]
#[must_use]
#[allow(clippy::cast_sign_loss)]
pub unsafe fn syscall2(num: u64, arg1: i64, arg2: i64) -> i64 {
    unsafe { syscall6(num, arg1 as u64, arg2 as u64, 0, 0, 0, 0) }
}

/// Perform a system call with 3 arguments
///
/// # Safety
/// This function is unsafe because it performs a raw system call.
#[inline]
#[must_use]
#[allow(clippy::cast_sign_loss)]
pub unsafe fn syscall3(num: u64, arg1: i64, arg2: i64, arg3: i64) -> i64 {
    unsafe { syscall6(num, arg1 as u64, arg2 as u64, arg3 as u64, 0, 0, 0) }
}

/// Perform a system call with 4 arguments
///
/// # Safety
/// This function is unsafe because it performs a raw system call.
#[inline]
#[must_use]
#[allow(clippy::cast_sign_loss)]
pub unsafe fn syscall4(num: u64, arg1: i64, arg2: i64, arg3: i64, arg4: i64) -> i64 {
    unsafe { syscall6(num, arg1 as u64, arg2 as u64, arg3 as u64, arg4 as u64, 0, 0) }
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
// io_uring System Calls
// ============================================================================

/// `io_uring_setup` - Initialize io_uring for the current process
///
/// # Arguments
/// * `entries` - Number of entries (must be power of 2, max 256)
///
/// # Returns
/// Address of the SQ header (for future mmap)
///
/// # Errors
/// * `EINVAL` - Invalid entries count
/// * `ENOMEM` - Out of memory
/// * `ESRCH` - Process not found
pub fn io_uring_setup(entries: u32) -> SyscallResult<u64> {
    let ret = unsafe {
        syscall6(SYS_IO_URING_SETUP, u64::from(entries), 0, 0, 0, 0, 0)
    };
    syscall_result(ret).map(|addr| addr as u64)
}

/// `io_uring_enter` - Submit I/O and optionally wait for completions
///
/// This is the main io_uring syscall for batched I/O operations.
///
/// # Arguments
/// * `fd` - io_uring file descriptor (currently ignored)
/// * `to_submit` - Number of submissions to process (0 = all available)
/// * `min_complete` - Minimum completions to wait for (0 = non-blocking)
/// * `flags` - Operation flags
///
/// # Returns
/// Number of completions available in the CQ
///
/// # Errors
/// * `EINVAL` - io_uring not set up
/// * `ESRCH` - Process not found
pub fn io_uring_enter(fd: u32, to_submit: u32, min_complete: u32, flags: u32) -> SyscallResult<u32> {
    let ret = unsafe {
        syscall6(
            SYS_IO_URING_ENTER,
            u64::from(fd),
            u64::from(to_submit),
            u64::from(min_complete),
            u64::from(flags),
            0,
            0,
        )
    };
    syscall_result(ret).map(|n| n as u32)
}

/// `io_uring_register` - Register resources with io_uring
///
/// Used to register buffers or file descriptors for zero-copy operations.
/// Currently not implemented.
///
/// # Arguments
/// * `fd` - io_uring file descriptor
/// * `opcode` - Registration operation
/// * `arg` - Operation-specific argument
/// * `nr_args` - Number of arguments
///
/// # Returns
/// Success (0) or error
///
/// # Errors
/// * `ENOSYS` - Not implemented
pub fn io_uring_register(fd: u32, opcode: u32, arg: u64, nr_args: u32) -> SyscallResult<()> {
    let ret = unsafe {
        syscall6(
            SYS_IO_URING_REGISTER,
            u64::from(fd),
            u64::from(opcode),
            arg,
            u64::from(nr_args),
            0,
            0,
        )
    };
    syscall_result(ret).map(|_| ())
}

// ============================================================================
// Fast IPC System Calls (Strategy 1-3)
// ============================================================================

/// Benchmark modes for `sys_benchmark`
pub mod benchmark_mode {
    /// Minimal syscall (just return)
    pub const MINIMAL: u64 = 0;
    /// Read timestamp (rdtsc)
    pub const TIMESTAMP: u64 = 1;
    /// Memory fence
    pub const FENCE: u64 = 2;
    /// Check shared ring
    pub const RING_CHECK: u64 = 3;
}

/// Fast I/O setup flags
pub mod fast_io_flags {
    /// Enable kernel polling (SQPOLL mode)
    pub const SQPOLL: u64 = 1 << 0;
    /// Enable I/O completion polling
    pub const IOPOLL: u64 = 1 << 1;
}

/// `sys_benchmark` - Minimal syscall for measuring overhead
///
/// This syscall does minimal work to measure raw syscall latency.
///
/// # Arguments
/// * `mode` - Benchmark mode:
///   - 0: Minimal (just return)
///   - 1: Read timestamp (rdtsc)
///   - 2: Memory fence
///   - 3: Check shared ring
///
/// # Returns
/// * Mode 0: 0
/// * Mode 1: Current CPU timestamp
/// * Mode 2: 0
/// * Mode 3: Number of pending operations
///
/// # Errors
/// * `EINVAL` - Invalid mode
///
/// # Example
/// ```
/// use libuser::syscall::{benchmark, benchmark_mode};
///
/// // Measure syscall overhead
/// let start = benchmark(benchmark_mode::TIMESTAMP).unwrap();
/// let _ = benchmark(benchmark_mode::MINIMAL);
/// let end = benchmark(benchmark_mode::TIMESTAMP).unwrap();
/// let overhead = end - start;
/// ```
#[allow(clippy::cast_sign_loss)]
pub fn benchmark(mode: u64) -> SyscallResult<u64> {
    let ret = unsafe {
        syscall6(SYS_BENCHMARK, mode, 0, 0, 0, 0, 0)
    };
    syscall_result(ret).map(|v| v as u64)
}

/// `sys_fast_poll` - Poll fast I/O rings without blocking
///
/// This is the "kick" syscall for SQPOLL mode. When the kernel's
/// polling thread is sleeping, this wakes it up to process submissions.
///
/// In high-throughput scenarios with SQPOLL enabled, this syscall
/// may not be needed at all since the kernel continuously polls.
///
/// # Returns
/// Number of operations processed
///
/// # Errors
/// * `ESRCH` - Process not found
#[allow(clippy::cast_sign_loss)]
pub fn fast_poll() -> SyscallResult<u64> {
    let ret = unsafe {
        syscall6(SYS_FAST_POLL, 0, 0, 0, 0, 0, 0)
    };
    syscall_result(ret).map(|v| v as u64)
}

/// `sys_fast_io_setup` - Set up syscall-less I/O rings
///
/// Creates shared memory ring buffers for syscall-less I/O operations.
/// After setup, userspace can submit I/O requests by writing to the
/// submission queue without executing syscall instructions.
///
/// # Arguments
/// * `flags` - Configuration flags:
///   - Bit 0 (SQPOLL): Enable kernel polling
///   - Bit 1 (IOPOLL): Enable completion polling
///
/// # Returns
/// Base address of the fast I/O context (for mmap)
///
/// # Errors
/// * `ENOMEM` - Out of memory
/// * `ESRCH` - Process not found
///
/// # Example
/// ```
/// use libuser::syscall::{fast_io_setup, fast_io_flags};
///
/// // Set up with kernel polling enabled
/// let ctx = fast_io_setup(fast_io_flags::SQPOLL)?;
///
/// // Now writes to the submission queue are processed
/// // automatically without syscalls!
/// ```
#[allow(clippy::cast_sign_loss)]
pub fn fast_io_setup(flags: u64) -> SyscallResult<u64> {
    let ret = unsafe {
        syscall6(SYS_FAST_IO_SETUP, flags, 0, 0, 0, 0, 0)
    };
    syscall_result(ret).map(|v| v as u64)
}

/// Read CPU timestamp counter directly (without syscall)
///
/// This is faster than `benchmark(TIMESTAMP)` since it doesn't
/// execute a syscall instruction at all.
///
/// # Returns
/// Current CPU timestamp counter value
#[inline]
#[must_use]
pub fn rdtsc() -> u64 {
    let lo: u32;
    let hi: u32;
    unsafe {
        core::arch::asm!(
            "rdtsc",
            out("eax") lo,
            out("edx") hi,
            options(nostack, nomem)
        );
    }
    (u64::from(hi) << 32) | u64::from(lo)
}

/// Measure syscall overhead in CPU cycles
///
/// Performs a minimal syscall and returns the cycle count.
///
/// # Returns
/// Approximate number of CPU cycles for a minimal syscall
#[must_use]
pub fn measure_syscall_overhead() -> u64 {
    let start = rdtsc();
    let _ = benchmark(benchmark_mode::MINIMAL);
    let end = rdtsc();
    end.saturating_sub(start)
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
