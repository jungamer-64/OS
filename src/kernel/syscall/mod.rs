//! System call implementation module
//!
//! This module provides the actual implementations of system calls
//! and the dispatch mechanism.

use crate::arch::Cpu;
use crate::debug_println;
use crate::println;

/// System call result type
pub type SyscallResult = i64;

/// Success code
pub const SUCCESS: SyscallResult = 0;

/// Error codes
pub const ERR_INVALID_SYSCALL: SyscallResult = -1;
pub const ERR_INVALID_ARG: SyscallResult = -2;
pub const ERR_NOT_IMPLEMENTED: SyscallResult = -3;

/// sys_write - Write to console
///
/// Arguments:
/// - arg1: buffer pointer (unused for now, we just write a test message)
/// - arg2: length
pub fn sys_write(_buf: u64, len: u64, _arg3: u64, _arg4: u64, _arg5: u64, _arg6: u64) -> SyscallResult {
    // For now, just write a test message
    println!("[SYSCALL] sys_write called with len={}", len);
    len as SyscallResult
}

/// sys_read - Read from keyboard
pub fn sys_read(_buf: u64, _len: u64, _arg3: u64, _arg4: u64, _arg5: u64, _arg6: u64) -> SyscallResult {
    debug_println!("[SYSCALL] sys_read not implemented yet");
    ERR_NOT_IMPLEMENTED
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
    ERR_NOT_IMPLEMENTED
}

/// sys_dealloc - Deallocate memory
pub fn sys_dealloc(ptr: u64, _arg2: u64, _arg3: u64, _arg4: u64, _arg5: u64, _arg6: u64) -> SyscallResult {
    debug_println!("[SYSCALL] sys_dealloc not implemented yet (ptr=0x{:x})", ptr);
    ERR_NOT_IMPLEMENTED
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
        return ERR_INVALID_SYSCALL;
    }
    
    debug_println!(
        "[SYSCALL] Dispatching syscall {} with args=({}, {}, {}, {}, {}, {})",
        syscall_num, arg1, arg2, arg3, arg4, arg5, arg6
    );
    
    let handler = SYSCALL_TABLE[num];
    handler(arg1, arg2, arg3, arg4, arg5, arg6)
}
