//! System Call Mechanism for x86_64
//!
//! This module implements the syscall/sysret mechanism for transitioning
//! between Ring 3 (user mode) and Ring 0 (kernel mode).

#![allow(unsafe_op_in_unsafe_fn)] // naked_asm! requires this

use x86_64::VirtAddr;
use x86_64::registers::model_specific::{Efer, EferFlags, LStar, Star, SFMask};
use x86_64::registers::rflags::RFlags;
use crate::arch::x86_64::gdt;
use crate::debug_println;

/// Initialize the syscall mechanism
///
/// This sets up the Model Specific Registers (MSRs) required for
/// the `syscall` and `sysret` instructions to work properly.
pub fn init() {
    unsafe {
        // Enable syscall/sysret in EFER
        Efer::update(|flags| {
            *flags |= EferFlags::SYSTEM_CALL_EXTENSIONS;
        });
        
        // Set up STAR register (kernel and user segment selectors)
        let selectors = gdt::selectors();
        
        // Use the Star::write method to set up segment selectors
        Star::write(
            selectors.kernel_code,
            selectors.kernel_data,
            selectors.user_code,
            selectors.user_data,
        ).unwrap();
        
        // Set up LSTAR register (syscall entry point)
        LStar::write(VirtAddr::new(syscall_entry as *const () as u64));
        
        // Set up SFMASK register (RFLAGS bits to clear on syscall)
        // We clear the interrupt flag to disable interrupts during syscall handling
        SFMask::write(RFlags::INTERRUPT_FLAG);
        
        let kernel_cs = selectors.kernel_code.0 as u64;
        let user_cs = selectors.user_code.0 as u64;
        debug_println!("[OK] Syscall mechanism initialized");
        debug_println!("  STAR: kernel_cs=0x{:x}, user_cs=0x{:x}", kernel_cs, user_cs);
        debug_println!("  LSTAR: 0x{:x}", syscall_entry as *const () as u64);
    }
}

/// Syscall entry point
///
/// This is called when userspace executes the `syscall` instruction.
/// 
/// Register state on entry (x86_64 calling convention):
/// - RAX: syscall number
/// - RDI, RSI, RDX, R10, R8, R9: arguments 1-6
/// - RCX: user RIP (saved by CPU)
/// - R11: user RFLAGS (saved by CPU)
/// - RSP: **still pointing to user stack** (not switched by CPU!)
///
/// The syscall instruction does NOT switch RSP automatically.
/// We must:
/// 1. Switch to kernel stack
/// 2. Save user registers on kernel stack
/// 3. Call the syscall handler
/// 4. Restore user registers from kernel stack
/// 5. Switch back to user stack
/// 6. Return to user mode with sysret
#[unsafe(naked)]
pub unsafe extern "C" fn syscall_entry() -> ! {
    core::arch::naked_asm!(
        // At this point:
        // - CS/SS have been switched to kernel segments by CPU
        // - RCX = user RIP, R11 = user RFLAGS (saved by CPU)
        // - RSP still points to USER stack (dangerous!)
        
        // Save user RSP in a scratch register
        "mov r15, rsp",
        
        // Switch to kernel stack from TSS
        // Note: In a future per-process implementation, this will be 
        // loaded from the current process's kernel stack pointer.
        // For now, we use the static stack set up in TSS.privilege_stack_table[0]
        // 
        // The kernel stack address is stored at offset 0x04 in the TSS
        // TSS layout: [0x00-0x03]=reserved, [0x04-0x0B]=RSP0, ...
        // We access it via gs segment (set up during CPU init)
        // 
        // WORKAROUND: x86_64 crate doesn't expose TSS.privilege_stack_table[0]
        // directly in a way accessible from assembly. We'll use a static variable.
        // Load kernel stack pointer from our static variable
        "mov rsp, qword ptr [rip + {kernel_stack}]",
        
        // Now RSP points to kernel stack - safe to use push instructions
        
        // Save user context on kernel stack
        "push r15",          // User RSP (saved earlier)
        "push rcx",          // User RIP (saved by CPU on syscall)
        "push r11",          // User RFLAGS (saved by CPU on syscall)
        
        // Save callee-saved registers
        "push rbp",
        "push rbx",
        "push r12",
        "push r13",
        "push r14",
        // r15 was clobbered above, but we don't need to save it since
        // it's caller-saved and will be handled by the C calling convention
        
        // Arguments are already in the right registers for C calling convention:
        // rax = syscall number
        // rdi, rsi, rdx = args 1-3
        // r10 = arg4 (need to move to rcx for C convention)
        // r8, r9 = args 5-6
        "mov rcx, r10",      // Move 4th arg from r10 to rcx for C ABI
        
        // Align stack to 16-byte boundary (required by System V ABI)
        "and rsp, -16",
        
        // Call the syscall handler
        "call {syscall_handler}",
        
        // Result is in RAX, preserve it
        
        // Restore callee-saved registers
        "pop r14",
        "pop r13",
        "pop r12",
        "pop rbx",
        "pop rbp",
        
        // Restore user context
        "pop r11",          // User RFLAGS
        "pop rcx",          // User RIP
        "pop r15",          // User RSP
        
        // Switch back to user stack
        "mov rsp, r15",
        
        // Return to user mode
        // sysretq will:
        // - Load RCX into RIP (user return address)
        // - Load R11 into RFLAGS
        // - Switch CS/SS back to user segments
        "sysretq",
        
        kernel_stack = sym KERNEL_SYSCALL_STACK,
        syscall_handler = sym syscall_handler,
    );
}

// Temporary kernel stack for syscalls
// TODO: Replace with per-process kernel stacks in Phase 2
#[repr(C, align(16))]
struct KernelStack {
    data: [u8; 8192], // 8KB stack
}

static mut KERNEL_STACK: KernelStack = KernelStack {
    data: [0; 8192],
};

// Pointer to top of kernel stack (stacks grow downward)
// Use addr_of! to avoid creating a reference to static mut
fn get_kernel_stack_top() -> usize {
    unsafe {
        let stack_ptr = core::ptr::addr_of!(KERNEL_STACK);
        let data_ptr = core::ptr::addr_of!((*stack_ptr).data);
        (data_ptr as *const u8).add(8192) as usize
    }
}

// Static variable storing the kernel stack pointer
use lazy_static::lazy_static;
lazy_static! {
    static ref KERNEL_SYSCALL_STACK: usize = get_kernel_stack_top();
}


/// Syscall handler dispatcher
///
/// This function is called from the syscall entry point and dispatches
/// to the appropriate syscall implementation based on the syscall number.
///
/// # Safety
///
/// This function is called from assembly and must maintain the syscall
/// calling convention.
#[unsafe(no_mangle)]
extern "C" fn syscall_handler(
    syscall_num: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    arg4: u64,
    arg5: u64,
    arg6: u64,
) -> u64 {
    // Call the syscall dispatcher
    let result = crate::kernel::syscall::dispatch(
        syscall_num, arg1, arg2, arg3, arg4, arg5, arg6
    );
    
    // Convert i64 result to u64 for return
    result as u64
}

/// Syscall numbers
#[repr(u64)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyscallNumber {
    /// Write to console
    Write = 0,
    /// Read from keyboard
    Read = 1,
    /// Exit current process
    Exit = 2,
    /// Get process ID
    GetPid = 3,
    /// Allocate memory
    Alloc = 4,
    /// Deallocate memory
    Dealloc = 5,
}

