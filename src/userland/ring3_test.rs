// src/userland/ring3_test.rs
//! Ring 3 (User Mode) Test Program
//!
//! This test program runs in user space (Ring 3) and validates that:
//! 1. User code can execute properly
//! 2. System calls work correctly
//! 3. Ring separation is enforced

#![allow(dead_code)]

use crate::userland::{syscall0, syscall2, syscall3, Syscall};

/// Test entry point for Ring 3 execution
///
/// This function is designed to be called after transitioning to Ring 3.
/// It tests basic system call functionality.
pub fn ring3_test_main() -> ! {
    // Test 1: Get PID
    let pid = unsafe { syscall0(Syscall::Getpid as u64) };
    
    // Test 2: Write "Hello from Ring 3!"
    let message = b"[Ring 3 Test] Hello from user space!\n";
    let result = unsafe {
        syscall3(
            Syscall::Write,
            1, // stdout
            message.as_ptr() as u64,
            message.len() as u64,
        )
    };
    
    // Test 3: Verify write succeeded
    if result < 0 {
        // Error occurred
        let error_msg = b"[Ring 3 Test] ERROR: Write syscall failed\n";
        unsafe {
            syscall3(
                Syscall::Write,
                1,
                error_msg.as_ptr() as u64,
                error_msg.len() as u64,
            );
        }
    }
    
    // Test 4: Print PID
    let mut buffer = [0u8; 64];
    let pid_str = format_pid(pid, &mut buffer);
    unsafe {
        syscall3(
            Syscall::Write,
            1,
            pid_str.as_ptr() as u64,
            pid_str.len() as u64,
        );
    }
    
    // Test 5: Exit gracefully
    let success_msg = b"[Ring 3 Test] All tests passed! Exiting...\n";
    unsafe {
        syscall3(
            Syscall::Write,
            1,
            success_msg.as_ptr() as u64,
            success_msg.len() as u64,
        );
        
        syscall0(Syscall::Exit);
    }
    
    // Should never reach here
    loop {
        core::hint::spin_loop();
    }
}

/// Format a PID value into a string buffer
///
/// Returns a string slice containing the formatted PID
///
/// # Safety
/// This function is safe against buffer overflow (max 64 bytes)
fn format_pid(pid: i64, buffer: &mut [u8; 64]) -> &[u8] {
    let prefix = b"[Ring 3 Test] Current PID: ";
    let mut pos = 0;
    
    // Copy prefix (safe: prefix.len() = 27 < 64)
    for &byte in prefix {
        if pos >= buffer.len() - 20 {
            break; // Leave 20 bytes for number + newline
        }
        buffer[pos] = byte;
        pos += 1;
    }
    
    // Format PID as decimal (handles negative values)
    if pid < 0 {
        // Negative PID (error code)
        if pos < buffer.len() {
            buffer[pos] = b'-';
            pos += 1;
        }
        let mut temp = [0u8; 20];
        let mut temp_pos = 0;
        let mut n = -pid; // Convert to positive
        
        while n > 0 && temp_pos < temp.len() {
            temp[temp_pos] = b'0' + (n % 10) as u8;
            temp_pos += 1;
            n /= 10;
        }
        
        // Reverse into buffer
        for i in (0..temp_pos).rev() {
            if pos >= buffer.len() - 2 {
                break;
            }
            buffer[pos] = temp[i];
            pos += 1;
        }
    } else if pid == 0 {
        if pos < buffer.len() - 2 {
            buffer[pos] = b'0';
            pos += 1;
        }
    } else {
        let mut temp = [0u8; 20];
        let mut temp_pos = 0;
        let mut n = pid;
        
        while n > 0 && temp_pos < temp.len() {
            temp[temp_pos] = b'0' + (n % 10) as u8;
            temp_pos += 1;
            n /= 10;
        }
        
        // Reverse into buffer
        for i in (0..temp_pos).rev() {
            if pos >= buffer.len() - 2 {
                break;
            }
            buffer[pos] = temp[i];
            pos += 1;
        }
    }
    
    // Add newline
    buffer[pos] = b'\n';
    pos += 1;
    
    &buffer[..pos]
}

/// Alternative test: Infinite loop that prints periodically
///
/// This can be used to test preemptive multitasking (once implemented)
#[allow(dead_code)]
pub fn ring3_loop_test() -> ! {
    let mut counter: u64 = 0;
    
    loop {
        if counter % 1000000 == 0 {
            let message = b"[Ring 3 Loop] Still alive...\n";
            unsafe {
                syscall3(
                    Syscall::Write as u64,
                    1,
                    message.as_ptr() as u64,
                    message.len() as u64,
                );
            }
        }
        
        counter = counter.wrapping_add(1);
        core::hint::spin_loop();
    }
}

/// Test that attempts privileged operations (should fail)
///
/// This test deliberately tries to execute privileged instructions
/// to verify that Ring 3 protection is working correctly.
#[allow(dead_code)]
pub fn ring3_privilege_test() -> ! {
    let message = b"[Ring 3 Privilege Test] Attempting privileged operation...\n";
    unsafe {
        syscall3(
            Syscall::Write as u64,
            1,
            message.as_ptr() as u64,
            message.len() as u64,
        );
    }
    
    // Try to execute a privileged instruction (should cause #GP fault)
    // Uncomment to test:
    // unsafe {
    //     core::arch::asm!("cli"); // Disable interrupts (privileged)
    // }
    
    let success_msg = b"[Ring 3 Privilege Test] If you see this, Ring 3 protection is NOT working!\n";
    unsafe {
        syscall3(
            Syscall::Write as u64,
            1,
            success_msg.as_ptr() as u64,
            success_msg.len() as u64,
        );
        
        syscall0(Syscall::Exit as u64);
    }
    
    loop {
        core::hint::spin_loop();
    }
}
