// kernel/src/kernel/io_uring/ring.rs
//! Ring buffer implementation for io_uring
//!
//! This module provides the core ring buffer data structures that enable
//! lock-free communication between user space and kernel space.

use core::sync::atomic::Ordering;
use alloc::boxed::Box;
use alloc::vec::Vec;

use crate::abi::io_uring::{
    SubmissionEntry, CompletionEntry, RingHeader,
    OpCode, RING_SIZE, RING_MASK,
};
use crate::debug_println;
use crate::kernel::security::{validate_user_read, validate_user_write};
use crate::kernel::syscall::{EFAULT, EINVAL, ENOMEM, EAGAIN, SyscallResult};

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

/// Kernel-side representation of an io_uring instance
///
/// Each process can have one or more io_uring instances.
/// The ring buffers are in shared memory, accessible by both user and kernel.
pub struct IoUring {
    /// Submission queue header (shared memory)
    sq_header: Box<RingHeader>,
    
    /// Completion queue header (shared memory)  
    cq_header: Box<RingHeader>,
    
    /// Submission queue entries (shared memory)
    sq_entries: Box<[SubmissionEntry; RING_SIZE as usize]>,
    
    /// Completion queue entries (shared memory)
    cq_entries: Box<[CompletionEntry; RING_SIZE as usize]>,
    
    /// Kernel-side copy of SQEs for TOCTOU protection
    /// We copy entries here before processing to prevent user modifications
    pending_sqes: Vec<SubmissionEntry>,
    
    /// Statistics
    submissions_total: u64,
    completions_total: u64,
    dropped_submissions: u64,
}

impl IoUring {
    /// Create a new io_uring instance
    #[must_use]
    pub fn new() -> Self {
        // Initialize ring entries to default values
        let sq_entries: Box<[SubmissionEntry; RING_SIZE as usize]> = {
            let mut entries = Vec::with_capacity(RING_SIZE as usize);
            entries.resize_with(RING_SIZE as usize, SubmissionEntry::default);
            entries.into_boxed_slice().try_into().unwrap()
        };
        
        let cq_entries: Box<[CompletionEntry; RING_SIZE as usize]> = {
            let mut entries = Vec::with_capacity(RING_SIZE as usize);
            entries.resize_with(RING_SIZE as usize, CompletionEntry::default);
            entries.into_boxed_slice().try_into().unwrap()
        };
        
        Self {
            sq_header: Box::new(RingHeader::new()),
            cq_header: Box::new(RingHeader::new()),
            sq_entries,
            cq_entries,
            pending_sqes: Vec::with_capacity(RING_SIZE as usize),
            submissions_total: 0,
            completions_total: 0,
            dropped_submissions: 0,
        }
    }
    
    /// Get the address of the SQ header (for mapping to user space)
    #[must_use]
    pub fn sq_header_addr(&self) -> u64 {
        (&*self.sq_header) as *const RingHeader as u64
    }
    
    /// Get the address of the CQ header (for mapping to user space)
    #[must_use]
    pub fn cq_header_addr(&self) -> u64 {
        (&*self.cq_header) as *const RingHeader as u64
    }
    
    /// Get the address of the SQ entries (for mapping to user space)
    #[must_use]
    pub fn sq_entries_addr(&self) -> u64 {
        self.sq_entries.as_ptr() as u64
    }
    
    /// Get the address of the CQ entries (for mapping to user space)
    #[must_use]
    pub fn cq_entries_addr(&self) -> u64 {
        self.cq_entries.as_ptr() as u64
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
        let sq_head = self.sq_header.head.load(Ordering::Acquire);
        let sq_tail = self.sq_header.tail.load(Ordering::Acquire);
        
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
            let entry = self.sq_entries[idx];
            self.pending_sqes.push(entry);
        }
        
        // Update SQ head to indicate we've consumed the entries
        self.sq_header.head.store(sq_head.wrapping_add(to_process), Ordering::Release);
        
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
        let cq_head = self.cq_header.head.load(Ordering::Acquire);
        let cq_tail = self.cq_header.tail.load(Ordering::Relaxed);
        
        let pending = cq_tail.wrapping_sub(cq_head);
        if pending >= RING_SIZE {
            // CQ is full, increment dropped counter
            self.cq_header.dropped.fetch_add(1, Ordering::Relaxed);
            self.dropped_submissions += 1;
            return Err(IoUringError::QueueFull);
        }
        
        // Write the completion entry
        let idx = (cq_tail & RING_MASK) as usize;
        self.cq_entries[idx] = CompletionEntry {
            user_data,
            result,
            flags,
        };
        
        // Update CQ tail to make the entry visible
        self.cq_header.tail.store(cq_tail.wrapping_add(1), Ordering::Release);
        
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
        self.cq_header.pending_count()
    }
    
    /// Get statistics
    #[must_use]
    pub fn stats(&self) -> IoUringStats {
        IoUringStats {
            submissions_total: self.submissions_total,
            completions_total: self.completions_total,
            dropped_submissions: self.dropped_submissions,
            sq_pending: self.sq_header.pending_count(),
            cq_pending: self.cq_header.pending_count(),
        }
    }
}

impl Default for IoUring {
    fn default() -> Self {
        Self::new()
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

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_io_uring_creation() {
        let ring = IoUring::new();
        assert_eq!(ring.completion_count(), 0);
        assert!(!ring.has_pending());
    }
    
    #[test]
    fn test_completion_posting() {
        let mut ring = IoUring::new();
        
        // Post a completion
        ring.complete_success(42, 100).unwrap();
        assert_eq!(ring.completion_count(), 1);
        
        // Post more completions
        for i in 0..10 {
            ring.complete_success(i, i as i32).unwrap();
        }
        assert_eq!(ring.completion_count(), 11);
    }
}
