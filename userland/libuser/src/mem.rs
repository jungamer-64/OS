//! Memory management API

use crate::syscall;

/// Memory protection flags
pub const PROT_READ: u64 = 1;
pub const PROT_WRITE: u64 = 2;
pub const PROT_EXEC: u64 = 4;

/// Memory mapping flags
pub const MAP_PRIVATE: u64 = 1;
pub const MAP_ANONYMOUS: u64 = 2;

/// Allocate memory using mmap
///
/// Returns the address of the allocated memory, or negative error code
pub fn alloc(size: u64) -> i64 {
    syscall::mmap(0, size, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS)
}

/// Deallocate memory using munmap
pub fn dealloc(addr: u64, size: u64) -> i64 {
    syscall::munmap(addr, size)
}

/// Map memory with specific protection and flags
pub fn mmap(addr: u64, len: u64, prot: u64, flags: u64) -> i64 {
    syscall::mmap(addr, len, prot, flags)
}
