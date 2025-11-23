//! Memory management API
//!
//! This module provides safe memory allocation and mapping functions.

use crate::syscall::{self, SyscallResult};

/// Memory protection flags
pub const PROT_NONE: u64 = 0;
pub const PROT_READ: u64 = 1;
pub const PROT_WRITE: u64 = 2;
pub const PROT_EXEC: u64 = 4;

/// Memory mapping flags
pub const MAP_PRIVATE: u64 = 1;
pub const MAP_ANONYMOUS: u64 = 2;
pub const MAP_SHARED: u64 = 4;

/// Allocate memory using mmap
///
/// This is a convenience function that allocates anonymous, private memory
/// with read/write permissions.
///
/// # Arguments
/// * `size` - Number of bytes to allocate (will be rounded up to page size)
///
/// # Returns
/// Address of allocated memory
///
/// # Errors
/// * `EINVAL` - Invalid size (0)
/// * `ENOMEM` - Out of memory
///
/// # Examples
/// ```no_run
/// use libuser::mem::alloc;
///
/// match alloc(4096) {
///     Ok(addr) => {
///         // Use the memory at addr
///         println!("Allocated memory at 0x{:x}", addr);
///     }
///     Err(e) => {
///         println!("Allocation failed: {}", e.description());
///     }
/// }
/// ```
pub fn alloc(size: u64) -> SyscallResult<u64> {
    mmap(0, size, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS)
}

/// Deallocate memory using munmap
///
/// # Arguments
/// * `addr` - Address of memory to deallocate
/// * `size` - Size of memory region in bytes
///
/// # Returns
/// Success or error
///
/// # Errors
/// * `EINVAL` - Invalid arguments
///
/// # Safety
/// The caller must ensure that:
/// - The memory region was previously allocated with `alloc()` or `mmap()`
/// - No references to the memory region exist
/// - The address and size match the original allocation
///
/// # Examples
/// ```no_run
/// use libuser::mem::{alloc, dealloc};
///
/// let size = 4096;
/// match alloc(size) {
///     Ok(addr) => {
///         // Use the memory...
///         
///         // Then deallocate it
///         match dealloc(addr, size) {
///             Ok(()) => println!("Memory freed"),
///             Err(e) => println!("Deallocation failed: {}", e.description()),
///         }
///     }
///     Err(_) => {}
/// }
/// ```
pub fn dealloc(addr: u64, size: u64) -> SyscallResult<()> {
    syscall::munmap(addr, size)
}

/// Map memory with specific protection and flags
///
/// # Arguments
/// * `addr` - Desired address (0 = kernel chooses)
/// * `len` - Length in bytes (will be rounded up to page size)
/// * `prot` - Memory protection flags (PROT_*)
/// * `flags` - Mapping flags (MAP_*)
///
/// # Returns
/// Address of mapped memory
///
/// # Errors
/// * `EINVAL` - Invalid arguments
/// * `ENOMEM` - Out of memory
///
/// # Examples
/// ```no_run
/// use libuser::mem::{mmap, PROT_READ, PROT_WRITE, MAP_PRIVATE, MAP_ANONYMOUS};
///
/// // Allocate 8KB of read-only memory
/// match mmap(0, 8192, PROT_READ, MAP_PRIVATE | MAP_ANONYMOUS) {
///     Ok(addr) => println!("Mapped at 0x{:x}", addr),
///     Err(e) => println!("Mapping failed: {}", e.description()),
/// }
/// ```
pub fn mmap(addr: u64, len: u64, prot: u64, flags: u64) -> SyscallResult<u64> {
    syscall::mmap(addr, len, prot, flags)
}

/// Memory region handle (RAII wrapper)
///
/// Automatically unmaps the memory when dropped.
pub struct MemoryRegion {
    addr: u64,
    size: u64,
}

impl MemoryRegion {
    /// Allocate a new memory region
    ///
    /// # Arguments
    /// * `size` - Size in bytes
    ///
    /// # Returns
    /// Memory region handle
    pub fn new(size: u64) -> SyscallResult<Self> {
        let addr = alloc(size)?;
        Ok(Self { addr, size })
    }

    /// Get the address of the memory region
    pub const fn addr(&self) -> u64 {
        self.addr
    }

    /// Get the size of the memory region
    pub const fn size(&self) -> u64 {
        self.size
    }

    /// Get a pointer to the memory region
    ///
    /// # Safety
    /// The caller must ensure proper use of the returned pointer.
    pub const fn as_ptr(&self) -> *const u8 {
        self.addr as *const u8
    }

    /// Get a mutable pointer to the memory region
    ///
    /// # Safety
    /// The caller must ensure proper use of the returned pointer.
    pub const fn as_mut_ptr(&self) -> *mut u8 {
        self.addr as *mut u8
    }
}

impl Drop for MemoryRegion {
    fn drop(&mut self) {
        // Ignore errors during drop
        let _ = dealloc(self.addr, self.size);
    }
}
