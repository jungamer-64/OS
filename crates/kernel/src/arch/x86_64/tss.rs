//! Task State Segment (TSS) management
//!
//! The TSS is used by the CPU to automatically switch to kernel stacks
//! during privilege level transitions (e.g., syscalls, interrupts).

use x86_64::VirtAddr;
use x86_64::structures::tss::TaskStateSegment;
use lazy_static::lazy_static;
use spin::Mutex;

lazy_static! {
    /// Global Task State Segment
    ///
    /// The TSS contains the privilege stack table which is used by the CPU
    /// to determine which kernel stack to use when transitioning from user mode.
    pub static ref TSS: Mutex<TaskStateSegment> = {
        let mut tss = TaskStateSegment::new();
        
        // Initialize privilege_stack_table[0] with a temporary value
        // This will be updated when the first process is created
        // For now, use a dummy address (will be replaced before use)
        tss.privilege_stack_table[0] = VirtAddr::new(0);
        
        // The interrupt stack table is used for specific interrupts that need
        // dedicated stacks (e.g., double fault, NMI)
        // We'll set these up later if needed
        
        Mutex::new(tss)
    };
}

/// Update the kernel stack pointer in the TSS
///
/// This function should be called whenever switching to a new process
/// to ensure that syscalls and interrupts use the correct kernel stack.
///
/// # Arguments
///
/// * `stack_top` - The top of the kernel stack for the current process
///
/// # Safety
///
/// The stack_top must point to a valid, properly aligned kernel stack.
pub fn update_kernel_stack(stack_top: VirtAddr) {
    let mut tss = TSS.lock();
    tss.privilege_stack_table[0] = stack_top;
    
    crate::debug_println!(
        "[TSS] Updated kernel stack to 0x{:x}",
        stack_top.as_u64()
    );
}

/// Get the current kernel stack pointer from the TSS
///
/// This is primarily for debugging and verification purposes.
#[allow(dead_code)]
pub fn get_kernel_stack() -> VirtAddr {
    let tss = TSS.lock();
    tss.privilege_stack_table[0]
}

/// Initialize TSS (called during boot)
///
/// This function is called once during system initialization.
/// The actual kernel stack values will be set when processes are created.
pub fn init() {
    crate::debug_println!("[TSS] Task State Segment initialized");
}
