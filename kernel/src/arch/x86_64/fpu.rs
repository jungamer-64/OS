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
    unsafe {
        core::arch::asm!(
            "xsave64 [{}]",
            in(reg) buffer,
            in("rax") 0xFFFFFFFF_FFFFFFFFu64,  // すべての状態を保存 (RFBM mask)
            in("rdx") 0,                        // 上位32ビット
            options(nostack),
        );
    }
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
    unsafe {
        core::arch::asm!(
            "xrstor64 [{}]",
            in(reg) buffer,
            in("rax") 0xFFFFFFFF_FFFFFFFFu64,  // すべての状態を復元 (RFBM mask)
            in("rdx") 0,                        // 上位32ビット
            options(nostack),
        );
    }
}

/// Initialize FPU for the current CPU
///
/// Detects and enables CPU features in the following order:
/// 1. x87 FPU (required)
/// 2. SSE/SSE2 (if available)
/// 3. AVX (if available)
/// 4. XSAVE/XRSTOR (if available)
///
/// # Safety
///
/// Should only be called once during CPU initialization.
#[allow(dead_code)]
pub unsafe fn init() {
    use raw_cpuid::CpuId;
    
    let cpuid = CpuId::new();
    
    // Check for basic FPU support (should always be present on x86_64)
    let has_fpu = cpuid.get_feature_info()
        .map_or(false, |f| f.has_fpu());
    
    if !has_fpu {
        crate::debug_println!("[FPU] ERROR: No FPU detected!");
        return;
    }
    
    // Enable x87 FPU (CR0.EM = 0, CR0.MP = 1)
    unsafe {
        core::arch::asm!(
            "mov rax, cr0",
            "btr rax, 2",       // Clear EM (bit 2)
            "bts rax, 1",       // Set MP (bit 1)
            "mov cr0, rax",
            out("rax") _,
            options(nostack),
        );
    }
    crate::debug_println!("[FPU] x87 FPU enabled");
    
    // Check and enable SSE
    let has_sse = cpuid.get_feature_info()
        .map_or(false, |f| f.has_sse());
    let has_sse2 = cpuid.get_feature_info()
        .map_or(false, |f| f.has_sse2());
    
    if has_sse && has_sse2 {
        unsafe {
            core::arch::asm!(
                "mov rax, cr4",
                "or rax, 0x600",    // Set OSFXSR (bit 9) and OSXMMEXCPT (bit 10)
                "mov cr4, rax",
                out("rax") _,
                options(nostack),
            );
        }
        crate::debug_println!("[FPU] SSE/SSE2 enabled");
    } else {
        crate::debug_println!("[FPU] SSE not available");
    }
    
    // Check and enable AVX
    let has_avx = cpuid.get_feature_info()
        .map_or(false, |f| f.has_avx());
    let has_xsave = cpuid.get_feature_info()
        .map_or(false, |f| f.has_xsave());
    
    if has_avx && has_xsave {
        unsafe {
            core::arch::asm!(
                // Enable XSAVE (CR4.OSXSAVE = 1, bit 18)
                "mov rax, cr4",
                "bts rax, 18",
                "mov cr4, rax",
                
                // Enable AVX in XCR0 (bits 0, 1, 2)
                // Bit 0: x87
                // Bit 1: SSE
                // Bit 2: AVX
                "xor rcx, rcx",     // XCR0
                "xgetbv",
                "or eax, 0x7",      // Enable x87, SSE, AVX
                "xsetbv",
                
                out("rax") _,
                out("rcx") _,
                out("rdx") _,
                options(nostack),
            );
        }
        crate::debug_println!("[FPU] AVX/XSAVE enabled");
    } else {
        if !has_xsave {
            crate::debug_println!("[FPU] XSAVE not available");
        }
        if !has_avx {
            crate::debug_println!("[FPU] AVX not available");
        }
    }
    
    // Log feature summary
    crate::debug_println!("[FPU] Initialization complete - FPU: {}, SSE: {}, AVX: {}, XSAVE: {}",
        has_fpu, has_sse && has_sse2, has_avx, has_xsave);
}
