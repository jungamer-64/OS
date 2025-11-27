// kernel/src/arch/x86_64/syscall.rs
//! System Call Mechanism for `x86_64`
//!
//! This module implements the syscall/sysret mechanism for transitioning
//! between Ring 3 (user mode) and Ring 0 (kernel mode).

#![allow(unsafe_op_in_unsafe_fn)] // naked_asm! requires this

use x86_64::VirtAddr;
use x86_64::registers::model_specific::{Efer, EferFlags, LStar, SFMask};
use x86_64::registers::rflags::RFlags;
use crate::arch::x86_64::gdt;
use crate::debug_println;
use crate::kernel::process::PROCESS_TABLE;

/// Initialize the syscall mechanism
///
/// This sets up the Model Specific Registers (MSRs) required for
/// the `syscall` and `sysret` instructions to work properly.
/// 
/// # Panics
/// 
/// Panics if `Star::write` fails (should never happen with valid GDT selectors).
#[allow(clippy::missing_panics_doc)] // Star::write panic is documented
pub fn init() {
    unsafe {
        // Enable syscall/sysret in EFER
        Efer::update(|flags| {
            *flags |= EferFlags::SYSTEM_CALL_EXTENSIONS;
        });
        
        // Set up STAR register (kernel and user segment selectors)
        let selectors = gdt::selectors();
        
        // IMPORTANT: Star::write requires base selectors WITHOUT RPL bits
        // User segments have RPL=3 (Ring 3) in lower 2 bits:
        //   user_code.0 = 0x1B (base 0x18 + RPL 0x03)
        //   user_data.0 = 0x23 (base 0x20 + RPL 0x03)
        // SYSRET will automatically add RPL=3 when returning to user mode
        
        use x86_64::structures::gdt::SegmentSelector;
        let user_code_base = SegmentSelector(selectors.user_code.0 & !0x03);
        let user_data_base = SegmentSelector(selectors.user_data.0 & !0x03);
        
        // STAR MSR format (Intel SDM Vol. 2B, SYSCALL/SYSRET):
        // Bits [63:48]: SYSRET CS selector (user_code_base)
        // Bits [47:32]: SYSCALL CS selector (kernel_code)
        // Bits [31:0]:  Reserved
        //
        // SYSRET behavior:
        //   CS = STAR[63:48] + 16     (user code with RPL=3)
        //   SS = STAR[63:48] + 8      (user data with RPL=3)
        //
        // Therefore: STAR[63:48] must be set to (user_code_base - 16)
        // In our case: user_code_base = 0x18, so STAR[63:48] = 0x08
        
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
        
        // Set up LSTAR register (syscall entry point)
        LStar::write(VirtAddr::new(syscall_entry as *const () as u64));
        
        // Set up SFMASK register (RFLAGS bits to clear on syscall)
        // We clear the interrupt flag to disable interrupts during syscall handling
        SFMask::write(RFlags::INTERRUPT_FLAG);
        
        // Initialize kernel stack for syscalls
        init_kernel_stack();
        
        let kernel_cs = u64::from(selectors.kernel_code.0);
        let user_cs = u64::from(selectors.user_code.0);
        debug_println!("[OK] Syscall mechanism initialized");
        debug_println!("  STAR: kernel_cs=0x{:x}, user_cs=0x{:x}", kernel_cs, user_cs);
        debug_println!("  LSTAR: 0x{:x}", syscall_entry as *const () as u64);
    }
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

/// Syscall entry point
///
/// This is called when userspace executes the `syscall` instruction.
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
pub unsafe extern "C" fn syscall_entry() {
    core::arch::naked_asm!(
        // At this point:
        // - CS/SS have been switched to kernel segments by CPU
        // - RCX = user RIP, R11 = user RFLAGS (saved by CPU)
        // - RSP still points to USER stack (dangerous!)
        
        // Save user RSP in a scratch register
        "mov r15, rsp",
        
        // Phase 2: Load kernel stack
        // CURRENT_KERNEL_STACK is updated during:
        // - init_kernel_stack() (boot time) -> fallback stack
        // - set_kernel_stack() (context switch) -> process kernel stack
        "mov rsp, qword ptr [rip + {current_stack}]",
        
        // === CRITICAL: Align stack BEFORE pushing ===
        // The kernel stack should already be 16-byte aligned from initialization,
        // but we verify and align if necessary.
        // This MUST happen before any push operations.
        "test rsp, 15",           // Check if RSP & 0xF == 0
        "jz 2f",                  // If aligned, skip
        "and rsp, -16",           // Otherwise, align to 16-byte boundary
        "2:",
        
        // === Save user context (8 registers = 64 bytes) ===
        // After this, RSP is still 16-byte aligned (aligned - 64 = aligned)
        "push r15",          // User RSP (saved earlier)
        "push rcx",          // User RIP (saved by CPU on syscall)
        "push r11",          // User RFLAGS (saved by CPU on syscall)
        
        // Save callee-saved registers
        "push rbp",
        "push rbx",
        "push r12",
        "push r13",
        "push r14",
        
        // === C ABI alignment requirement ===
        // System V AMD64 ABI requires RSP to be at (16*N + 8) before 'call'
        // because 'call' pushes an 8-byte return address.
        // 
        // Current state: RSP is 16-byte aligned (after 64 bytes of pushes)
        // Required: RSP = 16*N + 8
        // Solution: Push 8 more bytes as padding
        "sub rsp, 8",        // Padding for alignment
        
        // === Prepare arguments for C calling convention ===
        // Syscall ABI:
        //   RAX = syscall number
        //   RDI = arg1, RSI = arg2, RDX = arg3, R10 = arg4, R8 = arg5, R9 = arg6
        // 
        // C calling convention (System V AMD64):
        //   RDI = arg1, RSI = arg2, RDX = arg3, RCX = arg4, R8 = arg5, R9 = arg6
        //
        // syscall_handler(syscall_num, arg1, arg2, arg3, arg4, arg5, arg6)
        // So we need:
        //   RDI = syscall_num (from RAX)
        //   RSI = arg1 (from RDI)
        //   RDX = arg2 (from RSI)
        //   RCX = arg3 (from RDX)
        //   R8  = arg4 (from R10)
        //   R9  = arg5 (from R8)
        //   [stack] = arg6 (from R9)
        
        // Save original values before shuffling
        "push r9",           // Save arg6 to stack (will be 7th C arg)
        "mov r9, r8",        // arg5 (C arg6 = R9)
        "mov r8, r10",       // arg4 (C arg5 = R8)
        "mov rcx, rdx",      // arg3 (C arg4 = RCX)
        "mov rdx, rsi",      // arg2 (C arg3 = RDX)
        "mov rsi, rdi",      // arg1 (C arg2 = RSI)
        "mov rdi, rax",      // syscall_num (C arg1 = RDI)
        
        // === Call the syscall handler ===
        // Stack is now properly aligned: (16*N + 8) after sub + push
        // After 'call' pushes return address: (16*N)
        "call {syscall_handler}",
        
        // === Result is in RAX, preserve it ===
        
        // === Remove arg6 from stack and padding ===
        "add rsp, 16",       // Remove arg6 push (8) + alignment padding (8)
        
        // === Restore callee-saved registers (in reverse order) ===
        "pop r14",
        "pop r13",
        "pop r12",
        "pop rbx",
        "pop rbp",
        
        // === Restore user context ===
        "pop r11",          // User RFLAGS
        "pop rcx",          // User RIP
        "pop r15",          // User RSP
        
        // === Switch back to user stack ===
        "mov rsp, r15",
        
        // === Return to user mode ===
        // sysretq will:
        // - Load RCX into RIP (user return address)
        // - Load R11 into RFLAGS
        // - Switch CS/SS back to user segments
        "sysretq",
        
        current_stack = sym CURRENT_KERNEL_STACK,
        syscall_handler = sym syscall_handler,
    );
}

// Kernel stack management
// 
// Phase 2 Implementation:
// - Per-process kernel stacks (primary)
// - Fallback to global stack if no process is active
// 
// Each process has its own kernel stack stored in Process.kernel_stack
// This provides:
// 1. ✅ Safe for concurrent syscalls (each process isolated)
// 2. ✅ Safe with interrupts (stack is per-process)
// 3. ✅ Isolated per-process

// Fallback kernel stack for early boot or when no process is active
#[repr(C, align(16))]
struct KernelStack {
    data: [u8; 8192], // 8KB stack
}

static mut KERNEL_STACK: KernelStack = KernelStack {
    data: [0; 8192],
};

// Pointer to top of fallback kernel stack (stacks grow downward)
#[allow(clippy::ptr_as_ptr)] // Intentional: pointer arithmetic for stack calculation
fn get_fallback_kernel_stack_top() -> usize {
    unsafe {
        let stack_ptr = core::ptr::addr_of!(KERNEL_STACK);
        let data_ptr = core::ptr::addr_of!((*stack_ptr).data);
        (data_ptr as *const u8).add(8192) as usize
    }
}

/// Get the current process's kernel stack pointer
/// 
/// Returns the kernel stack for the currently running process.
/// If no process is active, returns the fallback global stack.
/// 
/// # Phase 2 Implementation
/// This function can be used in the future for more dynamic stack selection.
#[allow(dead_code)]
#[allow(clippy::cast_possible_truncation)] // x86_64 target only
#[allow(clippy::collapsible_if)] // More readable with explicit nesting
fn get_current_kernel_stack() -> usize {
    // Try to get the current process's kernel stack
    if let Some(table) = PROCESS_TABLE.try_lock() {
        if let Some(process) = table.current_process() {
            // Use the process's kernel stack
            return process.kernel_stack().as_u64() as usize;
        }
    }
    
    // Fallback to global stack if no process is active or table is locked
    get_fallback_kernel_stack_top()
}

// Static variable storing the kernel stack pointer
// Note: This is updated before each syscall entry
use lazy_static::lazy_static;
use spin::Mutex;
use core::sync::atomic::{AtomicUsize, Ordering};

// Current kernel stack (atomic for lock-free access from assembly)
static CURRENT_KERNEL_STACK: AtomicUsize = AtomicUsize::new(0);

lazy_static! {
    static ref KERNEL_SYSCALL_STACK: Mutex<usize> = Mutex::new(get_fallback_kernel_stack_top());
}

/// Initialize the current kernel stack pointer
/// 
/// This should be called during kernel initialization to set up the
/// fallback stack before any processes are created.
pub fn init_kernel_stack() {
    let stack_top = get_fallback_kernel_stack_top();
    
    // Verify 16-byte alignment (critical for C ABI and syscall_entry)
    debug_assert!(
        stack_top.is_multiple_of(16),
        "Kernel stack must be 16-byte aligned, got 0x{stack_top:x}"
    );
    
    CURRENT_KERNEL_STACK.store(stack_top, Ordering::Release);
    
    debug_println!("[OK] Kernel syscall stack initialized at 0x{:x}", stack_top);
}

/// Update the current kernel stack pointer
/// 
/// This should be called during context switch to update the stack
/// pointer for the next syscall.
/// 
/// # Panics
/// 
/// Panics in debug builds if the stack is not 16-byte aligned.
#[allow(dead_code)]
#[allow(clippy::cast_possible_truncation)] // x86_64 target only
pub fn set_kernel_stack(stack_top: VirtAddr) {
    let stack_addr = stack_top.as_u64() as usize;
    
    // Verify 16-byte alignment
    debug_assert!(
        stack_addr.is_multiple_of(16),
        "Kernel stack must be 16-byte aligned, got 0x{stack_addr:x}"
    );
    
    CURRENT_KERNEL_STACK.store(stack_addr, Ordering::Release);
}

/// Get the currently configured kernel stack
#[allow(dead_code)]
pub fn get_kernel_stack() -> VirtAddr {
    VirtAddr::new(CURRENT_KERNEL_STACK.load(Ordering::Acquire) as u64)
}

/// Check stack usage for debugging
/// 
/// This function should be called periodically during development
/// to detect stack overflows or excessive stack usage.
/// 
/// # Panics
/// 
/// Panics if stack overflow is detected (RSP below stack bottom).
#[cfg(debug_assertions)]
#[allow(dead_code)]
pub fn check_stack_usage() {
    let current_rsp: u64;
    unsafe {
        core::arch::asm!("mov {}, rsp", out(reg) current_rsp, options(nomem, nostack));
    }
    
    let stack_top = CURRENT_KERNEL_STACK.load(Ordering::Acquire) as u64;
    let stack_bottom = stack_top.saturating_sub(8192);
    
    assert!(
        current_rsp >= stack_bottom,
        "Stack overflow detected! RSP=0x{current_rsp:x}, bottom=0x{stack_bottom:x}"
    );
    
    let used = stack_top.saturating_sub(current_rsp);
    if used > 4096 {
        debug_println!("⚠️  High stack usage: {used} bytes / 8192");
    }
}

/// Validate syscall context (debug builds only)
/// 
/// Checks if the syscall handler is running in the correct context:
/// - Stack pointer within valid range
/// - CPU in Ring 0 (kernel mode)
#[cfg(debug_assertions)]
fn validate_syscall_context() {
    // Check stack range
    let current_rsp: u64;
    unsafe {
        core::arch::asm!("mov {}, rsp", out(reg) current_rsp, options(nomem, nostack));
    }
    
    let stack_top = CURRENT_KERNEL_STACK.load(Ordering::Acquire) as u64;
    let stack_bottom = stack_top.saturating_sub(8192);
    
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
    
    // Trace syscall entry in debug builds
    #[cfg(all(debug_assertions, feature = "syscall_trace"))]
    debug_println!(
        "[SYSCALL] num={}, args=({:#x}, {:#x}, {:#x}, {:#x}, {:#x}, {:#x})",
        syscall_num, arg1, arg2, arg3, arg4, arg5, arg6
    );
    
    // Call the syscall dispatcher
    let result = crate::kernel::syscall::dispatch(
        syscall_num, arg1, arg2, arg3, arg4, arg5, arg6
    );
    
    // Trace syscall return in debug builds
    #[cfg(all(debug_assertions, feature = "syscall_trace"))]
    debug_println!("[SYSCALL] num={syscall_num} returned {result:#x}");
    
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
}

