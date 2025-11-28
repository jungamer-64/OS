// kernel/src/arch/x86_64/per_cpu.rs
//! Per-CPU Data Management
//!
//! This module implements per-CPU data structures using the GS segment register.
//! On x86_64, `swapgs` allows efficient switching between user and kernel GS bases,
//! enabling each CPU core to have its own private data area.
//!
//! # Architecture
//!
//! ```text
//! User Mode:   GS -> User TLS (thread-local storage)
//! Kernel Mode: GS -> PerCpuData (after swapgs)
//!
//! IA32_KERNEL_GS_BASE MSR: Stores the kernel's GS base
//! IA32_GS_BASE MSR:        Stores the user's GS base (or kernel when active)
//! ```
//!
//! # Usage
//!
//! During syscall entry:
//! 1. `swapgs` exchanges GS base values
//! 2. Kernel accesses Per-CPU data via `gs:[offset]`
//! 3. Before sysret, `swapgs` restores user GS
//!
//! # Memory Layout
//!
//! The `PerCpuData` structure is carefully designed for fast assembly access:
//! ```text
//! Offset 0x00: user_rsp_scratch   - Temporary storage for user RSP
//! Offset 0x08: kernel_stack_top   - Kernel stack pointer for this CPU
//! Offset 0x10: user_gs_base       - Saved user GS base (for nested operations)
//! Offset 0x18: cpu_id             - CPU identification number
//! Offset 0x20: current_task       - Pointer to current task/process
//! Offset 0x28: tss_rsp0           - TSS RSP0 value (for interrupt stack)
//! ...
//! ```

#![allow(dead_code)]

use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use x86_64::VirtAddr;
use x86_64::registers::model_specific::Msr;

/// MSR addresses for GS base management
const IA32_GS_BASE: u32 = 0xC0000101;
const IA32_KERNEL_GS_BASE: u32 = 0xC0000102;

/// Maximum number of CPUs supported
/// For now, we support single CPU but the structure is SMP-ready
pub const MAX_CPUS: usize = 256;

/// Per-CPU data structure
///
/// This structure is accessed via the GS segment register in kernel mode.
/// The field layout is critical - assembly code uses hardcoded offsets.
///
/// # Safety
///
/// The offsets defined in `PerCpuOffset` MUST match this struct's layout.
/// Any changes here require updating the assembly code in `syscall.rs`.
#[repr(C, align(64))] // Cache-line aligned for performance
pub struct PerCpuData {
    // === Syscall fast path fields (frequently accessed) ===
    // Keep these at the beginning for minimal offset calculations
    
    /// Scratch space for user RSP during syscall entry
    /// Offset: 0x00
    /// Used by: `mov gs:[0], rsp` in syscall_entry
    pub user_rsp_scratch: AtomicU64,
    
    /// Top of kernel stack for this CPU
    /// Offset: 0x08
    /// Used by: `mov rsp, gs:[8]` in syscall_entry
    pub kernel_stack_top: AtomicU64,
    
    /// Saved user GS base (for restore on kernel->user transition)
    /// Offset: 0x10
    /// Used for nested syscall handling or debugging
    pub user_gs_base: AtomicU64,
    
    /// CPU identification number (0-based)
    /// Offset: 0x18
    pub cpu_id: u64,
    
    // === Process management fields ===
    
    /// Pointer to current task/process structure
    /// Offset: 0x20
    /// Updated during context switch
    pub current_task: AtomicU64,
    
    /// TSS RSP0 value (kernel stack for privilege level transitions)
    /// Offset: 0x28
    /// This should match TSS.privilege_stack_table[0]
    pub tss_rsp0: AtomicU64,
    
    // === Statistics and debugging ===
    
    /// Total number of syscalls processed by this CPU
    /// Offset: 0x30
    pub syscall_count: AtomicU64,
    
    /// Timestamp of last syscall (for performance monitoring)
    /// Offset: 0x38
    pub last_syscall_time: AtomicU64,
    
    // === Cache-line padding ===
    // Pad to next cache line to avoid false sharing
    _padding: [u64; 4],
}

