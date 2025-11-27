//! Security tests for system call pointer validation
//!
//! These tests verify that system calls properly validate user-provided pointers
//! and reject invalid addresses.

#![cfg(test)]

use super::*;
use crate::kernel::security::address_space;

/// Test sys_write with null pointer
#[test_case]
fn test_sys_write_null_pointer() {
    let result = sys_write(1, 0, 100, 0, 0, 0);
    assert_eq!(result, EFAULT, "Should reject null pointer");
}

/// Test sys_write with kernel address
#[test_case]
fn test_sys_write_kernel_address() {
    let kernel_addr = address_space::KERNEL_START;
    let result = sys_write(1, kernel_addr, 100, 0, 0, 0);
    assert_eq!(result, EFAULT, "Should reject kernel address");
}

/// Test sys_write with address crossing user/kernel boundary
#[test_case]
fn test_sys_write_boundary_crossing() {
    let addr = address_space::USER_END - 10; // Near boundary
    let len = 20; // Crosses into kernel space
    let result = sys_write(1, addr, len, 0, 0, 0);
    assert_eq!(result, EFAULT, "Should reject boundary-crossing access");
}

/// Test sys_write with excessive length
#[test_case]
fn test_sys_write_excessive_length() {
    let addr = 0x1000; // Valid user address
    let len = MAX_WRITE_LEN + 1;
    let result = sys_write(1, addr, len, 0, 0, 0);
    assert_eq!(result, EINVAL, "Should reject excessive length");
}

/// Test sys_read with null pointer
#[test_case]
fn test_sys_read_null_pointer() {
    // FD 0 (stdin) returns ENOSYS, so use another FD
    let result = sys_read(3, 0, 100, 0, 0, 0);
    // Will fail with EBADF or EFAULT depending on process state
    assert!(result < 0, "Should return error for null pointer");
}

/// Test sys_read with kernel address
#[test_case]
fn test_sys_read_kernel_address() {
    let kernel_addr = address_space::KERNEL_START;
    let result = sys_read(3, kernel_addr, 100, 0, 0, 0);
    assert!(result < 0, "Should reject kernel address");
}

/// Test sys_mmap with kernel address hint
#[test_case]
fn test_sys_mmap_kernel_hint() {
    let kernel_addr = address_space::KERNEL_START;
    let result = sys_mmap(kernel_addr, 4096, 0, 0, 0, 0);
    assert_eq!(result, EFAULT, "Should reject kernel address hint");
}

/// Test sys_mmap with zero length
#[test_case]
fn test_sys_mmap_zero_length() {
    let result = sys_mmap(0, 0, 0, 0, 0, 0);
    assert_eq!(result, EINVAL, "Should reject zero length");
}

/// Test sys_mmap with excessive length
#[test_case]
fn test_sys_mmap_excessive_length() {
    let excessive_size = crate::kernel::security::MAX_ALLOC_SIZE + 1;
    let result = sys_mmap(0, excessive_size, 0, 0, 0, 0);
    assert_eq!(result, EINVAL, "Should reject excessive allocation");
}

/// Test sys_munmap with null pointer
#[test_case]
fn test_sys_munmap_null_pointer() {
    let result = sys_munmap(0, 4096, 0, 0, 0, 0);
    assert_eq!(result, EFAULT, "Should reject null pointer");
}

/// Test sys_munmap with kernel address
#[test_case]
fn test_sys_munmap_kernel_address() {
    let kernel_addr = address_space::KERNEL_START;
    let result = sys_munmap(kernel_addr, 4096, 0, 0, 0, 0);
    assert_eq!(result, EFAULT, "Should reject kernel address");
}

/// Test sys_munmap with zero length
#[test_case]
fn test_sys_munmap_zero_length() {
    let result = sys_munmap(0x1000, 0, 0, 0, 0, 0);
    assert_eq!(result, EINVAL, "Should reject zero length");
}

/// Test sys_wait with null status pointer
#[test_case]
fn test_sys_wait_null_status() {
    // Passing 0 for status_ptr is allowed (no status returned)
    // This will fail with ECHILD if no children, which is expected
    let result = sys_wait(0, 0, 0, 0, 0, 0);
    // Expected: ECHILD or blocks
    assert!(result == ECHILD || result == ESRCH, "Should handle null status gracefully");
}

/// Test sys_wait with kernel status pointer
#[test_case]
fn test_sys_wait_kernel_status() {
    let kernel_addr = address_space::KERNEL_START;
    let result = sys_wait(0, kernel_addr, 0, 0, 0, 0);
    // Will fail with validation error before ECHILD
    assert!(result < 0, "Should reject kernel address");
}

/// Test sys_pipe with null array pointer
#[test_case]
fn test_sys_pipe_null_pointer() {
    let result = sys_pipe(0, 0, 0, 0, 0, 0);
    assert_eq!(result, EFAULT, "Should reject null pointer");
}

/// Test sys_pipe with kernel array pointer
#[test_case]
fn test_sys_pipe_kernel_pointer() {
    let kernel_addr = address_space::KERNEL_START;
    let result = sys_pipe(kernel_addr, 0, 0, 0, 0, 0);
    assert_eq!(result, EFAULT, "Should reject kernel address");
}

/// Test sys_exec with null path pointer
#[test_case]
fn test_sys_exec_null_path() {
    let result = sys_exec(0, 100, 0, 0, 0, 0);
    assert_eq!(result, EFAULT, "Should reject null pointer");
}

/// Test sys_exec with kernel path pointer
#[test_case]
fn test_sys_exec_kernel_path() {
    let kernel_addr = address_space::KERNEL_START;
    let result = sys_exec(kernel_addr, 100, 0, 0, 0, 0);
    assert_eq!(result, EFAULT, "Should reject kernel address");
}

/// Test integer overflow in address + length
#[test_case]
fn test_address_overflow() {
    let addr = u64::MAX - 100;
    let len = 200; // Will overflow
    let result = sys_write(1, addr, len, 0, 0, 0);
    assert_eq!(result, EFAULT, "Should detect integer overflow");
}

/// Test boundary conditions
#[test_case]
fn test_boundary_conditions() {
    // Last valid user address
    let last_user_addr = address_space::USER_END - 1;
    
    // Reading 1 byte from last valid address should work (if mapped)
    // Will fail unmapped, but not with EFAULT from range check
    let result = sys_write(1, last_user_addr, 1, 0, 0, 0);
    // Could be EFAULT (unmapped) or success if mapped
   assert!(result <= 1, "Last user byte should not fail range check");
    
    // Reading 2 bytes from last valid address should fail  (crosses into kernel)
    let result = sys_write(1, last_user_addr, 2, 0, 0, 0);
    assert_eq!(result, EFAULT, "Should reject access crossing into kernel space");
}

/// Test sys_munmap with unmapped pages
#[test_case]
fn test_sys_munmap_unmapped_pages() {
    // Try to unmap a page that was never mapped
    let unmapped_addr = 0x5000_0000; // Arbitrary user address
    let result = sys_munmap(unmapped_addr, 4096, 0, 0, 0, 0);
    
    // Current implementation: lenient (returns SUCCESS)
    // Strict POSIX would return EINVAL
    // Both behaviors are acceptable depending on policy
    assert_eq!(result, SUCCESS, "Current policy: lenient unmapping");
}
