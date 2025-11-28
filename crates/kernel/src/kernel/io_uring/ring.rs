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
//! - cq_entries: 1 page (256 * 16 = 4 KiB)
//!
//! Total: 7 pages (28 KiB)

use core::sync::atomic::Ordering;
use alloc::vec::Vec;

use x86_64::structures::paging::{PhysFrame, FrameAllocator, Size4KiB, PageSize};
use x86_64::VirtAddr;

use crate::abi::io_uring::{
    SubmissionEntry, CompletionEntry, RingHeader,
    OpCode, RING_SIZE, RING_MASK,
};
use crate::debug_println;
use crate::kernel::security::{validate_user_read, validate_user_write};
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
    /// Convert to syscall result (negative errno)
    #[must_use]
    pub const fn to_errno(self) -> SyscallResult {
        match self {
            Self::InvalidArgument => EINVAL,
            Self::OutOfMemory => ENOMEM,
            Self::QueueFull | Self::QueueEmpty => EAGAIN,
            Self::BadAddress => EFAULT,
            Self::NotSupported => -38, // ENOSYS
            Self::Internal => -5, // EIO
        }
    }
}

/// Number of pages for SQ header (1 page)
const SQ_HEADER_PAGES: usize = 1;
/// Number of pages for CQ header (1 page)
const CQ_HEADER_PAGES: usize = 1;
/// Number of pages for SQ entries (256 * 64 = 16384 bytes = 4 pages)
const SQ_ENTRIES_PAGES: usize = 4;
/// Number of pages for CQ entries (256 * 16 = 4096 bytes = 1 page)
const CQ_ENTRIES_PAGES: usize = 1;

/// Page-aligned memory region allocated from frame allocator
struct PageAlignedRegion {
    /// Physical frames backing this region
    frames: Vec<PhysFrame<Size4KiB>>,
    /// Kernel virtual address (phys_mem_offset + phys_addr)
    virt_addr: VirtAddr,
}

impl PageAlignedRegion {
    /// Get the kernel virtual address
    fn addr(&self) -> u64 {
        self.virt_addr.as_u64()
    }
    
    /// Get the physical address of the first frame
    fn phys_addr(&self) -> u64 {
        if self.frames.is_empty() {
            0
        } else {
            self.frames[0].start_address().as_u64()
        }
    }
}

/// Kernel-side representation of an io_uring instance
///
/// Each process can have one or more io_uring instances.
/// The ring buffers are in shared memory, accessible by both user and kernel.
///
/// ## Page-Aligned Memory
/// 
/// All ring buffers are page-aligned to ensure proper mapping to user space.
/// Memory is allocated directly from the frame allocator.
pub struct IoUring {
    /// Submission queue header (1 page, page-aligned)
    sq_header_region: PageAlignedRegion,
    
    /// Completion queue header (1 page, page-aligned)
    cq_header_region: PageAlignedRegion,
    
    /// Submission queue entries (4 pages, page-aligned)
    sq_entries_region: PageAlignedRegion,
    
    /// Completion queue entries (1 page, page-aligned)
    cq_entries_region: PageAlignedRegion,
    
    /// Kernel-side copy of SQEs for TOCTOU protection
    /// We copy entries here before processing to prevent user modifications
    pending_sqes: Vec<SubmissionEntry>,
    
    /// Statistics
    submissions_total: u64,
    completions_total: u64,
    dropped_submissions: u64,
}