/// Offsets into PerCpuData structure
/// These MUST match the actual struct layout!
pub mod offset {
    /// Offset of `user_rsp_scratch` field
    pub const USER_RSP_SCRATCH: usize = 0x00;
    /// Offset of `kernel_stack_top` field  
    pub const KERNEL_STACK_TOP: usize = 0x08;
    /// Offset of `user_gs_base` field
    pub const USER_GS_BASE: usize = 0x10;
    /// Offset of `cpu_id` field
    pub const CPU_ID: usize = 0x18;
    /// Offset of `current_task` field
    pub const CURRENT_TASK: usize = 0x20;
    /// Offset of `tss_rsp0` field
    pub const TSS_RSP0: usize = 0x28;
    /// Offset of `syscall_count` field
    pub const SYSCALL_COUNT: usize = 0x30;
    /// Offset of `last_syscall_time` field
    pub const LAST_SYSCALL_TIME: usize = 0x38;
}

impl PerCpuData {
    /// Create a new Per-CPU data structure
    const fn new(cpu_id: u64) -> Self {
        Self {
            user_rsp_scratch: AtomicU64::new(0),
            kernel_stack_top: AtomicU64::new(0),
            user_gs_base: AtomicU64::new(0),
            cpu_id,
            current_task: AtomicU64::new(0),
            tss_rsp0: AtomicU64::new(0),
            syscall_count: AtomicU64::new(0),
            last_syscall_time: AtomicU64::new(0),
            _padding: [0; 4],
        }
    }
    
    /// Set the kernel stack top for this CPU
    pub fn set_kernel_stack(&self, stack_top: VirtAddr) {
        self.kernel_stack_top.store(stack_top.as_u64(), Ordering::Release);
        self.tss_rsp0.store(stack_top.as_u64(), Ordering::Release);
    }
    
    /// Get the kernel stack top for this CPU
    pub fn get_kernel_stack(&self) -> VirtAddr {
        VirtAddr::new(self.kernel_stack_top.load(Ordering::Acquire))
    }
    
    /// Set the current task pointer
    pub fn set_current_task(&self, task_ptr: u64) {
        self.current_task.store(task_ptr, Ordering::Release);
    }
    
    /// Get the current task pointer
    pub fn get_current_task(&self) -> u64 {
        self.current_task.load(Ordering::Acquire)
    }
    
    /// Increment syscall counter
    pub fn inc_syscall_count(&self) {
        self.syscall_count.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Get syscall count
    pub fn get_syscall_count(&self) -> u64 {
        self.syscall_count.load(Ordering::Relaxed)
    }
}

// Compile-time layout verification
const _: () = {
    use core::mem::offset_of;
    
    assert!(offset_of!(PerCpuData, user_rsp_scratch) == offset::USER_RSP_SCRATCH);
    assert!(offset_of!(PerCpuData, kernel_stack_top) == offset::KERNEL_STACK_TOP);
    assert!(offset_of!(PerCpuData, user_gs_base) == offset::USER_GS_BASE);
    assert!(offset_of!(PerCpuData, cpu_id) == offset::CPU_ID);
    assert!(offset_of!(PerCpuData, current_task) == offset::CURRENT_TASK);
    assert!(offset_of!(PerCpuData, tss_rsp0) == offset::TSS_RSP0);
    assert!(offset_of!(PerCpuData, syscall_count) == offset::SYSCALL_COUNT);
    assert!(offset_of!(PerCpuData, last_syscall_time) == offset::LAST_SYSCALL_TIME);
};

/// Static Per-CPU data array
/// In a full SMP implementation, each CPU would have its own entry
static mut PER_CPU_DATA: [PerCpuData; MAX_CPUS] = {
    // Initialize all entries
    // Note: We can't use a loop in const context, so we use a macro
    const INIT: PerCpuData = PerCpuData::new(0);
    [INIT; MAX_CPUS]
};

/// Boot CPU index (will be dynamically determined in full SMP)
static BOOT_CPU_ID: AtomicUsize = AtomicUsize::new(0);

/// Kernel stack for boot CPU (fallback)
#[repr(C, align(16))]
struct BootKernelStack {
    data: [u8; 16384], // 16KB stack
}

static mut BOOT_KERNEL_STACK: BootKernelStack = BootKernelStack {
    data: [0; 16384],
};

/// Get the Per-CPU data for the current CPU
///
/// # Safety
///
/// This function reads from the GS segment, which must be properly initialized.
/// Call only after `init()` has been called for this CPU.
#[inline(always)]
pub unsafe fn current() -> &'static PerCpuData {
    // In SMP, we'd read the CPU ID from GS and index into the array
    // For single CPU, we just return the first entry
    // SAFETY: We're in an unsafe function and caller ensures Per-CPU is initialized
    unsafe { &*core::ptr::addr_of!(PER_CPU_DATA[0]) }
}

