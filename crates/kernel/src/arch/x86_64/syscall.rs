// kernel/src/arch/x86_64/syscall.rs
//! System Call Mechanism for `x86_64`
//!
//! This module implements the syscall/sysret mechanism for transitioning
//! between Ring 3 (user mode) and Ring 0 (kernel mode).
//!
//! # Architecture (Phase 3: swapgs-based Per-CPU)
//!
//! Uses `swapgs` instruction for SMP-safe kernel entry:
//! 1. On syscall entry, `swapgs` swaps GS base with IA32_KERNEL_GS_BASE
//! 2. Kernel accesses Per-CPU data via `gs:[offset]`
//! 3. On sysret, `swapgs` restores user GS base
//!
//! This eliminates the global `CURRENT_KERNEL_STACK` variable and enables
//! true multi-core support.

#![allow(unsafe_op_in_unsafe_fn)] // naked_asm! requires this

use x86_64::VirtAddr;
use x86_64::registers::model_specific::{Efer, EferFlags, LStar, SFMask};
use x86_64::registers::rflags::RFlags;
use crate::arch::x86_64::gdt;
use crate::debug_println;

/// Syscall mode selection
/// 
/// Determines which syscall entry point is used:
/// - `Traditional`: Full register save/restore (compatible with all syscalls)
/// - `RingBased`: Doorbell-only entry (requires ring buffer setup)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyscallMode {
    /// Traditional syscall with full register save/restore
    Traditional,
    /// Ring-based syscall (doorbell only, no arguments)
    RingBased,
}

/// Current syscall mode (set during initialization)
static SYSCALL_MODE: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);

/// Get the current syscall mode
pub fn current_mode() -> SyscallMode {
    match SYSCALL_MODE.load(core::sync::atomic::Ordering::Relaxed) {
        1 => SyscallMode::RingBased,
        _ => SyscallMode::Traditional,
    }
}

/// Initialize the syscall mechanism
///
/// This sets up the Model Specific Registers (MSRs) required for
/// the `syscall` and `sysret` instructions to work properly.
/// 
/// # Arguments
/// 
/// * `mode` - Which syscall entry point to use (Traditional or RingBased)
/// 
/// # Panics
/// 
/// Panics if `Star::write` fails (should never happen with valid GDT selectors).
#[allow(clippy::missing_panics_doc)] // Star::write panic is documented
pub fn init_with_mode(mode: SyscallMode) {
    unsafe {
        // Enable syscall/sysret in EFER
        Efer::update(|flags| {
            *flags |= EferFlags::SYSTEM_CALL_EXTENSIONS;
        });
        
        // Set up STAR register (kernel and user segment selectors)
        let selectors = gdt::selectors();
        
        // IMPORTANT: Star::write requires base selectors WITHOUT RPL bits
        // New SYSRET-compatible GDT layout:
        //   0x08: kernel_code
        //   0x10: user_data (with RPL=3 -> 0x13)
        //   0x18: user_code (with RPL=3 -> 0x1B)
        //   0x20: kernel_data
        // SYSRET will automatically set RPL=3 when returning to user mode
        
        use x86_64::structures::gdt::SegmentSelector;
        let user_code_base = SegmentSelector(selectors.user_code.0 & !0x03);
        let user_data_base = SegmentSelector(selectors.user_data.0 & !0x03);
        
        // STAR MSR format (Intel SDM Vol. 2B, SYSCALL/SYSRET):
        // Bits [63:48]: SYSRET CS selector base (we use kernel_code = 0x08)
        // Bits [47:32]: SYSCALL CS selector (kernel_code = 0x08)
        // Bits [31:0]:  Reserved
        //
        // SYSRET behavior (64-bit mode):
        //   CS = STAR[63:48] + 16 = 0x08 + 16 = 0x18 (with RPL=3 -> 0x1B)
        //   SS = STAR[63:48] + 8  = 0x08 + 8  = 0x10 (with RPL=3 -> 0x13)
        //
        // Our GDT satisfies this: user_code at 0x18, user_data at 0x10
        
        debug_println!("[DEBUG] Setting up STAR MSR:");
        debug_println!("  kernel_code: 0x{:X}", selectors.kernel_code.0);
        debug_println!("  kernel_data: 0x{:X}", selectors.kernel_data.0);
        debug_println!("  user_code: 0x{:X} (base: 0x{:X})", selectors.user_code.0, user_code_base.0);
        debug_println!("  user_data: 0x{:X} (base: 0x{:X})", selectors.user_data.0, user_data_base.0);
        
        // Write STAR MSR manually
        use x86_64::registers::model_specific::Msr;
        let mut star = Msr::new(0xC0000081);
        
        // STAR[63:48] = kernel_code (SYSRET will add 16 for user CS, 8 for user SS)
        // STAR[47:32] = kernel_code (SYSCALL entry CS)
        let star_value = (u64::from(selectors.kernel_code.0) << 48) 
                       | (u64::from(selectors.kernel_code.0) << 32);
        
        star.write(star_value);
        debug_println!("[OK] STAR MSR written: 0x{:X}", star_value);
        
        // Set up LSTAR register based on mode
        let entry_point = match mode {
            SyscallMode::Traditional => {
                SYSCALL_MODE.store(0, core::sync::atomic::Ordering::Relaxed);
                syscall_entry as *const () as u64
            }
            SyscallMode::RingBased => {
                SYSCALL_MODE.store(1, core::sync::atomic::Ordering::Relaxed);
                super::syscall_ring::ideal_syscall_entry as *const () as u64
            }
        };
        
        LStar::write(VirtAddr::new(entry_point));
        
        // Set up SFMASK register (RFLAGS bits to clear on syscall)
        // We clear the interrupt flag to disable interrupts during syscall handling
        SFMask::write(RFlags::INTERRUPT_FLAG);
        
        // Initialize Per-CPU data (Phase 3: swapgs-based)
        // This sets up IA32_KERNEL_GS_BASE for the boot CPU
        super::per_cpu::init();
        
        let kernel_cs = u64::from(selectors.kernel_code.0);
        let user_cs = u64::from(selectors.user_code.0);
        let mode_str = match mode {
            SyscallMode::Traditional => "Traditional (full context)",
            SyscallMode::RingBased => "Ring-based (doorbell)",
        };
        debug_println!("[OK] Syscall mechanism initialized (swapgs-based)");
        debug_println!("  Mode: {}", mode_str);
        debug_println!("  STAR: kernel_cs=0x{:x}, user_cs=0x{:x}", kernel_cs, user_cs);
        debug_println!("  LSTAR: 0x{:x}", entry_point);
    }
}

