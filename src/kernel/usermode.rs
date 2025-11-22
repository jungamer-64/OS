// src/kernel/usermode.rs
//! Usermode execution support (Phase 1)
//!
//! Provides simple Ring 0 -> Ring 3 transition for testing purposes.
//! This is a simplified version that doesn't require full process management.

use x86_64::VirtAddr;
use x86_64::registers::rflags::RFlags;
use crate::arch::x86_64::gdt;
use crate::debug_println;

/// Simple usermode transition (Phase 1)
///
/// This version doesn't use full process management, just tests the Ring 0 -> Ring 3 transition.
///
/// # Safety
/// This function performs a direct Ring 0 -> Ring 3 transition using iretq.
/// It never returns.
pub unsafe fn jump_to_usermode_simple(entry_point: VirtAddr, user_stack: VirtAddr) -> ! {
    // Get GDT selectors
    let selectors = gdt::selectors();
    
    // User segment selectors (with RPL=3)
    let user_data_selector: u64 = u64::from(selectors.user_data.0);
    let user_code_selector: u64 = u64::from(selectors.user_code.0);
    
    // RFLAGS with interrupts enabled
    let rflags = RFlags::INTERRUPT_FLAG.bits();
    
    debug_println!("[Jump] Entry=0x{:x}, Stack=0x{:x}", entry_point.as_u64(), user_stack.as_u64());
    debug_println!("[Jump] CS=0x{:x}, DS=0x{:x}", user_code_selector, user_data_selector);
    
    // Perform Ring 0 -> Ring 3 transition
    // SAFETY: Caller ensures entry_point and user_stack are valid Ring 3 addresses
    unsafe {
        core::arch::asm!(
            // Set up data segments for Ring 3
            "mov ds, {0:x}",
            "mov es, {0:x}",
            "mov fs, {0:x}",
            "mov gs, {0:x}",
            
            // Push iretq frame (5 values)
            // Stack grows downward, so push in reverse order:
            "push {0}",          // SS (user data selector)
            "push {1}",          // RSP (user stack pointer)
            "push {2}",          // RFLAGS
            "push {3}",          // CS (user code selector)
            "push {4}",          // RIP (entry point)
            
            // Return to user mode (Ring 3)
            "iretq",
            
            in(reg) user_data_selector,
            in(reg) user_stack.as_u64(),
            in(reg) rflags,
            in(reg) user_code_selector,
            in(reg) entry_point.as_u64(),
            options(noreturn)
        )
    }
}

/// Test usermode execution (Phase 1)
///
/// This function:
/// 1. Allocates a user stack
/// 2. Sets up the syscall kernel stack
/// 3. Jumps to `user_main()` in Ring 3
///
/// # Panics
/// Panics if user stack allocation fails.
///
/// # Safety
/// This function never returns. It transitions to Ring 3 and executes user code.
#[cfg(feature = "test_usermode")]
#[allow(unsafe_op_in_unsafe_fn)]
#[allow(clippy::items_after_statements)]
pub unsafe fn test_usermode_execution() -> ! {
    use crate::arch::x86_64::syscall;
    use alloc::alloc::{alloc_zeroed, Layout};
    
    debug_println!("\n=== Preparing to test usermode execution ===");
    
    // User program entry point
    // In Phase 1, we use a function linked into the kernel
    unsafe extern "C" {
        fn user_main() -> !;
    }
    
    let entry_point = VirtAddr::new(user_main as *const () as u64);
    debug_println!("User entry point: 0x{:x}", entry_point.as_u64());
    
    // Allocate user stack (64 KiB, 16-byte aligned)
    // Phase 1: Allocate from kernel heap (should be in user space in Phase 2)
    let stack_layout = Layout::from_size_align(64 * 1024, 16)
        .expect("Failed to create stack layout");
    
    let stack_bottom = alloc_zeroed(stack_layout);
    assert!(!stack_bottom.is_null(), "Failed to allocate user stack");
    
    let user_stack_top = VirtAddr::new(stack_bottom as u64 + 64 * 1024);
    debug_println!("User stack: 0x{:x}", user_stack_top.as_u64());
    
    // Kernel stack for syscalls (already initialized by syscall::init())
    let kernel_stack = syscall::get_kernel_stack();
    debug_println!("Kernel stack: 0x{:x}", kernel_stack.as_u64());
    
    debug_println!("Jumping to user mode...\n");
    
    // Jump to Ring 3
    jump_to_usermode_simple(entry_point, user_stack_top)
}
