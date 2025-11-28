// crates/kernel/src/arch/x86_64/cr3_test.rs
//! CR3 switching test utilities for Phase 3
//! 
//! These functions help diagnose the CR3 switching issue by testing
//! different aspects of CR3 operations in isolation.

/// Test CR3 switch without iretq
/// Returns 0 on success
pub unsafe fn test_cr3_switch(new_cr3: u64) -> i32 {
    unsafe extern "C" {
        fn test_cr3_switch(new_cr3: u64) -> i32;
    }
    
    unsafe { test_cr3_switch(new_cr3) }
}

/// Test iretq without CR3 switch
/// Returns 0 on success
pub unsafe fn test_iretq_only() -> i32 {
    unsafe extern "C" {
        fn test_iretq_only() -> i32;
    }
    
    unsafe { test_iretq_only() }
}

/// Test CR3 switch with simple code execution
/// Returns 0 on success
pub unsafe fn test_cr3_with_execution(new_cr3: u64) -> i32 {
    unsafe extern "C" {
        fn test_cr3_with_execution(new_cr3: u64) -> i32;
    }
    
    unsafe { test_cr3_with_execution(new_cr3) }
}

/// Run all CR3 tests and report results
pub unsafe fn run_cr3_diagnostic_tests(user_cr3: u64) {
    use crate::debug_println;
    
    debug_println!("[CR3 Diagnostic] Starting CR3 switching tests...");
    debug_println!("[CR3 Diagnostic] User CR3: {:#x}", user_cr3);
    
    // Test 1: iretq without CR3 switch
    debug_println!("[CR3 Test 1] Testing iretq without CR3 switch...");
    let result = unsafe { test_iretq_only() };
    if result == 0 {
        debug_println!("[CR3 Test 1] ✅ PASSED: iretq works");
    } else {
        debug_println!("[CR3 Test 1] ❌ FAILED: iretq returned {}", result);
    }
    
    // Test 2: CR3 switch without iretq
    debug_println!("[CR3 Test 2] Testing CR3 switch without iretq...");
    let result = unsafe { test_cr3_switch(user_cr3) };
    if result == 0 {
        debug_println!("[CR3 Test 2] ✅ PASSED: CR3 switch works");
    } else {
        debug_println!("[CR3 Test 2] ❌ FAILED: CR3 switch returned {}", result);
    }
    
    // Test 3: CR3 switch with code execution
    debug_println!("[CR3 Test 3] Testing CR3 switch with code execution...");
    let result = unsafe { test_cr3_with_execution(user_cr3) };
    if result == 0 {
        debug_println!("[CR3 Test 3] ✅ PASSED: CR3 + execution works");
    } else {
        debug_println!("[CR3 Test 3] ❌ FAILED: CR3 + execution returned {}", result);
    }
    
    debug_println!("[CR3 Diagnostic] Tests completed");
}