/// Initialize the syscall mechanism with traditional mode
///
/// This sets up the Model Specific Registers (MSRs) required for
/// the `syscall` and `sysret` instructions to work properly.
/// 
/// Uses traditional syscall entry point with full register save/restore.
/// 
/// # Panics
/// 
/// Panics if `Star::write` fails (should never happen with valid GDT selectors).
#[allow(clippy::missing_panics_doc)] // Star::write panic is documented
pub fn init() {
    init_with_mode(SyscallMode::Traditional);
}

/// Switch syscall mode at runtime
///
/// # Safety
///
/// This function changes the syscall entry point. All processes must be
/// prepared to handle the new mode before calling this.
pub unsafe fn switch_mode(mode: SyscallMode) {
    let entry_point = match mode {
        SyscallMode::Traditional => {
            SYSCALL_MODE.store(0, core::sync::atomic::Ordering::Release);
            syscall_entry as *const () as u64
        }
        SyscallMode::RingBased => {
            SYSCALL_MODE.store(1, core::sync::atomic::Ordering::Release);
            super::syscall_ring::ideal_syscall_entry as *const () as u64
        }
    };
    
    LStar::write(VirtAddr::new(entry_point));
    
    let mode_str = match mode {
        SyscallMode::Traditional => "Traditional",
        SyscallMode::RingBased => "Ring-based",
    };
    debug_println!("[Syscall] Mode switched to: {} (LSTAR=0x{:x})", mode_str, entry_point);
}

/// Dump CPU registers for debugging
///
/// This function is used to print register values at critical points
/// during syscall handling (e.g., before iretq, after context switch).
///
/// # Safety
/// This function reads raw CPU registers and may contain invalid values.
/// It's intended for debugging only and should not be used in production.
#[cfg(debug_assertions)]
#[allow(dead_code)]
fn dump_registers(context: &str) {
    let (rsp, rbp, rax, rdi, rsi, rdx, cs, ss): (u64, u64, u64, u64, u64, u64, u16, u16);
    
    unsafe {
        core::arch::asm!(
            "mov {rsp}, rsp",
            "mov {rbp}, rbp",
            "mov {rax}, rax",
            "mov {rdi}, rdi",
            "mov {rsi}, rsi",
            "mov {rdx}, rdx",
            "mov {cs:x}, cs",
            "mov {ss:x}, ss",
            rsp = out(reg) rsp,
            rbp = out(reg) rbp,
            rax = out(reg) rax,
            rdi = out(reg) rdi,
            rsi = out(reg) rsi,
            rdx = out(reg) rdx,
            cs = out(reg) cs,
            ss = out(reg) ss,
        );
    }
    
    let ring = cs & 3;
    debug_println!("=== Register Dump: {} ===", context);
    debug_println!("  RSP: 0x{:016x}", rsp);
    debug_println!("  RBP: 0x{:016x}", rbp);
    debug_println!("  RAX: 0x{:016x}", rax);
    debug_println!("  RDI: 0x{:016x}", rdi);
    debug_println!("  RSI: 0x{:016x}", rsi);
    debug_println!("  RDX: 0x{:016x}", rdx);
    debug_println!("  CS:  0x{:04x} (Ring {})", cs, ring);
    debug_println!("  SS:  0x{:04x}", ss);
}