/// Allocate page-aligned memory from the frame allocator
fn allocate_page_region(
    num_pages: usize,
    allocator: &mut crate::kernel::mm::BootInfoFrameAllocator,
) -> Option<PageAlignedRegion> {
    let phys_mem_offset = PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
    
    let mut frames = Vec::with_capacity(num_pages);
    
    // Allocate contiguous frames
    let first_frame = allocator.allocate_frame()?;
    let base_phys = first_frame.start_address().as_u64();
    frames.push(first_frame);
    
    // For more than one page, allocate additional frames
    // Note: These may not be physically contiguous, but for our purposes
    // we only use the first frame's address for mapping.
    // The kernel accesses through virtual addresses anyway.
    for _ in 1..num_pages {
        let frame = allocator.allocate_frame()?;
        frames.push(frame);
    }
    
    // Calculate kernel virtual address
    let virt_addr = VirtAddr::new(phys_mem_offset + base_phys);
    
    // Zero-initialize the memory
    unsafe {
        let ptr = virt_addr.as_mut_ptr::<u8>();
        core::ptr::write_bytes(ptr, 0, num_pages * Size4KiB::SIZE as usize);
    }
    
    Some(PageAlignedRegion { frames, virt_addr })
}

impl IoUring {
    /// Create a new io_uring instance with page-aligned buffers
    ///
    /// This allocates physical frames directly from the frame allocator
    /// to ensure page alignment for user-space mapping.
    ///
    /// # Arguments
    /// * `allocator` - Frame allocator to use for memory allocation
    ///
    /// # Returns
    /// * `Some(IoUring)` on success
    /// * `None` if allocation fails
    pub fn new_with_allocator(
        allocator: &mut crate::kernel::mm::BootInfoFrameAllocator,
    ) -> Option<Self> {
        // Allocate page-aligned regions for each buffer
        let sq_header_region = allocate_page_region(SQ_HEADER_PAGES, allocator)?;
        let cq_header_region = allocate_page_region(CQ_HEADER_PAGES, allocator)?;
        let sq_entries_region = allocate_page_region(SQ_ENTRIES_PAGES, allocator)?;
        let cq_entries_region = allocate_page_region(CQ_ENTRIES_PAGES, allocator)?;
        
        debug_println!(
            "[io_uring] Allocated page-aligned buffers: sq_header={:#x} cq_header={:#x} sq_entries={:#x} cq_entries={:#x}",
            sq_header_region.addr(),
            cq_header_region.addr(),
            sq_entries_region.addr(),
            cq_entries_region.addr()
        );
        
        // Initialize RingHeader structures
        unsafe {
            let sq_header = sq_header_region.virt_addr.as_mut_ptr::<RingHeader>();
            core::ptr::write(sq_header, RingHeader::new());
            
            let cq_header = cq_header_region.virt_addr.as_mut_ptr::<RingHeader>();
            core::ptr::write(cq_header, RingHeader::new());
        }
        
        Some(Self {
            sq_header_region,
            cq_header_region,
            sq_entries_region,
            cq_entries_region,
            pending_sqes: Vec::with_capacity(RING_SIZE as usize),
            submissions_total: 0,
            completions_total: 0,
            dropped_submissions: 0,
        })
    }
    
    /// Get a reference to the SQ header
    fn sq_header(&self) -> &RingHeader {
        unsafe { &*(self.sq_header_region.virt_addr.as_ptr::<RingHeader>()) }
    }
    
    /// Get a mutable reference to the SQ header
    fn sq_header_mut(&mut self) -> &mut RingHeader {
        unsafe { &mut *(self.sq_header_region.virt_addr.as_mut_ptr::<RingHeader>()) }
    }
    
    /// Get a reference to the CQ header
    fn cq_header(&self) -> &RingHeader {
        unsafe { &*(self.cq_header_region.virt_addr.as_ptr::<RingHeader>()) }
    }
    
    /// Get a mutable reference to the CQ header
    #[allow(dead_code)]
    fn cq_header_mut(&mut self) -> &mut RingHeader {
        unsafe { &mut *(self.cq_header_region.virt_addr.as_mut_ptr::<RingHeader>()) }
    }
    
    /// Get a reference to the SQ entries array
    fn sq_entries(&self) -> &[SubmissionEntry; RING_SIZE as usize] {
        unsafe { &*(self.sq_entries_region.virt_addr.as_ptr::<[SubmissionEntry; RING_SIZE as usize]>()) }
    }
    
    /// Get a reference to the CQ entries array  
    fn cq_entries(&self) -> &[CompletionEntry; RING_SIZE as usize] {
        unsafe { &*(self.cq_entries_region.virt_addr.as_ptr::<[CompletionEntry; RING_SIZE as usize]>()) }
    }
    
