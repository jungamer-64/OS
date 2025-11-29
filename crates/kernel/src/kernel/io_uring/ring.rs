// kernel/src/kernel/io_uring/ring.rs
//! Ring buffer implementation for io_uring
//!
//! This module provides the core ring buffer data structures that enable
//! lock-free communication between user space and kernel space.
//!
//! ## Memory Layout
//!
//! io_uring uses page-aligned buffers for shared memory between kernel and user:
//! - sq_header: 1 page (4 KiB)
//! - cq_header: 1 page (4 KiB)
//! - sq_entries: 4 pages (256 * 64 = 16 KiB)
//! - cq_entries: 3 pages (256 * 40 = 10 KiB, rounded to 12 KiB)
//!
//! Total: 10 pages (40 KiB)

use core::sync::atomic::Ordering;
use alloc::vec::Vec;

use x86_64::structures::paging::{PhysFrame, FrameAllocator, Size4KiB, PageSize};
use x86_64::VirtAddr;

use crate::abi::io_uring_v2::{
    SubmissionEntryV2, CompletionEntryV2, RingHeaderV2, V2Features
};
use crate::abi::io_uring_common::{RING_SIZE, RING_MASK};
use crate::abi::error::SyscallError;
use crate::debug_println;
use crate::kernel::syscall::{EFAULT, EINVAL, ENOMEM, EAGAIN, SyscallResult};
use crate::kernel::mm::PHYS_MEM_OFFSET;

/// io_uring error types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoUringError {
    /// Invalid argument
    InvalidArgument,
    /// Out of memory
    OutOfMemory,
    /// Queue is full
    QueueFull,
    /// Queue is empty
    QueueEmpty,
    /// Invalid user pointer
    BadAddress,
    /// Operation not supported
    NotSupported,
    /// Internal error
    Internal,
}

impl IoUringError {
    /// Convert to errno value
    pub const fn to_errno(self) -> i32 {
        match self {
            Self::InvalidArgument => 22, // EINVAL
            Self::OutOfMemory => 12,     // ENOMEM
            Self::QueueFull => 11,       // EAGAIN
            Self::QueueEmpty => 11,      // EAGAIN
            Self::BadAddress => 14,      // EFAULT
            Self::NotSupported => 38,    // ENOSYS
            Self::Internal => 5,         // EIO
        }
    }
    
    /// Convert to V2 SyscallError
    pub const fn to_syscall_error(self) -> SyscallError {
        match self {
            Self::InvalidArgument => SyscallError::InvalidArgument,
            Self::OutOfMemory => SyscallError::OutOfMemory,
            Self::QueueFull => SyscallError::WouldBlock,
            Self::QueueEmpty => SyscallError::WouldBlock,
            Self::BadAddress => SyscallError::InvalidAddress,
            Self::NotSupported => SyscallError::NotImplemented,
            Self::Internal => SyscallError::IoError,
        }
    }
}

/// io_uring statistics
#[derive(Debug, Clone, Copy)]
pub struct IoUringStats {
    /// Total submissions processed
    pub submissions_total: u64,
    /// Total completions posted
    pub completions_total: u64,
    /// Dropped submissions (CQ overflow)
    pub dropped_submissions: u64,
    /// Current pending submissions in SQ
    pub sq_pending: u32,
    /// Current pending completions in CQ
    pub cq_pending: u32,
}

// Memory layout constants for V2 (40-byte CQE)
const SQ_HEADER_PAGES: usize = 1;
const CQ_HEADER_PAGES: usize = 1;
const SQ_ENTRIES_PAGES: usize = 4; // 256 entries * 64 bytes = 16 KiB
const CQ_ENTRIES_PAGES: usize = 3; // 256 entries * 40 bytes = 10 KiB, rounded to 12 KiB

/// Main io_uring ring structure (V2)
pub struct IoUring {
    // Submission queue
    sq_header: *mut RingHeaderV2,
    sq_entries: *mut SubmissionEntryV2,
    
    // Completion queue
    cq_header: *mut RingHeaderV2,
    cq_entries: *mut CompletionEntryV2,
    
    // Pending submissions (kernel-side buffer)
    pending_submissions: Vec<SubmissionEntryV2>,
    
    // Statistics
    submissions_total: u64,
    completions_total: u64,
    dropped_submissions: u64,
    
    // Memory management
    _sq_header_frames: Vec<PhysFrame>,
    _cq_header_frames: Vec<PhysFrame>,
    _sq_entries_frames: Vec<PhysFrame>,
    _cq_entries_frames: Vec<PhysFrame>,
}

