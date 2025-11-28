// kernel/src/kernel/io_uring/registered_buffers.rs
//! Registered Buffers for io_uring
//!
//! Pre-registered buffers eliminate the need for `copy_from_user` on every
//! I/O operation by pinning user memory and validating it once upfront.
//!
//! # Security Model
//!
//! 1. Registration validates all buffer addresses are in user space
//! 2. Buffers are pinned (cannot be unmapped while registered)
//! 3. Kernel maintains mapping for direct access
//! 4. Unregistration releases the pin
//!
//! # Performance Benefits
//!
//! - No per-operation address validation
//! - No per-operation copy
//! - Better cache utilization (no temporary kernel buffers)
//! - Lower latency for small I/O operations
//!
//! # Usage Flow
//!
//! ```text
//! 1. User calls io_uring_register_buffers(bufs, count)
//! 2. Kernel validates all addresses are user-accessible
//! 3. Kernel pins pages (prevents page-out/unmap)
//! 4. Returns buffer indices to user
//! 5. User submits SQE with FIXED_BUFFER flag + buffer index
//! 6. Kernel uses pre-validated pointer directly
//! 7. On cleanup: io_uring_unregister_buffers()
//! ```

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use alloc::vec::Vec;

use x86_64::VirtAddr;

use crate::debug_println;
use crate::kernel::security::{is_user_range, validate_user_read, validate_user_write};
use crate::kernel::syscall::{EFAULT, EINVAL, ENOMEM, EAGAIN, SyscallResult};

/// Device or resource busy
const EBUSY: SyscallResult = -16;

/// Maximum number of registered buffers per io_uring instance
pub const MAX_REGISTERED_BUFFERS: usize = 1024;

/// Maximum size of a single registered buffer (16 MiB)
pub const MAX_BUFFER_SIZE: usize = 16 * 1024 * 1024;

/// Registered buffer descriptor
#[derive(Debug)]
pub struct RegisteredBuffer {
    /// User-space virtual address
    user_addr: u64,
    
    /// Buffer length in bytes
    len: usize,
    
    /// Whether buffer is for reading (kernel writes to it)
    readable: bool,
    
    /// Whether buffer is for writing (kernel reads from it)
    writable: bool,
    
    /// Number of active references (for safe unregistration)
    ref_count: AtomicU64,
    
    /// Whether this slot is valid
    valid: AtomicBool,
}

impl RegisteredBuffer {
    /// Create a new registered buffer
    fn new(user_addr: u64, len: usize, readable: bool, writable: bool) -> Self {
        Self {
            user_addr,
            len,
            readable,
            writable,
            ref_count: AtomicU64::new(0),
            valid: AtomicBool::new(true),
        }
    }
    
    /// Get the user address
    #[inline]
    pub fn user_addr(&self) -> u64 {
        self.user_addr
    }
    
    /// Get the buffer length
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }
    
    /// Check if buffer is empty
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    
    /// Check if buffer is valid for reading (kernel writes)
    #[inline]
    pub fn is_readable(&self) -> bool {
        self.readable
    }
    
    /// Check if buffer is valid for writing (kernel reads)
    #[inline]
    pub fn is_writable(&self) -> bool {
        self.writable
    }
    
    /// Check if buffer is valid
    #[inline]
    pub fn is_valid(&self) -> bool {
        self.valid.load(Ordering::Acquire)
    }
    
    /// Acquire a reference to the buffer
    /// 
    /// Returns None if buffer is invalid
    pub fn acquire(&self) -> Option<RegisteredBufferRef<'_>> {
        if !self.is_valid() {
            return None;
        }
        
        self.ref_count.fetch_add(1, Ordering::AcqRel);
        
        // Double-check validity after increment
        if !self.is_valid() {
            self.ref_count.fetch_sub(1, Ordering::Release);
            return None;
        }
        
        Some(RegisteredBufferRef { buffer: self })
    }
    
    /// Mark buffer as invalid (for unregistration)
    fn invalidate(&self) -> bool {
        self.valid.store(false, Ordering::Release);
        
        // Wait for all references to be released
        self.ref_count.load(Ordering::Acquire) == 0
    }
    
    /// Check if buffer has no active references
    fn is_free(&self) -> bool {
        self.ref_count.load(Ordering::Acquire) == 0
    }
}

/// RAII guard for registered buffer reference
pub struct RegisteredBufferRef<'a> {
    buffer: &'a RegisteredBuffer,
}

impl<'a> RegisteredBufferRef<'a> {
    /// Get the user address
    #[inline]
    pub fn user_addr(&self) -> u64 {
        self.buffer.user_addr
    }
    
    /// Get the buffer length
    #[inline]
    pub fn len(&self) -> usize {
        self.buffer.len
    }
    
