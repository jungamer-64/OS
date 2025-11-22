// src/userland/test_syscall.rs
//! Test program for system call mechanism
//!
//! This is a simple user-space program (Ring 3) that tests the basic
//! system call functionality.

#![no_std]
// Note: #![no_main] is intentionally omitted - this is a library module
// that provides user_main() to be called from the kernel

use crate::userland::{self, syscall0, syscall1, syscall2, Syscall};

/// User program entry point
/// 
/// This function runs in Ring 3 (user mode).
#[no_mangle]
pub extern "C" fn user_main() -> ! {
    // Test 1: sys_getpid
    test_getpid();
    
    // Test 2: sys_write (valid)
    test_write_valid();
    
    // Test 3: sys_write (invalid pointer)
    test_write_invalid();
    
    // Test 4: sys_write (kernel address)
    test_write_kernel_addr();
    
    // Test 5: Exit with success
    test_exit();
}

fn test_getpid() {
    let result = unsafe { syscall0(Syscall::GetPid as u64) };
    
    if result > 0 {
        write_str("✓ Test 1 PASSED: getpid() = ");
        write_number(result as u64);
        write_str("\n");
    } else {
        write_str("✗ Test 1 FAILED: getpid() returned error\n");
    }
}

fn test_write_valid() {
    let msg = "Test 2 PASSED: sys_write with valid buffer\n";
    let result = unsafe {
        syscall2(
            Syscall::Write as u64,
            msg.as_ptr() as u64,
            msg.len() as u64,
        )
    };
    
    if result == msg.len() as i64 {
        // Message already printed by syscall
    } else {
        write_str("Test 2 FAILED: sys_write returned wrong length\n");
    }
}

fn test_write_invalid() {
    // Try to write from NULL pointer (should fail with EFAULT)
    let result = unsafe {
        syscall2(
            Syscall::Write as u64,
            0, // NULL pointer
            10,
        )
    };
    
    if result == userland::EFAULT {
        write_str("Test 3 PASSED: sys_write rejected NULL pointer\n");
    } else {
        write_str("Test 3 FAILED: sys_write should reject NULL pointer\n");
    }
}

fn test_write_kernel_addr() {
    // Try to write from kernel address (should fail with EFAULT)
    let kernel_addr = 0xFFFF_8000_0000_0000u64;
    let result = unsafe {
        syscall2(
            Syscall::Write as u64,
            kernel_addr,
            10,
        )
    };
    
    if result == userland::EFAULT {
        write_str("Test 4 PASSED: sys_write rejected kernel address\n");
    } else {
        write_str("Test 4 FAILED: sys_write should reject kernel address\n");
    }
}

fn test_exit() {
    write_str("All tests completed. Exiting with code 0...\n");
    unsafe {
        syscall1(Syscall::Exit as u64, 0);
    }
    loop {}
}

// Helper functions (without allocation)

fn write_str(s: &str) {
    let _ = unsafe {
        syscall2(
            Syscall::Write as u64,
            s.as_ptr() as u64,
            s.len() as u64,
        )
    };
}

fn write_number(mut n: u64) {
    if n == 0 {
        write_str("0");
        return;
    }
    
    let mut buf = [0u8; 20];
    let mut i = 0;
    
    while n > 0 {
        buf[i] = (n % 10) as u8 + b'0';
        n /= 10;
        i += 1;
    }
    
    // Reverse and print
    while i > 0 {
        i -= 1;
        let c = [buf[i]];
        let _ = unsafe {
            syscall2(
                Syscall::Write as u64,
                c.as_ptr() as u64,
                1,
            )
        };
    }
}