/// Syscall entry point (Phase 3: swapgs-based Per-CPU)
///
/// This is called when userspace executes the `syscall` instruction.
/// 
/// # Architecture
///
/// Uses `swapgs` to access Per-CPU data structure via GS segment:
/// - `gs:[0x00]` = user_rsp_scratch (temporary RSP storage)
/// - `gs:[0x08]` = kernel_stack_top
///
/// This design is SMP-safe: each CPU has its own Per-CPU data.
/// 
/// # Safety
/// This function is unsafe because it directly manipulates CPU registers
/// and must maintain the syscall calling convention.
/// 
/// Register state on entry (x86-64 calling convention):
/// - RAX: syscall number
/// - RDI, RSI, RDX, R10, R8, R9: arguments 1-6
/// - RCX: user RIP (saved by CPU)
/// - R11: user RFLAGS (saved by CPU)
/// - RSP: **still pointing to user stack** (not switched by CPU!)
#[unsafe(naked)]
pub unsafe extern "C" fn syscall_entry() {
    core::arch::naked_asm!(
        // ============================================================
        // Phase 1: GS segment switch (swapgs)
        // ============================================================
        // Swap GS base: User GS <-> Kernel GS (IA32_KERNEL_GS_BASE)
        // After this, GS points to Per-CPU data for this CPU
        "swapgs",
        
        // ============================================================
        // Phase 2: Save user RSP and switch to kernel stack
        // ============================================================
        // Save user RSP to Per-CPU scratch area
        // gs:[0x00] = PerCpuData.user_rsp_scratch
        "mov qword ptr gs:[0x00], rsp",
        
        // Load kernel stack from Per-CPU data
        // gs:[0x08] = PerCpuData.kernel_stack_top
        "mov rsp, qword ptr gs:[0x08]",
        
        // ============================================================
        // Phase 3: Stack alignment verification (debug mode)
        // ============================================================
        // Kernel stack should already be 16-byte aligned from initialization
        // But verify and align if necessary (should not happen)
        "test rsp, 15",
        "jz 2f",
        "and rsp, -16",
        "2:",
        
        // ============================================================
        // Phase 4: Save minimal context
        // ============================================================
        // Only save what's needed for sysret:
        // - User RSP (from scratch area)
        // - User RIP (RCX, saved by CPU)
        // - User RFLAGS (R11, saved by CPU)
        //
        // Callee-saved registers (rbp, rbx, r12-r14) are handled by
        // the Rust compiler in syscall_handler.
        "push qword ptr gs:[0x00]",  // User RSP (3rd from top)
        "push rcx",                   // User RIP (2nd from top)  
        "push r11",                   // User RFLAGS (top of saved context)
        
        // ============================================================
        // Phase 5: C ABI stack alignment
        // ============================================================
        // System V AMD64 ABI requires RSP to be 16-byte aligned at 'call'.
        // After 3 pushes (24 bytes), we need 8 more for alignment.
        "sub rsp, 8",
        
        // ============================================================
        // Phase 6: Prepare arguments for syscall_handler
        // ============================================================
        // Syscall ABI -> C ABI register mapping:
        //   RAX -> RDI (syscall number)
        //   RDI -> RSI (arg1)
        //   RSI -> RDX (arg2)
        //   RDX -> RCX (arg3)
        //   R10 -> R8  (arg4)
        //   R8  -> R9  (arg5)
        //   R9  -> stack (arg6)
        
        // Push arg6 to stack
        "push r9",
        "sub rsp, 8",  // Alignment padding
        
        // Shuffle registers (order matters!)
        "mov r9, r8",    // arg5
        "mov r8, r10",   // arg4
        "mov rcx, rdx",  // arg3
        "mov rdx, rsi",  // arg2
        "mov rsi, rdi",  // arg1
        "mov rdi, rax",  // syscall number
        
        // ============================================================
        // Phase 7: Call Rust handler
        // ============================================================
        "call {syscall_handler}",
        
        // ============================================================
        // Phase 8: Clean up and restore
        // ============================================================
        // Result is in RAX
        
        // Remove call adjustments
        "add rsp, 24",   // padding(8) + arg6(8) + alignment(8)
        
        // Restore user context
        "pop r11",       // User RFLAGS
        "pop rcx",       // User RIP
        "pop rsp",       // User RSP (direct restore, no need for scratch)
        
        // ============================================================
        // Phase 9: Return to user mode
        // ============================================================
        // Restore user GS base before returning
        "swapgs",
        
        // sysretq will:
        // - Load RCX into RIP
        // - Load R11 into RFLAGS
        // - Switch CS/SS to user segments
        "sysretq",
        
        syscall_handler = sym syscall_handler,
    );
}