/// Get the Per-CPU data for a specific CPU
///
/// # Safety
///
/// The cpu_id must be valid and the CPU must be initialized.
#[allow(dead_code)]
pub unsafe fn get(cpu_id: usize) -> &'static PerCpuData {
    assert!(cpu_id < MAX_CPUS, "Invalid CPU ID");
    // SAFETY: We're in an unsafe function and caller ensures validity
    unsafe { &*core::ptr::addr_of!(PER_CPU_DATA[cpu_id]) }
}

/// Get a mutable reference to Per-CPU data
///
/// # Safety
///
/// Must ensure exclusive access (typically during initialization)
unsafe fn get_mut(cpu_id: usize) -> &'static mut PerCpuData {
    assert!(cpu_id < MAX_CPUS, "Invalid CPU ID");
    // SAFETY: We're in an unsafe function and caller ensures exclusive access
    unsafe { &mut *core::ptr::addr_of_mut!(PER_CPU_DATA[cpu_id]) }
}

/// Initialize Per-CPU data for the boot CPU
///
/// This must be called early in the boot process, before any syscalls.
/// It sets up the GS segment and kernel stack for the current CPU.
pub fn init() {
    let cpu_id = 0_usize; // Boot CPU is always 0
    BOOT_CPU_ID.store(cpu_id, Ordering::Release);
    
    unsafe {
        // Get mutable reference to our CPU's data
        let per_cpu = get_mut(cpu_id);
        
        // Set CPU ID
        // Note: cpu_id field is not atomic, so we do this during init only
        let data_ptr = per_cpu as *mut PerCpuData as *mut u8;
        let cpu_id_ptr = data_ptr.add(offset::CPU_ID) as *mut u64;
        core::ptr::write_volatile(cpu_id_ptr, cpu_id as u64);
        
        // Set up boot kernel stack
        let stack_ptr = core::ptr::addr_of!(BOOT_KERNEL_STACK);
        let stack_top = (stack_ptr as *const u8).add(16384) as u64;
        
        // Ensure 16-byte alignment
        let stack_top_aligned = stack_top & !0xF;
        
        per_cpu.set_kernel_stack(VirtAddr::new(stack_top_aligned));
        
        // Calculate the address of this CPU's Per-CPU data
        let per_cpu_addr = per_cpu as *const PerCpuData as u64;
        
        // Set up IA32_KERNEL_GS_BASE MSR
        // This is the GS base that will be active after `swapgs` in syscall entry
        let mut kernel_gs_base = Msr::new(IA32_KERNEL_GS_BASE);
        kernel_gs_base.write(per_cpu_addr);
        
        // Set current GS base to 0 (user default)
        // On first syscall, swapgs will swap these
        let mut gs_base = Msr::new(IA32_GS_BASE);
        gs_base.write(0);
        
        crate::debug_println!("[Per-CPU] Initialized for CPU {}", cpu_id);
        crate::debug_println!("  Per-CPU data at: {:#x}", per_cpu_addr);
        crate::debug_println!("  Kernel stack top: {:#x}", stack_top_aligned);
        crate::debug_println!("  IA32_KERNEL_GS_BASE: {:#x}", per_cpu_addr);
    }
}

/// Update the kernel stack for the current CPU
///
/// Called during process context switch to set the kernel stack
/// for the new process.
pub fn update_kernel_stack(stack_top: VirtAddr) {
    unsafe {
        let per_cpu = current();
        per_cpu.set_kernel_stack(stack_top);
        
        // Also update TSS for interrupt stack switching
        super::tss::update_kernel_stack(stack_top);
    }
}

/// Get the current kernel stack pointer
pub fn get_kernel_stack() -> VirtAddr {
    unsafe {
        current().get_kernel_stack()
    }
}

/// Get the Per-CPU data address (for debugging)
pub fn get_per_cpu_addr() -> u64 {
    unsafe {
        let per_cpu = current();
        per_cpu as *const PerCpuData as u64
    }
}

/// Read the current IA32_KERNEL_GS_BASE value (for debugging)
pub fn read_kernel_gs_base() -> u64 {
    unsafe {
        let msr = Msr::new(IA32_KERNEL_GS_BASE);
        msr.read()
    }
}

/// Read the current IA32_GS_BASE value (for debugging)
pub fn read_gs_base() -> u64 {
    unsafe {
        let msr = Msr::new(IA32_GS_BASE);
        msr.read()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test_case]
    fn test_per_cpu_layout() {
        // Verify struct size is reasonable
        assert!(core::mem::size_of::<PerCpuData>() <= 128);
        
        // Verify alignment
        assert!(core::mem::align_of::<PerCpuData>() >= 64);
    }
}