// SAFETY: IoUring contains raw pointers to page-aligned kernel memory.
// These pointers are:
// 1. Allocated from physical frames via FrameAllocator
// 2. Mapped to kernel virtual address space with appropriate flags
// 3. Never deallocated (frames are kept alive via _*_frames fields)
// 4. Accessed exclusively through synchronized methods (via Process -> Mutex)
//
// Therefore, it is safe to send the IoUring between threads as long as:
// - The pointers remain valid (guaranteed by holding PhysFrames)
// - Access is synchronized (guaranteed by Mutex<ProcessTable>)
unsafe impl Send for IoUring {}
unsafe impl Sync for IoUring {}

impl IoUring {
    /// Create a new io_uring instance with page-aligned buffers
    ///
    /// This allocates physical frames and maps them to kernel virtual addresses.
    ///
    /// # Arguments
    /// * `allocator` - Frame allocator for allocating physical memory
    ///
    /// # Returns
    /// * `Some(IoUring)` on success
    /// * `None` if allocation fails
    pub fn new_with_allocator<A>(allocator: &mut A) -> Option<Self>
    where
        A: FrameAllocator<Size4KiB>,
    {
        let phys_offset = VirtAddr::new(
            PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed)
        );
        
        // Allocate SQ header
        let sq_header_frames = Self::allocate_frames(allocator, SQ_HEADER_PAGES)?;
        let sq_header = (phys_offset + sq_header_frames[0].start_address().as_u64()).as_mut_ptr();
        
        // Allocate CQ header
        let cq_header_frames = Self::allocate_frames(allocator, CQ_HEADER_PAGES)?;
        let cq_header = (phys_offset + cq_header_frames[0].start_address().as_u64()).as_mut_ptr();
        
        // Allocate SQ entries
        let sq_entries_frames = Self::allocate_frames(allocator, SQ_ENTRIES_PAGES)?;
        let sq_entries = (phys_offset + sq_entries_frames[0].start_address().as_u64()).as_mut_ptr();
        
        // Allocate CQ entries
        let cq_entries_frames = Self::allocate_frames(allocator, CQ_ENTRIES_PAGES)?;
        let cq_entries = (phys_offset + cq_entries_frames[0].start_address().as_u64()).as_mut_ptr();
        
        // Initialize headers
        unsafe {
            *sq_header = RingHeaderV2::new(V2Features::ALL_V2);
            *cq_header = RingHeaderV2::new(V2Features::ALL_V2);
        }
        
