// kernel/src/arch/x86_64/syscall_fast.rs
//! Optimized syscall entry for io_uring-style batched operations
//!
//! This module provides an alternative, streamlined syscall entry point
//! specifically designed for io_uring operations where:
//! 
//! 1. Arguments are minimal (just a doorbell signal)
//! 2. The kernel reads requests from shared memory ring buffers
//! 3. Register save/restore overhead is minimized
//!
//! # Future Optimization: swapgs
//! 
//! When GS base is properly set up, we can use `swapgs` for faster
//! kernel stack access:
//! 
//! ```asm
//! swapgs                    ; Switch to kernel GS base
//! mov rsp, gs:[0]           ; Load kernel stack from GS-relative address
//! ; ... process ...
//! swapgs                    ; Switch back to user GS base
//! sysretq
//! ```
//!
//! This avoids the RIP-relative load of CURRENT_KERNEL_STACK.
//!
//! # io_uring Specific Fast Path
//!
//! For io_uring_enter with just a doorbell signal:
//! - RAX = syscall number (13 for io_uring_enter)
//! - RDI = fd (can be ignored if we have one ring per process)
//! - RSI = to_submit (0 = process all)
//! - RDX = min_complete (0 = non-blocking)
//! 
//! No need to shuffle 6 arguments - we can call handler directly with fewer args.

#![allow(unsafe_op_in_unsafe_fn)]

use x86_64::VirtAddr;
use crate::debug_println;

/// Fast path syscall numbers that bypass full argument passing
pub const SYSCALL_IO_URING_ENTER: u64 = 13;
pub const SYSCALL_IO_URING_SETUP: u64 = 12;

/// Minimal syscall entry for io_uring operations
///
/// This is a potential future optimization for io_uring-heavy workloads.
/// Currently not used - keeping as documentation of optimization path.
///
/// # Safety
/// Same requirements as regular syscall_entry.
///
/// # Optimization Notes
/// 
/// Compared to standard syscall_entry, this version:
/// - Saves only essential registers (user RSP, RIP, RFLAGS)
/// - Doesn't shuffle 6 arguments through registers
/// - Calls specialized handler directly
///
/// ```text
/// Standard syscall_entry:       Fast io_uring path:
/// ─────────────────────────     ─────────────────────
/// push r15 (user rsp)           push r15 (user rsp)
/// push rcx (user rip)           push rcx (user rip)  
/// push r11 (user rflags)        push r11 (user rflags)
/// push rbp                      ; skip callee-saved
/// push rbx                      ; for short handlers
/// push r12
/// push r13
/// push r14
/// shuffle 6 args                mov rdi, rax ; syscall num only
/// call handler                  call io_uring_fast_handler
/// pop r14                       ; skip restore
/// pop r13
/// pop r12
/// pop rbx
/// pop rbp
/// pop r11                       pop r11
/// pop rcx                       pop rcx
/// pop r15                       pop r15
/// sysretq                       sysretq
/// ```
/// 
/// This reduces ~25 instructions to ~15 for the fast path.
#[allow(dead_code)]
#[unsafe(naked)]
pub unsafe extern "C" fn syscall_entry_fast() {
    core::arch::naked_asm!(
        // Save user RSP
        "mov r15, rsp",
        
        // Switch to kernel stack
        "mov rsp, qword ptr [rip + {current_stack}]",
        
        // Quick alignment check
        "and rsp, -16",
        
        // Minimal save: just what we need for sysret
        "push r15",          // User RSP
        "push rcx",          // User RIP
        "push r11",          // User RFLAGS
        
        // For io_uring, we don't need to pass all 6 arguments
        // Just pass syscall number in RDI
        // The handler will read ring buffers from current process
        "mov rdi, rax",      // syscall number
        "mov rsi, rsi",      // to_submit (already in RSI)
        "mov rdx, rdx",      // min_complete (already in RDX)
        
        // Call fast handler
        "call {fast_handler}",
        
        // Restore and return
        "pop r11",           // User RFLAGS
        "pop rcx",           // User RIP
        "pop r15",           // User RSP
        "mov rsp, r15",      // Switch back to user stack
        
        "sysretq",
        
        current_stack = sym super::syscall::CURRENT_KERNEL_STACK,
        fast_handler = sym io_uring_fast_handler,
    );
}