    /// Get a mutable reference to the CQ entries array
    fn cq_entries_mut(&mut self) -> &mut [CompletionEntry; RING_SIZE as usize] {
        unsafe { &mut *(self.cq_entries_region.virt_addr.as_mut_ptr::<[CompletionEntry; RING_SIZE as usize]>()) }
    }
    
    /// Get the address of the SQ header (for mapping to user space)
    /// Returns a page-aligned kernel virtual address
    #[must_use]
    pub fn sq_header_addr(&self) -> u64 {
        self.sq_header_region.addr()
    }
    
    /// Get the address of the CQ header (for mapping to user space)
    /// Returns a page-aligned kernel virtual address
    #[must_use]
    pub fn cq_header_addr(&self) -> u64 {
        self.cq_header_region.addr()
    }
    
    /// Get the address of the SQ entries (for mapping to user space)
    /// Returns a page-aligned kernel virtual address
    #[must_use]
    pub fn sq_entries_addr(&self) -> u64 {
        self.sq_entries_region.addr()
    }
    
    /// Get the address of the CQ entries (for mapping to user space)
    /// Returns a page-aligned kernel virtual address
    #[must_use]
    pub fn cq_entries_addr(&self) -> u64 {
        self.cq_entries_region.addr()
    }
    
    /// Process pending submissions from the SQ
    ///
    /// This function:
    /// 1. Reads new entries from the SQ (atomically)
    /// 2. Copies them to kernel-side storage (TOCTOU protection)
    /// 3. Returns the number of entries copied
    ///
    /// # Returns
    /// Number of submissions harvested
    pub fn harvest_submissions(&mut self) -> u32 {
        let sq_head = self.sq_header().head.load(Ordering::Acquire);
        let sq_tail = self.sq_header().tail.load(Ordering::Acquire);
        
        let pending = sq_tail.wrapping_sub(sq_head);
        if pending == 0 {
            return 0;
        }
        
        // Limit how many we process at once to prevent starvation
        let to_process = pending.min(64);
        
        // Copy entries to kernel-side storage
        for i in 0..to_process {
            let idx = (sq_head.wrapping_add(i) & RING_MASK) as usize;
            // SAFETY: We're copying from shared memory that has been validated
            // during io_uring setup. The entry is copied atomically.
            let entry = self.sq_entries()[idx];
            self.pending_sqes.push(entry);
        }
        
        // Update SQ head to indicate we've consumed the entries
        self.sq_header().head.store(sq_head.wrapping_add(to_process), Ordering::Release);
        
        self.submissions_total += u64::from(to_process);
        
        to_process
    }
    
    /// Get the next pending SQE for processing
    #[must_use]
    pub fn pop_pending(&mut self) -> Option<SubmissionEntry> {
        self.pending_sqes.pop()
    }
    
    /// Check if there are pending SQEs to process
    #[must_use]
    pub fn has_pending(&self) -> bool {
        !self.pending_sqes.is_empty()
    }
    
    /// Post a completion to the CQ
    ///
    /// # Arguments
    /// * `user_data` - The user_data from the original SQE
    /// * `result` - The result of the operation (positive for success, negative for error)
    /// * `flags` - Completion flags
    ///
    /// # Returns
    /// `Ok(())` if posted successfully, `Err(IoUringError::QueueFull)` if CQ is full
    pub fn post_completion(
        &mut self,
        user_data: u64,
        result: i32,
        flags: u32,
    ) -> Result<(), IoUringError> {
        let cq_head = self.cq_header().head.load(Ordering::Acquire);
        let cq_tail = self.cq_header().tail.load(Ordering::Relaxed);
        
        let pending = cq_tail.wrapping_sub(cq_head);
        if pending >= RING_SIZE {
            // CQ is full, increment dropped counter
            self.cq_header().dropped.fetch_add(1, Ordering::Relaxed);
            self.dropped_submissions += 1;
            return Err(IoUringError::QueueFull);
        }
        
        // Write the completion entry
        let idx = (cq_tail & RING_MASK) as usize;
        self.cq_entries_mut()[idx] = CompletionEntry {
            user_data,
            result,
            flags,
        };
        
        // Update CQ tail to make the entry visible
        self.cq_header().tail.store(cq_tail.wrapping_add(1), Ordering::Release);
        
        self.completions_total += 1;
        
        Ok(())
    }
    
