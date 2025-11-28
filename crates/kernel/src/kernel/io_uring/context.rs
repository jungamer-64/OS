// kernel/src/kernel/io_uring/context.rs
//! Per-process io_uring context
//!
//! Each process can have an io_uring context that manages its submission
//! and completion queues, registered buffers, and SQPOLL configuration.

use spin::Mutex;

use super::ring::IoUring;
use super::handlers::{dispatch_sqe, OpResult};
use super::registered_buffers::{RegisteredBufferTable, RegisteredBufferStats};
use crate::abi::io_uring::OpCode;
use crate::debug_println;
use crate::kernel::mm::BootInfoFrameAllocator;
use crate::kernel::syscall::SyscallResult;

/// Per-process io_uring context
///
/// Manages the io_uring instance for a process, including:
/// - The ring buffers
/// - Pending async operations
/// - Registered buffers for zero-copy I/O
/// - SQPOLL configuration
pub struct IoUringContext {
    /// The io_uring instance
    ring: IoUring,
    
    /// Number of operations in flight
    in_flight: u32,
    
    /// Maximum operations in flight (rate limiting)
    max_in_flight: u32,
    
    /// Registered buffers for zero-copy I/O
    registered_buffers: RegisteredBufferTable,
    
    /// Whether SQPOLL is enabled for this context
    sqpoll_enabled: bool,
}

impl IoUringContext {
    /// Create a new io_uring context with page-aligned buffers
    ///
    /// # Arguments
    /// * `allocator` - Frame allocator for allocating page-aligned memory
    ///
    /// # Returns
    /// * `Some(IoUringContext)` on success
    /// * `None` if allocation fails
    pub fn new_with_allocator(allocator: &mut BootInfoFrameAllocator) -> Option<Self> {
        let ring = IoUring::new_with_allocator(allocator)?;
        Some(Self {
            ring,
            in_flight: 0,
            max_in_flight: 256, // Match ring size
            registered_buffers: RegisteredBufferTable::new(),
            sqpoll_enabled: false,
        })
    }
    
    /// Create a new io_uring context with SQPOLL enabled
    pub fn new_with_sqpoll(allocator: &mut BootInfoFrameAllocator) -> Option<Self> {
        let mut ctx = Self::new_with_allocator(allocator)?;
        ctx.sqpoll_enabled = true;
        Some(ctx)
    }
    
    /// Register a buffer for zero-copy I/O
    ///
    /// # Arguments
    /// * `user_addr` - User-space virtual address
    /// * `len` - Buffer length in bytes
    /// * `readable` - Allow kernel to write to buffer
    /// * `writable` - Allow kernel to read from buffer
    ///
    /// # Returns
    /// * `Ok(index)` - Buffer index for use in SQEs
    /// * `Err(errno)` - Error code
    pub fn register_buffer(
        &mut self,
        user_addr: u64,
        len: usize,
        readable: bool,
        writable: bool,
    ) -> Result<u32, SyscallResult> {
        self.registered_buffers.register(user_addr, len, readable, writable)
    }
    
    /// Register multiple buffers from a user-provided iovec array
    pub fn register_buffers(&mut self, user_iov: u64, count: usize) -> Result<u32, SyscallResult> {
        self.registered_buffers.register_buffers(user_iov, count)
    }
    
    /// Unregister a buffer
    pub fn unregister_buffer(&mut self, index: u32) -> Result<(), SyscallResult> {
        self.registered_buffers.unregister(index)
    }
    
    /// Unregister all buffers
    pub fn unregister_all_buffers(&mut self) -> Result<(), SyscallResult> {
        self.registered_buffers.unregister_all()
    }
    
    /// Get a registered buffer by index
    pub fn get_registered_buffer(&self, index: u32) -> Option<&super::registered_buffers::RegisteredBuffer> {
        self.registered_buffers.get(index)
    }
    
