//! FPU/SSE state management
//!
//! This module provides functions to save and restore FPU/SSE/AVX state
//! using the XSAVE/XRSTOR instructions for proper context switching.

/// Save FPU/SSE/AVX state to a buffer
///
/// Uses the XSAVE instruction to save all extended CPU state including:
/// - x87 FPU state
/// - SSE state (XMM registers)
/// - AVX state (YMM registers, if available)
///
/// # Arguments
///
/// * `buffer` - Pointer to a 64-byte aligned, 512-byte buffer
///
/// # Safety
///
/// - The buffer must be at least 512 bytes and 64-byte aligned
/// - The buffer must be valid for writes
/// - XSAVE must be supported by the CPU (check CPUID)
#[inline(always)]
pub unsafe fn save_fpu_state(buffer: *mut u8) {
    core::arch::asm!(
        "xsave64 [{}]",
        in(reg) buffer,
        in("rax") 0xFFFFFFFF_FFFFFFFFu64,  // すべての状態を保存 (RFBM mask)
        in("rdx") 0,                        // 上位32ビット
        options(nostack),
    );
}

/// Restore FPU/SSE/AVX state from a buffer
///
/// Uses the XRSTOR instruction to restore all extended CPU state.
///
/// # Arguments
///
/// * `buffer` - Pointer to a 64-byte aligned, 512-byte buffer containing saved state
///
/// # Safety
///
/// - The buffer must be at least 512 bytes and 64-byte aligned
/// - The buffer must contain valid XSAVE state data
/// - XRSTOR must be supported by the CPU (check CPUID)
#[inline(always)]
pub unsafe fn restore_fpu_state(buffer: *const u8) {
    core::arch::asm!(
        "xrstor64 [{}]",
        in(reg) buffer,
        in("rax") 0xFFFFFFFF_FFFFFFFFu64,  // すべての状態を復元 (RFBM mask)
        in("rdx") 0,                        // 上位32ビット
        options(nostack),
    );
}

/// Initialize FPU for the current CPU
///
/// Enables SSE and sets up FPU control flags.
///
/// # Safety
///
/// Should only be called once during CPU initialization.
#[allow(dead_code)]
pub unsafe fn init() {
    // Enable SSE (CR4.OSFXSR = 1, CR4.OSXMMEXCPT = 1)
    // Enable x87 FPU (CR0.EM = 0, CR0.MP = 1)
    core::arch::asm!(
        // Clear EM (bit 2) and set MP (bit 1) in CR0
        "mov rax, cr0",
        "and rax, ~4",      // Clear EM
        "or rax, 2",        // Set MP
        "mov cr0, rax",
        
        // Set OSFXSR (bit 9) and OSXMMEXCPT (bit 10) in CR4
        "mov rax, cr4",
        "or rax, 0x600",    // Set bits 9 and 10
        "mov cr4, rax",
        
        out("rax") _,
        options(nostack, preserves_flags),
    );
    
    crate::debug_println!("[FPU] FPU/SSE initialized");
}