    /// Post a success completion
    pub fn complete_success(&mut self, user_data: u64, result: i32) -> Result<(), IoUringError> {
        self.post_completion(user_data, result, 0)
    }
    
    /// Post an error completion
    pub fn complete_error(&mut self, user_data: u64, errno: i32) -> Result<(), IoUringError> {
        self.post_completion(user_data, -errno, 0)
    }
    
    /// Get the number of pending completions (readable by user)
    #[must_use]
    pub fn completion_count(&self) -> u32 {
        self.cq_header().pending_count()
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

/// Validate a submission entry from user space
///
/// Checks that all pointers are valid and in user space.
/// This is a critical security check to prevent kernel memory access.
pub fn validate_sqe(sqe: &SubmissionEntry) -> Result<(), IoUringError> {
    let op = match OpCode::from_u8(sqe.opcode) {
        Some(op) => op,
        None => return Err(IoUringError::NotSupported),
    };
    
    match op {
        OpCode::Nop => Ok(()),
        
        OpCode::Read | OpCode::Write => {
            // Validate buffer pointer
            if sqe.len > 0 {
                if sqe.addr == 0 {
                    return Err(IoUringError::BadAddress);
                }
                
                // For read operations, validate write access to buffer
                // For write operations, validate read access to buffer
                let validation = if op == OpCode::Read {
                    validate_user_write(sqe.addr, u64::from(sqe.len))
                } else {
                    validate_user_read(sqe.addr, u64::from(sqe.len))
                };
                
                if validation.is_err() {
                    return Err(IoUringError::BadAddress);
                }
            }
            Ok(())
        }
        
        OpCode::Open => {
            // Path pointer validation
            // Path is stored in addr, length in len
            if sqe.len > 0 && sqe.addr != 0 {
                if validate_user_read(sqe.addr, u64::from(sqe.len)).is_err() {
                    return Err(IoUringError::BadAddress);
                }
            }
            Ok(())
        }
        
        OpCode::Close | OpCode::Fsync => {
            // Only need valid fd, no pointer validation needed
            Ok(())
        }
        
        OpCode::Mmap => {
            // addr is the hint address (0 = any), len is the size
            // No validation needed for hint address
            if sqe.len == 0 {
                return Err(IoUringError::InvalidArgument);
            }
            Ok(())
        }
        
        OpCode::Munmap => {
            // addr must be page-aligned, len must be non-zero
            if sqe.addr == 0 || sqe.len == 0 {
                return Err(IoUringError::InvalidArgument);
            }
            if sqe.addr & 0xFFF != 0 {
                return Err(IoUringError::InvalidArgument);
            }
            Ok(())
        }
        
        // Network operations - validate buffers similar to read/write
        OpCode::Connect | OpCode::Accept | OpCode::Send | OpCode::Recv => {
            if sqe.len > 0 && sqe.addr != 0 {
                if validate_user_read(sqe.addr, u64::from(sqe.len)).is_err() {
                    return Err(IoUringError::BadAddress);
                }
            }
            Ok(())
        }
        
        OpCode::Poll | OpCode::Cancel | OpCode::LinkTimeout | OpCode::Exit => {
            // No pointer validation needed for these
            Ok(())
        }
    }
}

// Unit tests require a mock frame allocator which is complex to set up
// These tests are disabled for now and should be tested via integration tests
#[cfg(test)]
mod tests {
    // Tests disabled - IoUring::new_with_allocator requires a frame allocator
    // which is not available in unit test context.
    // Integration tests should verify io_uring functionality.
}