/// Fast handler for io_uring operations
/// 
/// This bypasses the standard dispatch table for io_uring syscalls.
#[unsafe(no_mangle)]
extern "C" fn io_uring_fast_handler(syscall_num: u64, to_submit: u64, min_complete: u64) -> u64 {
    use crate::kernel::process::PROCESS_TABLE;
    
    match syscall_num {
        SYSCALL_IO_URING_ENTER => {
            let mut table = PROCESS_TABLE.lock();
            let process = match table.current_process_mut() {
                Some(p) => p,
                None => return (-3_i64) as u64, // ESRCH
            };
            
            let ctx = match process.io_uring_mut() {
                Some(ctx) => ctx,
                None => return (-22_i64) as u64, // EINVAL
            };
            
            let completed = ctx.enter(min_complete as u32);
            completed as u64
        }
        
        SYSCALL_IO_URING_SETUP => {
            let mut table = PROCESS_TABLE.lock();
            let process = match table.current_process_mut() {
                Some(p) => p,
                None => return (-3_i64) as u64, // ESRCH
            };
            
            let ctx = process.io_uring_setup();
            ctx.sq_header_addr()
        }
        
        _ => {
            // Fall back to returning ENOSYS for unknown syscalls
            (-38_i64) as u64
        }
    }
}

/// Statistics for syscall performance monitoring
#[derive(Debug, Default)]
pub struct SyscallStats {
    /// Total syscalls processed
    pub total: u64,
    /// io_uring_enter calls
    pub io_uring_enter: u64,
    /// io_uring_setup calls  
    pub io_uring_setup: u64,
    /// Standard (slow path) syscalls
    pub standard: u64,
}

impl SyscallStats {
    /// Create new stats
    pub const fn new() -> Self {
        Self {
            total: 0,
            io_uring_enter: 0,
            io_uring_setup: 0,
            standard: 0,
        }
    }
}

// Global stats (could be made per-CPU for better scalability)
use spin::Mutex;
use lazy_static::lazy_static;

lazy_static! {
    /// Global syscall statistics
    pub static ref SYSCALL_STATS: Mutex<SyscallStats> = Mutex::new(SyscallStats::new());
}

/// Record a syscall for statistics
#[allow(dead_code)]
pub fn record_syscall(syscall_num: u64) {
    let mut stats = SYSCALL_STATS.lock();
    stats.total += 1;
    
    match syscall_num {
        SYSCALL_IO_URING_ENTER => stats.io_uring_enter += 1,
        SYSCALL_IO_URING_SETUP => stats.io_uring_setup += 1,
        _ => stats.standard += 1,
    }
}

/// Print syscall statistics
#[allow(dead_code)]
pub fn print_stats() {
    let stats = SYSCALL_STATS.lock();
    debug_println!("=== Syscall Statistics ===");
    debug_println!("Total:           {}", stats.total);
    debug_println!("io_uring_enter:  {}", stats.io_uring_enter);
    debug_println!("io_uring_setup:  {}", stats.io_uring_setup);
    debug_println!("Standard:        {}", stats.standard);
    
    if stats.total > 0 {
        let io_uring_pct = (stats.io_uring_enter + stats.io_uring_setup) * 100 / stats.total;
        debug_println!("io_uring ratio:  {}%", io_uring_pct);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_stats() {
        let mut stats = SyscallStats::new();
        assert_eq!(stats.total, 0);
        
        stats.total = 100;
        stats.io_uring_enter = 80;
        stats.standard = 20;
        
        assert_eq!(stats.total, 100);
    }
}
