// src/kernel/syscall/mod.rs
//! System call implementation module
//!
//! This module provides the actual implementations of system calls
//! and the dispatch mechanism.

use crate::arch::Cpu;
use crate::debug_println;
use crate::println;
use crate::kernel::core::traits::CharDevice;

/// Maximum length for sys_write (1MB)
const MAX_WRITE_LEN: u64 = 1024 * 1024;

/// Check if an address is in user space
/// 
/// User space: 0x0000_0000_0000_0000 ~ 0x0000_7FFF_FFFF_FFFF
/// Kernel space: 0xFFFF_8000_0000_0000 ~ 0xFFFF_FFFF_FFFF_FFFF
#[inline]
fn is_user_address(addr: u64) -> bool {
    addr < 0x0000_8000_0000_0000
}

/// Check if a memory range is in user space
#[inline]
fn is_user_range(addr: u64, len: u64) -> bool {
    // Check for overflow
    let end = addr.checked_add(len);
    if end.is_none() {
        return false;
    }
    
    let end = end.unwrap();
    is_user_address(addr) && is_user_address(end.saturating_sub(1))
}

/// System call result type
pub type SyscallResult = i64;

/// Success code
pub const SUCCESS: SyscallResult = 0;

/// Error codes (Linux-compatible)
pub const EPERM: SyscallResult = -1;     // Operation not permitted
pub const ENOENT: SyscallResult = -2;    // No such file or directory
pub const EINTR: SyscallResult = -4;     // Interrupted system call
pub const EIO: SyscallResult = -5;       // I/O error
pub const EBADF: SyscallResult = -9;     // Bad file descriptor
pub const ENOMEM: SyscallResult = -12;   // Out of memory
pub const EFAULT: SyscallResult = -14;   // Bad address (invalid pointer)
pub const EINVAL: SyscallResult = -22;   // Invalid argument
pub const ENOSYS: SyscallResult = -38;   // Function not implemented

/// sys_write - Write to console
///
/// Arguments:
/// - arg1: buffer pointer
/// - arg2: length
/// 
/// Returns:
/// - Positive: Number of bytes written
/// - Negative: Error code (EFAULT, EINVAL)
pub fn sys_write(buf: u64, len: u64, _arg3: u64, _arg4: u64, _arg5: u64, _arg6: u64) -> SyscallResult {
    // 1. Validate pointer is in user space
    if buf == 0 || !is_user_address(buf) {
        debug_println!("[SYSCALL] sys_write: invalid buffer address 0x{:x}", buf);
        return EFAULT;
    }
    
    // 2. Validate length
    if len > MAX_WRITE_LEN {
        debug_println!("[SYSCALL] sys_write: length too large ({})", len);
        return EINVAL;
    }
    
    // 3. Validate memory range is in user space
    if !is_user_range(buf, len) {
        debug_println!("[SYSCALL] sys_write: buffer range crosses user/kernel boundary");
        return EFAULT;
    }
    
    // 4. Safely read user buffer
    // SAFETY: We've validated that the pointer is in user space
    // TODO: In Phase 2, also validate that the memory is mapped and readable
    let slice = unsafe {
        core::slice::from_raw_parts(buf as *const u8, len as usize)
    };
    
    // 5. Write to console
    use crate::kernel::driver::serial::SERIAL1;
    if let Some(mut serial) = SERIAL1.try_lock() {
        for &byte in slice {
            let _ = serial.write_byte(byte);
        }
    }
    
    len as SyscallResult
}

/// sys_read - Read from keyboard
pub fn sys_read(_buf: u64, _len: u64, _arg3: u64, _arg4: u64, _arg5: u64, _arg6: u64) -> SyscallResult {
    debug_println!("[SYSCALL] sys_read not implemented yet");
    ENOSYS
}

/// sys_exit - Exit current process
pub fn sys_exit(code: u64, _arg2: u64, _arg3: u64, _arg4: u64, _arg5: u64, _arg6: u64) -> SyscallResult {
    println!("[SYSCALL] sys_exit called with code={}", code);
    // TODO: Actually terminate the process
    // For now, just loop
    loop {
        crate::arch::ArchCpu::halt();
    }
}