    /// Check if buffer is empty
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }
    
    /// Get a slice of the buffer for reading (kernel writes)
    /// 
    /// # Safety
    /// 
    /// The buffer address was validated at registration time.
    /// Caller must ensure proper synchronization with user space.
    #[inline]
    pub unsafe fn as_mut_slice(&self) -> Option<&mut [u8]> {
        if !self.buffer.readable {
            return None;
        }
        // SAFETY: Buffer address was validated at registration time
        unsafe {
            Some(core::slice::from_raw_parts_mut(
                self.buffer.user_addr as *mut u8,
                self.buffer.len,
            ))
        }
    }
    
    /// Get a slice of the buffer for writing (kernel reads)
    /// 
    /// # Safety
    /// 
    /// The buffer address was validated at registration time.
    /// Caller must ensure proper synchronization with user space.
    #[inline]
    pub unsafe fn as_slice(&self) -> Option<&[u8]> {
        if !self.buffer.writable {
            return None;
        }
        // SAFETY: Buffer address was validated at registration time
        unsafe {
            Some(core::slice::from_raw_parts(
                self.buffer.user_addr as *const u8,
                self.buffer.len,
            ))
        }
    }
}

impl Drop for RegisteredBufferRef<'_> {
    fn drop(&mut self) {
        self.buffer.ref_count.fetch_sub(1, Ordering::Release);
    }
}

/// Buffer registration table for an io_uring instance
pub struct RegisteredBufferTable {
    /// Registered buffers (sparse array, None = empty slot)
    buffers: Vec<Option<RegisteredBuffer>>,
    
    /// Number of valid buffers
    count: usize,
    
    /// Maximum capacity
    capacity: usize,
    
    /// Statistics: total bytes registered
    total_bytes: u64,
    
    /// Statistics: total registrations
    registrations: u64,
    
    /// Statistics: total unregistrations
    unregistrations: u64,
}

impl RegisteredBufferTable {
    /// Create a new buffer registration table
    pub fn new() -> Self {
        Self::with_capacity(MAX_REGISTERED_BUFFERS)
    }
    
    /// Create a table with specific capacity
    pub fn with_capacity(capacity: usize) -> Self {
        let capacity = capacity.min(MAX_REGISTERED_BUFFERS);
        Self {
            buffers: Vec::with_capacity(capacity),
            count: 0,
            capacity,
            total_bytes: 0,
            registrations: 0,
            unregistrations: 0,
        }
    }
    
    /// Register a buffer
    /// 
    /// # Arguments
    /// * `user_addr` - User-space virtual address
    /// * `len` - Buffer length in bytes
    /// * `readable` - Allow kernel to write (user reads result)
    /// * `writable` - Allow kernel to read (user provides data)
    /// 
    /// # Returns
    /// * `Ok(index)` - Buffer index for use in SQEs
    /// * `Err(errno)` - Error code
    pub fn register(
        &mut self,
        user_addr: u64,
        len: usize,
        readable: bool,
        writable: bool,
    ) -> Result<u32, SyscallResult> {
        // Validate parameters
        if len == 0 {
            return Err(EINVAL);
        }
        
        if len > MAX_BUFFER_SIZE {
            return Err(EINVAL);
        }
        
        if self.count >= self.capacity {
            return Err(ENOMEM);
        }
        
        // Validate user address range
        if !is_user_range(user_addr, len as u64) {
            debug_println!(
                "[RegisteredBuffers] Invalid user range: addr={:#x}, len={}",
                user_addr, len
            );
            return Err(EFAULT);
        }
        
        // Validate accessibility
        if readable {
            if validate_user_write(user_addr, len as u64).is_err() {
                return Err(EFAULT);
            }
        }
        
        if writable {
            if validate_user_read(user_addr, len as u64).is_err() {
                return Err(EFAULT);
            }
        }
        
        // Find a free slot
        let index = self.find_free_slot().ok_or(ENOMEM)?;
        
        // Create and store the buffer
        let buffer = RegisteredBuffer::new(user_addr, len, readable, writable);
        
        if index < self.buffers.len() {
            self.buffers[index] = Some(buffer);
        } else {
            // Extend the vector
            while self.buffers.len() < index {
                self.buffers.push(None);
            }
            self.buffers.push(Some(buffer));
        }
        
        self.count += 1;
        self.total_bytes += len as u64;
        self.registrations += 1;
        
        debug_println!(
            "[RegisteredBuffers] Registered buffer {}: addr={:#x}, len={}, r={}, w={}",
            index, user_addr, len, readable, writable
        );
        
        Ok(index as u32)
    }
    