        Some(Self {
            sq_header,
            sq_entries,
            cq_header,
            cq_entries,
            pending_submissions: Vec::with_capacity(RING_SIZE as usize),
            submissions_total: 0,
            completions_total: 0,
            dropped_submissions: 0,
            _sq_header_frames: sq_header_frames,
            _cq_header_frames: cq_header_frames,
            _sq_entries_frames: sq_entries_frames,
            _cq_entries_frames: cq_entries_frames,
        })
    }
    
    /// Helper to allocate multiple frames
    fn allocate_frames<A>(allocator: &mut A, count: usize) -> Option<Vec<PhysFrame>>
    where
        A: FrameAllocator<Size4KiB>,
    {
        let mut frames = Vec::with_capacity(count);
        for _ in 0..count {
            frames.push(allocator.allocate_frame()?);
        }
        Some(frames)
    }
    
    /// Get SQ header reference
    #[must_use]
    pub fn sq_header(&self) -> &RingHeaderV2 {
        unsafe { &*self.sq_header }
    }
    
    /// Get CQ header reference
    #[must_use]
    pub fn cq_header(&self) -> &RingHeaderV2 {
        unsafe { &*self.cq_header }
    }
    
    /// Get SQ entries slice
    #[must_use]
    pub fn sq_entries(&self) -> &[SubmissionEntryV2] {
        unsafe { core::slice::from_raw_parts(self.sq_entries, RING_SIZE as usize) }
    }
    
    /// Get CQ entries mutable slice
    #[must_use]
    pub fn cq_entries_mut(&mut self) -> &mut [CompletionEntryV2] {
        unsafe { core::slice::from_raw_parts_mut(self.cq_entries, RING_SIZE as usize) }
    }
    
    /// Harvest new submissions from the submission queue
    ///
    /// Copies pending SQEs from shared memory to kernel buffer.
    ///
    /// # Returns
    /// Number of entries harvested
    pub fn harvest_submissions(&mut self) -> u32 {
        let head = self.sq_header().head.load(core::sync::atomic::Ordering::Acquire);
        let tail = self.sq_header().tail.load(core::sync::atomic::Ordering::Acquire);
        
        if head == tail {
            return 0; // Queue is empty
        }
        
        
        let mut count = 0;
        
        let mut current = head;
        while current != tail {
            let index = (current & RING_MASK) as usize;
            // Access and copy the SQE, then immediately drop the borrow
            let sqe = self.sq_entries()[index];
            
            // Copy to pending buffer (now sq_entries borrow is dropped)
            self.pending_submissions.push(sqe);
            
            current = current.wrapping_add(1);
            count += 1;
        }
        
        // Update head
        unsafe { (*self.sq_header).advance_head(count); }
        
        self.submissions_total += u64::from(count);
        count
    }
    
    /// Pop a pending submission entry
    ///
    /// # Returns
    /// * `Some(sqe)` if there are pending submissions
    /// * `None` if the pending queue is empty
    pub fn pop_pending(&mut self) -> Option<SubmissionEntryV2> {
        self.pending_submissions.pop()
    }
    
    /// Post a completion entry to the CQ
    ///
    /// # Arguments
    /// * `cqe` - Completion entry to post
    ///
    /// # Returns
    /// * `Ok(())` on success
    /// * `Err(IoUringError::QueueFull)` if CQ is full
    pub fn post_completion(&mut self, cqe: CompletionEntryV2) -> Result<(), IoUringError> {
        // Check if CQ is full
        if self.cq_header().pending_count() >= RING_SIZE {
            self.dropped_submissions += 1;
            return Err(IoUringError::QueueFull);
        }
        
        let tail = self.cq_header().tail.load(core::sync::atomic::Ordering::Acquire);
        let index = (tail & RING_MASK) as usize;
        
        // Write CQE
        let cq_entries = self.cq_entries_mut();
        cq_entries[index] = cqe;
        
        // Update tail
        unsafe { (*self.cq_header).advance_tail(1); }
        
        self.completions_total += 1;
        Ok(())
    }
    
    /// Post a success completion
    ///
    /// # Arguments
    /// * `user_data` - User data from the corresponding SQE
    /// * `result` - Operation result value
    ///
    /// # Returns
    /// * `Ok(())` on success
    /// * `Err(IoUringError::QueueFull)` if CQ is full
    pub fn complete_success(&mut self, user_data: u64, result: u64) -> Result<(), IoUringError> {
        let cqe = CompletionEntryV2::success(user_data, result as i32);
        self.post_completion(cqe)
    }
    
    /// Post an error completion
    ///
    /// # Arguments
    /// * `user_data` - User data from the corresponding SQE
    /// * `error` - Error code
    ///
    /// # Returns
    /// * `Ok(())` on success
    /// * `Err(IoUringError::QueueFull)` if CQ is full
    pub fn complete_error(&mut self, user_data: u64, error: SyscallError) -> Result<(), IoUringError> {
        let cqe = CompletionEntryV2::error(user_data, error);
        self.post_completion(cqe)
    }
    
    /// Get the number of completions available in CQ
    #[must_use]
    pub fn completion_count(&self) -> u32 {
        self.cq_header().pending_count()
    }
    
    /// Get SQ header physical address for mapping to user space
    #[must_use]
    pub fn sq_header_addr(&self) -> u64 {
        self.sq_header as u64
    }
    
    /// Get CQ header physical address for mapping to user space
    #[must_use]
    pub fn cq_header_addr(&self) -> u64 {
        self.cq_header as u64
    }
    
    /// Get SQ entries physical address for mapping to user space
    #[must_use]
    pub fn sq_entries_addr(&self) -> u64 {
        self.sq_entries as u64
    }
    
    /// Get CQ entries physical address for mapping to user space
    #[must_use]
    pub fn cq_entries_addr(&self) -> u64 {
        self.cq_entries as u64
    }
    
    /// Get statistics
    #[must_use]
    pub fn stats(&self) -> IoUringStats {
        IoUringStats {
            submissions_total: self.submissions_total,
            completions_total: self.completions_total,
            dropped_submissions: self.dropped_submissions,
            sq_pending: self.sq_header().pending_count(),
            cq_pending: self.cq_header().pending_count(),
        }
    }
}

// Note: IoUring no longer implements Default because it requires a frame allocator

// Unit tests require a mock frame allocator which is complex to set up
// These tests are disabled for now and should be tested via integration tests
#[cfg(test)]
mod tests {
    // Tests disabled - IoUring::new_with_allocator requires a frame allocator
    // which is not available in unit test context.
    // Integration tests should verify io_uring functionality.
}