/// sys_getpid - Get process ID
pub fn sys_getpid(_arg1: u64, _arg2: u64, _arg3: u64, _arg4: u64, _arg5: u64, _arg6: u64) -> SyscallResult {
    // For now, always return PID 1 (we only have one "process")
    1
}

/// sys_alloc - Allocate memory
pub fn sys_alloc(size: u64, _arg2: u64, _arg3: u64, _arg4: u64, _arg5: u64, _arg6: u64) -> SyscallResult {
    debug_println!("[SYSCALL] sys_alloc not implemented yet (requested {} bytes)", size);
    ENOSYS
}

/// sys_dealloc - Deallocate memory
pub fn sys_dealloc(ptr: u64, _arg2: u64, _arg3: u64, _arg4: u64, _arg5: u64, _arg6: u64) -> SyscallResult {
    debug_println!("[SYSCALL] sys_dealloc not implemented yet (ptr=0x{:x})", ptr);
    ENOSYS
}

/// Syscall handler function type
type SyscallHandler = fn(u64, u64, u64, u64, u64, u64) -> SyscallResult;

/// Syscall dispatch table
static SYSCALL_TABLE: &[SyscallHandler] = &[
    sys_write,    // 0
    sys_read,     // 1
    sys_exit,     // 2
    sys_getpid,   // 3
    sys_alloc,    // 4
    sys_dealloc,  // 5
];

/// Dispatch a syscall to its handler
pub fn dispatch(
    syscall_num: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    arg4: u64,
    arg5: u64,
    arg6: u64,
) -> SyscallResult {
    let num = syscall_num as usize;
    
    if num >= SYSCALL_TABLE.len() {
        debug_println!("[SYSCALL] Invalid syscall number: {}", syscall_num);
        return ENOSYS;
    }
    
    debug_println!(
        "[SYSCALL] Dispatching syscall {} with args=({}, {}, {}, {}, {}, {})",
        syscall_num, arg1, arg2, arg3, arg4, arg5, arg6
    );
    
    let handler = SYSCALL_TABLE[num];
    handler(arg1, arg2, arg3, arg4, arg5, arg6)
}

/// Test syscall mechanism from kernel space
///
/// This is a simple test that can be called from kernel initialization
/// to verify that syscalls work correctly before jumping to user mode.
///
/// # Safety
/// This function simulates syscalls but runs in kernel space (Ring 0).
/// It's safe to call during boot before user mode is active.
#[cfg(debug_assertions)]
#[allow(dead_code)]
pub fn test_syscall_mechanism() {
    debug_println!("\n=== Testing Syscall Mechanism ===");
    
    // Test 1: sys_getpid
    debug_println!("Test 1: sys_getpid");
    let pid = dispatch(3, 0, 0, 0, 0, 0, 0);
    debug_println!("  Result: PID = {}", pid);
    
    // Test 2: sys_write (valid)
    debug_println!("\nTest 2: sys_write (valid message)");
    let message = b"[Test] Hello from syscall test!\n";
    let result = dispatch(
        0, // sys_write
        1, // stdout
        message.as_ptr() as u64,
        message.len() as u64,
        0, 0, 0
    );
    debug_println!("  Result: {} bytes written", result);
    
    // Test 3: sys_write (invalid pointer)
    debug_println!("\nTest 3: sys_write (invalid pointer)");
    let result = dispatch(
        0, // sys_write
        1, // stdout
        0, // NULL pointer
        100,
        0, 0, 0
    );
    debug_println!("  Result: {} (expected EFAULT = -14)", result);
    
    // Test 4: sys_write (kernel address)
    debug_println!("\nTest 4: sys_write (kernel address)");
    let result = dispatch(
        0, // sys_write
        1, // stdout
        0xFFFF_8000_0000_0000, // Kernel space
        100,
        0, 0, 0
    );
    debug_println!("  Result: {} (expected EFAULT = -14)", result);
    
    debug_println!("\n=== Syscall Mechanism Test Complete ===\n");
}