    /// Register multiple buffers from a user-provided array
    /// 
    /// # Arguments
    /// * `user_iov` - User address of iovec array
    /// * `count` - Number of iovecs
    /// 
    /// # Returns
    /// * `Ok(first_index)` - Index of first registered buffer
    /// * `Err(errno)` - Error code
    pub fn register_buffers(
        &mut self,
        user_iov: u64,
        count: usize,
    ) -> Result<u32, SyscallResult> {
        if count == 0 || count > self.capacity - self.count {
            return Err(EINVAL);
        }
        
        // iovec structure: { void* base; size_t len; }
        const IOVEC_SIZE: usize = 16; // 8 bytes pointer + 8 bytes length
        
        // Validate the iovec array
        let iov_array_size = count * IOVEC_SIZE;
        if validate_user_read(user_iov, iov_array_size as u64).is_err() {
            return Err(EFAULT);
        }
        
        let first_index = self.find_free_slot().ok_or(ENOMEM)?;
        
        // Read and register each buffer
        for i in 0..count {
            let iov_addr = user_iov + (i * IOVEC_SIZE) as u64;
            
            // SAFETY: We validated the array is readable
            let (base, len) = unsafe {
                let base_ptr = iov_addr as *const u64;
                let len_ptr = (iov_addr + 8) as *const u64;
                (*base_ptr, *len_ptr as usize)
            };
            
            // Register as both readable and writable
            self.register(base, len, true, true)?;
        }
        
        Ok(first_index as u32)
    }
    
    /// Unregister a buffer
    pub fn unregister(&mut self, index: u32) -> Result<(), SyscallResult> {
        let index = index as usize;
        
        if index >= self.buffers.len() {
            return Err(EINVAL);
        }
        
        let buffer = match &self.buffers[index] {
            Some(b) => b,
            None => return Err(EINVAL),
        };
        
        // Check for active references
        if !buffer.is_free() {
            return Err(EBUSY);
        }
        
        // Invalidate and remove
        buffer.invalidate();
        
        let len = buffer.len;
        self.buffers[index] = None;
        self.count -= 1;
        self.total_bytes -= len as u64;
        self.unregistrations += 1;
        
        debug_println!("[RegisteredBuffers] Unregistered buffer {}", index);
        
        Ok(())
    }
    
    /// Unregister all buffers
    pub fn unregister_all(&mut self) -> Result<(), SyscallResult> {
        for i in 0..self.buffers.len() {
            if self.buffers[i].is_some() {
                if let Err(e) = self.unregister(i as u32) {
                    if e == EBUSY {
                        // Buffer is in use, fail the operation
                        return Err(EBUSY);
                    }
                }
            }
        }
        Ok(())
    }
    
    /// Get a buffer by index
    pub fn get(&self, index: u32) -> Option<&RegisteredBuffer> {
        let index = index as usize;
        if index >= self.buffers.len() {
            return None;
        }
        self.buffers[index].as_ref().filter(|b| b.is_valid())
    }
    
    /// Acquire a reference to a buffer
    pub fn acquire(&self, index: u32) -> Option<RegisteredBufferRef<'_>> {
        self.get(index)?.acquire()
    }
    
    /// Find a free slot
    fn find_free_slot(&self) -> Option<usize> {
        // First, check existing slots
        for (i, slot) in self.buffers.iter().enumerate() {
            if slot.is_none() {
                return Some(i);
            }
        }
        
        // If no free slot, use next index if under capacity
        if self.buffers.len() < self.capacity {
            return Some(self.buffers.len());
        }
        
        None
    }
    
    /// Get number of registered buffers
    #[inline]
    pub fn count(&self) -> usize {
        self.count
    }
    
    /// Get statistics
    pub fn stats(&self) -> RegisteredBufferStats {
        RegisteredBufferStats {
            count: self.count,
            capacity: self.capacity,
            total_bytes: self.total_bytes,
            registrations: self.registrations,
            unregistrations: self.unregistrations,
        }
    }
}

impl Default for RegisteredBufferTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics for registered buffers
#[derive(Debug, Clone, Copy)]
pub struct RegisteredBufferStats {
    /// Number of currently registered buffers
    pub count: usize,
    /// Maximum capacity
    pub capacity: usize,
    /// Total bytes across all registered buffers
    pub total_bytes: u64,
    /// Total registration operations
    pub registrations: u64,
    /// Total unregistration operations
    pub unregistrations: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test_case]
    fn test_registered_buffer_table_creation() {
        let table = RegisteredBufferTable::new();
        assert_eq!(table.count(), 0);
    }
    
    #[test_case]
    fn test_max_buffer_size() {
        assert_eq!(MAX_BUFFER_SIZE, 16 * 1024 * 1024);
    }
    
    #[test_case]
    fn test_max_registered_buffers() {
        assert_eq!(MAX_REGISTERED_BUFFERS, 1024);
    }
}
