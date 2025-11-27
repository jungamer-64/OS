// src/tests/syscall_alignment_test.rs
//! System Call Alignment and Safety Tests
//!
//! These tests verify the correctness of the syscall implementation,
//! particularly around stack alignment and register preservation.

#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(tiny_os::test_runner)]
#![reexport_test_harness_main = "test_main"]

use tiny_os::{serial_print, serial_println};
use core::panic::PanicInfo;

#[no_mangle]
pub extern "C" fn _start() -> ! {
    test_main();
    loop {}
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    tiny_os::test_panic_handler(info)
}

/// Test that the kernel stack is properly aligned
#[test_case]
fn test_kernel_stack_alignment() {
    use tiny_os::arch::x86_64::syscall;
    
    serial_print!("test_kernel_stack_alignment... ");
    
    let stack = syscall::get_kernel_stack();
    assert_eq!(
        stack.as_u64() % 16,
        0,
        "Kernel stack not 16-byte aligned: 0x{:x}",
        stack.as_u64()
    );
    
    serial_println!("[ok]");
}

/// Test syscall number enum values
#[test_case]
fn test_syscall_numbers() {
    use tiny_os::arch::x86_64::syscall::SyscallNumber;
    
    serial_print!("test_syscall_numbers... ");
    
    assert_eq!(SyscallNumber::Write as u64, 0);
    assert_eq!(SyscallNumber::Read as u64, 1);
    assert_eq!(SyscallNumber::Exit as u64, 2);
    assert_eq!(SyscallNumber::GetPid as u64, 3);
    assert_eq!(SyscallNumber::Alloc as u64, 4);
    assert_eq!(SyscallNumber::Dealloc as u64, 5);
    
    serial_println!("[ok]");
}

/// Test user address validation
#[test_case]
fn test_user_address_validation() {
    use tiny_os::arch::x86_64::syscall::validation;
    
    serial_print!("test_user_address_validation... ");
    
    // Valid user addresses
    assert!(validation::is_user_address(0x0000_0000_0000_0000));
    assert!(validation::is_user_address(0x0000_7FFF_FFFF_FFFF));
    assert!(validation::is_user_address(0x0000_1000_0000_0000));
    
    // Invalid kernel addresses
    assert!(!validation::is_user_address(0x0000_8000_0000_0000));
    assert!(!validation::is_user_address(0xFFFF_8000_0000_0000));
    assert!(!validation::is_user_address(0xFFFF_FFFF_FFFF_FFFF));
    
    serial_println!("[ok]");
}

/// Test user range validation
#[test_case]
fn test_user_range_validation() {
    use tiny_os::arch::x86_64::syscall::validation;
    
    serial_print!("test_user_range_validation... ");
    
    // Valid ranges
    assert!(validation::is_user_range(0x1000, 0x1000)); // 4KB at 0x1000
    assert!(validation::is_user_range(0x0000_7FFF_0000_0000, 0x0FFF_FFFF));
    
    // Invalid: zero length
    assert!(!validation::is_user_range(0x1000, 0));
    
    // Invalid: overflow
    assert!(!validation::is_user_range(0xFFFF_FFFF_FFFF_FFFF, 1));
    
    // Invalid: crosses into kernel space
    assert!(!validation::is_user_range(0x0000_7FFF_FFFF_0000, 0x10000));
    
    // Invalid: starts in kernel space
    assert!(!validation::is_user_range(0xFFFF_8000_0000_0000, 0x1000));
    
    serial_println!("[ok]");
}

/// Test stack usage check (if available in debug builds)
#[test_case]
#[cfg(debug_assertions)]
fn test_stack_usage_check() {
    use tiny_os::arch::x86_64::syscall;
    
    serial_print!("test_stack_usage_check... ");
    
    // This should not panic in normal conditions
    syscall::check_stack_usage();
    
    serial_println!("[ok]");
}