// ============================================================================
// Kernel Stack Management (Phase 3: Per-CPU based)
// ============================================================================
//
// Stack management has been moved to per_cpu.rs for SMP support.
// These functions provide backwards-compatible API for existing code.
//
// Phase 3 changes:
// - Stack pointer is now stored in PerCpuData.kernel_stack_top
// - Accessed via GS segment in syscall_entry (gs:[0x08])
// - Each CPU has its own stack, eliminating race conditions

/// Update the current kernel stack pointer (Phase 3)
/// 
/// This should be called during context switch to update the stack
/// pointer for the current CPU.
/// 
/// # Panics
/// 
/// Panics in debug builds if the stack is not 16-byte aligned.
#[allow(clippy::cast_possible_truncation)]
pub fn set_kernel_stack(stack_top: VirtAddr) {
    let stack_addr = stack_top.as_u64() as usize;
    
    // Verify 16-byte alignment
    debug_assert!(
        stack_addr.is_multiple_of(16),
        "Kernel stack must be 16-byte aligned, got 0x{stack_addr:x}"
    );
    
    // Update Per-CPU data (primary)
    super::per_cpu::update_kernel_stack(stack_top);
}

/// Get the currently configured kernel stack
pub fn get_kernel_stack() -> VirtAddr {
    super::per_cpu::get_kernel_stack()
}

/// Check stack usage for debugging (Phase 3)
/// 
/// Uses Per-CPU data for stack bounds.
#[cfg(debug_assertions)]
#[allow(dead_code)]
pub fn check_stack_usage() {
    let current_rsp: u64;
    unsafe {
        core::arch::asm!("mov {}, rsp", out(reg) current_rsp, options(nomem, nostack));
    }
    
    let stack_top = super::per_cpu::get_kernel_stack().as_u64();
    let stack_bottom = stack_top - 16384; // 16KB stack in Per-CPU
    
    assert!(
        current_rsp >= stack_bottom,
        "Stack overflow detected! RSP=0x{current_rsp:x}, bottom=0x{stack_bottom:x}"
    );
    
    let used = stack_top - current_rsp;
    if used > 8192 {
        debug_println!("⚠️  High stack usage: {used} bytes / 16384");
    }
}

/// Validate syscall context (debug builds only)
/// 
/// Checks if the syscall handler is running in the correct context:
/// - Stack pointer within valid range
/// - CPU in Ring 0 (kernel mode)
/// - GS base points to valid Per-CPU data (Phase 3)
#[cfg(debug_assertions)]
fn validate_syscall_context() {
    // Check stack range using Per-CPU data
    let current_rsp: u64;
    unsafe {
        core::arch::asm!("mov {}, rsp", out(reg) current_rsp, options(nomem, nostack));
    }
    
    let stack_top = super::per_cpu::get_kernel_stack().as_u64();
    let stack_bottom = stack_top - 16384; // 16KB stack
    
    assert!(
        current_rsp >= stack_bottom && current_rsp <= stack_top,
        "Invalid RSP during syscall: 0x{current_rsp:x} (expected 0x{stack_bottom:x}-0x{stack_top:x})"
    );
    
    // Check privilege level (CS & 3 should be 0 for Ring 0)
    let cs: u16;
    unsafe {
        core::arch::asm!("mov {:x}, cs", out(reg) cs, options(nomem, nostack));
    }
    
    assert_eq!(
        cs & 3,
        0,
        "Syscall handler running in wrong privilege level! CS=0x{cs:x}"
    );
    
    // Verify GS base is set correctly (should point to Per-CPU data)
    // IMPORTANT: After swapgs, IA32_GS_BASE contains the kernel's Per-CPU address,
    // and IA32_KERNEL_GS_BASE contains the user's original GS (typically 0).
    // So we check IA32_GS_BASE, not IA32_KERNEL_GS_BASE!
    let current_gs = super::per_cpu::read_gs_base();
    let expected_gs = super::per_cpu::get_per_cpu_addr();
    debug_assert_eq!(
        current_gs, expected_gs,
        "IA32_GS_BASE mismatch after swapgs: got 0x{current_gs:x}, expected 0x{expected_gs:x}"
    );
}