    /// Acquire a reference to a registered buffer
    pub fn acquire_buffer(&self, index: u32) -> Option<super::registered_buffers::RegisteredBufferRef<'_>> {
        self.registered_buffers.acquire(index)
    }
    
    /// Get registered buffer statistics
    pub fn buffer_stats(&self) -> RegisteredBufferStats {
        self.registered_buffers.stats()
    }
    
    /// Check if SQPOLL is enabled
    pub fn is_sqpoll_enabled(&self) -> bool {
        self.sqpoll_enabled
    }
    
    /// Enable SQPOLL for this context
    pub fn enable_sqpoll(&mut self) {
        self.sqpoll_enabled = true;
    }
    
    /// Disable SQPOLL for this context
    pub fn disable_sqpoll(&mut self) {
        self.sqpoll_enabled = false;
    }
    
    /// Process the submission queue
    ///
    /// This function:
    /// 1. Harvests pending submissions from SQ
    /// 2. Validates each SQE
    /// 3. Dispatches to handlers
    /// 4. Posts completions to CQ
    ///
    /// # Returns
    /// Number of operations completed
    pub fn process(&mut self) -> u32 {
        // Harvest new submissions
        let harvested = self.ring.harvest_submissions();
        
        if harvested > 0 {
            debug_println!("[io_uring] Harvested {} submissions", harvested);
        }
        
        let mut completed = 0;
        
        // Process pending SQEs
        while let Some(sqe) = self.ring.pop_pending() {
            // Validate SQE
            if let Err(e) = super::ring::validate_sqe(&sqe) {
                // Post error completion
                let _ = self.ring.post_completion(sqe.user_data, e.to_errno() as i32, 0);
                completed += 1;
                continue;
            }
            
            // Handle exit specially (doesn't use ring)
            if sqe.opcode == OpCode::Exit as u8 {
                debug_println!("[io_uring] Exit requested via SQE, user_data={}", sqe.user_data);
                // Exit will be handled at syscall level
                continue;
            }
            
            // Dispatch to handler
            let result = dispatch_sqe(&sqe);
            
            // Post completion
            if self.ring.post_completion(result.user_data, result.result, result.flags).is_ok() {
                completed += 1;
            }
        }
        
        if completed > 0 {
            debug_println!("[io_uring] Completed {} operations", completed);
        }
        
        completed
    }
    
    /// Submit operations and wait for completions
    ///
    /// This is the main syscall entry point for io_uring.
    ///
    /// # Arguments
    /// * `min_complete` - Minimum number of completions to wait for (0 = non-blocking)
    ///
    /// # Returns
    /// Number of completions available in CQ
    pub fn enter(&mut self, min_complete: u32) -> u32 {
        // Process any pending submissions
        self.process();
        
        // If min_complete > 0, we would wait for completions
        // For now, we just return the completion count
        // True async waiting would require scheduler integration
        
        let cq_count = self.ring.completion_count();
        
        if min_complete > 0 && cq_count < min_complete {
            // In a real implementation, we would:
            // 1. Block the current task
            // 2. Register a wakeup when completions are available
            // 3. Resume when min_complete are ready
            //
            // For now, just return what we have
            debug_println!(
                "[io_uring] enter: requested {} completions, have {}",
                min_complete, cq_count
            );
        }
        
        cq_count
    }
    
    /// Get the SQ header address for mapping to user space
    #[must_use]
    pub fn sq_header_addr(&self) -> u64 {
        self.ring.sq_header_addr()
    }
    
    /// Get the CQ header address for mapping to user space
    #[must_use]
    pub fn cq_header_addr(&self) -> u64 {
        self.ring.cq_header_addr()
    }
    
    /// Get the SQ entries address for mapping to user space
    #[must_use]
    pub fn sq_entries_addr(&self) -> u64 {
        self.ring.sq_entries_addr()
    }
    
    /// Get the CQ entries address for mapping to user space
    #[must_use]
    pub fn cq_entries_addr(&self) -> u64 {
        self.ring.cq_entries_addr()
    }
    
    /// Get ring statistics
    #[must_use]
    pub fn stats(&self) -> super::ring::IoUringStats {
        self.ring.stats()
    }
}

// Note: IoUringContext no longer implements Default because it requires a frame allocator

/// Global io_uring processing function
///
/// This can be called from the scheduler or a dedicated kernel thread
/// to process io_uring operations across all processes.
pub fn process_all_rings() {
    use crate::kernel::process::PROCESS_TABLE;
    
    // TODO: Iterate through all processes with active io_uring contexts
    // and process their rings.
    //
    // For now, we only process when explicitly called via syscall.
}

// Unit tests disabled - IoUringContext requires a frame allocator
#[cfg(test)]
mod tests {
    // Tests disabled - IoUringContext::new_with_allocator requires a frame allocator
    // which is not available in unit test context.
}