/// Syscall handler dispatcher (Phase 3: Per-CPU based)
///
/// This function is called from the syscall entry point and dispatches
/// to the appropriate syscall implementation based on the syscall number.
///
/// # Safety
///
/// This function is called from assembly and must maintain the syscall
/// calling convention.
#[unsafe(no_mangle)]
#[allow(clippy::cast_sign_loss)] // Intentional: syscall result conversion
extern "C" fn syscall_handler(
    syscall_num: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    arg4: u64,
    arg5: u64,
    arg6: u64,
) -> u64 {
    // Validate context in debug builds
    #[cfg(debug_assertions)]
    validate_syscall_context();
    
    // Increment syscall counter for this CPU (Phase 3)
    unsafe {
        super::per_cpu::current().inc_syscall_count();
    }
    
    // Trace syscall entry in debug builds
    debug_println!(
        "[SYSCALL-ENTRY] num={}, args=({:#x}, {:#x}, {:#x}, {:#x}, {:#x}, {:#x})",
        syscall_num, arg1, arg2, arg3, arg4, arg5, arg6
    );
    
    // Call the syscall dispatcher
    let result = crate::kernel::syscall::dispatch(
        syscall_num, arg1, arg2, arg3, arg4, arg5, arg6
    );
    
    // Trace syscall return
    debug_println!("[SYSCALL-RESULT] num={syscall_num} returned {result}");
    
    // Convert i64 result to u64 for return
    result as u64
}

/// User pointer validation module
/// 
/// Provides safe wrappers for accessing user-space memory from kernel.
pub mod validation {
    /// Check if an address is in user space
    /// 
    /// User space: `0x0000_0000_0000_0000` ~ `0x0000_7FFF_FFFF_FFFF`
    /// Kernel space: `0xFFFF_8000_0000_0000` ~ `0xFFFF_FFFF_FFFF_FFFF`
    #[must_use]
    pub const fn is_user_address(addr: u64) -> bool {
        addr < 0x0000_8000_0000_0000
    }
    
    /// Check if a memory range is valid and in user space
    /// 
    /// Returns `true` if:
    /// - Length is non-zero
    /// - No overflow in `addr + len`
    /// - Both start and end are in user space
    #[must_use]
    pub const fn is_user_range(addr: u64, len: u64) -> bool {
        if len == 0 {
            return false;
        }
        
        // Check for overflow
        let Some(end) = addr.checked_add(len) else {
            return false;
        };
        
        is_user_address(addr) && is_user_address(end - 1)
    }
    
    /// Safely copy from user space to kernel space
    /// 
    /// Returns `None` if:
    /// - Address is invalid (not in user space)
    /// - Address is not readable (page fault would occur)
    /// - Page is not mapped or lacks read permission
    /// 
    /// # Safety
    /// 
    /// This is still unsafe because:
    /// - TOCTOU issues (page could be unmapped after check)
    /// 
    /// # Phase 2 Implementation
    /// 
    /// This now properly checks if pages are mapped and readable.
    #[must_use]
    #[allow(dead_code)]
    #[allow(clippy::missing_const_for_fn)] // Unsafe fn cannot be const
    pub unsafe fn copy_from_user<T: Copy>(user_ptr: u64) -> Option<T> {
        use crate::kernel::security::validate_user_read;
        
        // Validate that the pointer is in user space and the page is mapped/readable
        if validate_user_read(user_ptr, core::mem::size_of::<T>() as u64).is_err() {
            return None;
        }
        
        // Safe to read from user space (validated above)
        #[allow(clippy::cast_ptr_alignment)] // Caller ensures alignment
        Some(*(user_ptr as *const T))
    }
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
    /// Fork process
    Fork = 6,
    /// Execute program
    Exec = 7,
    /// Wait for child process
    Wait = 8,
    /// Map memory
    Mmap = 9,
    /// Unmap memory
    Munmap = 10,
    /// Create pipe
    Pipe = 11,
    /// Initialize io_uring
    IoUringSetup = 12,
    /// Submit/complete io_uring operations
    IoUringEnter = 13,
    /// Register io_uring resources
    IoUringRegister = 14,
}

