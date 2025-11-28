//! Asynchronous I/O API using `io_uring`
//!
//! This module provides high-level async I/O operations that use the
//! `io_uring`-style interface for efficient batched syscalls.
//!
//! # Example
//!
//! ```no_run
//! use libuser::async_io::{AsyncContext, AsyncOp};
//!
//! // Create async context (initializes io_uring)
//! let mut ctx = AsyncContext::new().unwrap();
//!
//! // Submit multiple operations at once
//! ctx.submit(AsyncOp::write(1, b"Hello"));
//! ctx.submit(AsyncOp::write(1, b" World!\n"));
//!
//! // Process all with single syscall
//! ctx.flush().unwrap();
//! ```

use core::sync::atomic::Ordering;
use crate::syscall::{self, SyscallResult, SyscallError};
use crate::io_uring::{
    SubmissionEntry, CompletionEntry, RingHeader,
    RING_SIZE, RING_MASK,
};

/// Maximum number of operations that can be batched
pub const MAX_BATCH_SIZE: usize = 64;

/// Result of an async operation
#[derive(Debug, Clone, Copy)]
pub struct AsyncResult {
    /// User-provided ID to correlate with submission
    pub user_data: u64,
    /// Result code (positive = success, negative = error)
    pub result: i32,
    /// Additional flags
    pub flags: u32,
}

impl AsyncResult {
    /// Check if the operation succeeded
    pub const fn is_ok(&self) -> bool {
        self.result >= 0
    }

    /// Get the result as usize (for read/write byte counts)
    pub fn as_usize(&self) -> Result<usize, SyscallError> {
        if self.result >= 0 {
            Ok(self.result as usize)
        } else {
            Err(SyscallError::new((-self.result) as i64))
        }
    }
}

/// An async operation to submit
#[derive(Debug)]
pub enum AsyncOp<'a> {
    /// No operation (useful for testing)
    Nop {
        /// User data for correlation
        user_data: u64,
    },
    /// Read from file descriptor
    Read {
        /// File descriptor to read from
        fd: i32,
        /// Buffer to read into
        buf: &'a mut [u8],
        /// Offset in file
        offset: u64,
        /// User data for correlation
        user_data: u64,
    },
    /// Write to file descriptor
    Write {
        /// File descriptor to write to
        fd: i32,
        /// Data to write
        buf: &'a [u8],
        /// Offset in file
        offset: u64,
        /// User data for correlation
        user_data: u64,
    },
    /// Close a file descriptor
    Close {
        /// File descriptor to close
        fd: i32,
        /// User data for correlation
        user_data: u64,
    },
    /// Map memory
    Mmap {
        /// Hint address
        addr: u64,
        /// Length in bytes
        len: u32,
        /// Protection flags
        prot: u32,
        /// Mapping flags
        flags: u32,
        /// User data for correlation
        user_data: u64,
    },
    /// Unmap memory
    Munmap {
        /// Address to unmap
        addr: u64,
        /// Length in bytes
        len: u32,
        /// User data for correlation
        user_data: u64,
    },
}

impl<'a> AsyncOp<'a> {
    /// Create a NOP operation
    pub const fn nop(user_data: u64) -> Self {
        Self::Nop { user_data }
    }

    /// Create a write operation
    pub const fn write(fd: i32, buf: &'a [u8], user_data: u64) -> Self {
        Self::Write { fd, buf, offset: 0, user_data }
    }

    /// Create a write operation with offset
    pub const fn write_at(fd: i32, buf: &'a [u8], offset: u64, user_data: u64) -> Self {
        Self::Write { fd, buf, offset, user_data }
    }

    /// Create a read operation
    pub fn read(fd: i32, buf: &'a mut [u8], user_data: u64) -> Self {
        Self::Read { fd, buf, offset: 0, user_data }
    }

    /// Create a read operation with offset
    pub fn read_at(fd: i32, buf: &'a mut [u8], offset: u64, user_data: u64) -> Self {
        Self::Read { fd, buf, offset, user_data }
    }

    /// Create a close operation
    pub const fn close(fd: i32, user_data: u64) -> Self {
        Self::Close { fd, user_data }
    }

    /// Create an mmap operation
    pub const fn mmap(addr: u64, len: u32, prot: u32, flags: u32, user_data: u64) -> Self {
        Self::Mmap { addr, len, prot, flags, user_data }
    }

    /// Create a munmap operation
    pub const fn munmap(addr: u64, len: u32, user_data: u64) -> Self {
        Self::Munmap { addr, len, user_data }
    }

    /// Convert to submission entry
    fn to_sqe(&self) -> SubmissionEntry {
        match *self {
            Self::Nop { user_data } => SubmissionEntry::nop(user_data),
            Self::Read { fd, ref buf, offset, user_data } => {
                SubmissionEntry::read(fd, buf.as_ptr() as *mut u8, buf.len() as u32, offset, user_data)
            }
            Self::Write { fd, buf, offset, user_data } => {
                SubmissionEntry::write(fd, buf.as_ptr(), buf.len() as u32, offset, user_data)
            }
            Self::Close { fd, user_data } => SubmissionEntry::close(fd, user_data),
            Self::Mmap { addr, len, prot, flags, user_data } => {
                SubmissionEntry::mmap(addr, len, prot, flags, user_data)
            }
            Self::Munmap { addr, len, user_data } => {
                SubmissionEntry::munmap(addr, len, user_data)
            }
        }
    }

    /// Get the user data
    pub const fn user_data(&self) -> u64 {
        match *self {
            Self::Nop { user_data } => user_data,
            Self::Read { user_data, .. } => user_data,
            Self::Write { user_data, .. } => user_data,
            Self::Close { user_data, .. } => user_data,
            Self::Mmap { user_data, .. } => user_data,
            Self::Munmap { user_data, .. } => user_data,
        }
    }
}

/// Fixed offsets for io_uring memory layout in user space
/// These must match the kernel's USER_IO_URING_BASE layout
mod async_io_offsets {
    /// Offset to sq_header from base
    pub const SQ_HEADER_OFFSET: usize = 0x0000;
    /// Offset to cq_header from base
    pub const CQ_HEADER_OFFSET: usize = 0x1000;
    /// Offset to sq_entries from base
    pub const SQ_ENTRIES_OFFSET: usize = 0x2000;
    /// Offset to cq_entries from base
    pub const CQ_ENTRIES_OFFSET: usize = 0x6000;
}

/// Asynchronous I/O context
///
/// Manages the io_uring submission and completion queues.
pub struct AsyncContext {
    /// Base address of the ring buffer region
    ring_base: *mut u8,
    /// Number of pending submissions
    pending: u32,
    /// Auto-incrementing user data counter
    next_user_data: u64,
}

impl AsyncContext {
    /// Create a new async context
    ///
    /// Initializes io_uring for the current process.
    pub fn new() -> SyscallResult<Self> {
        let result = syscall::io_uring_setup(RING_SIZE as u32)?;
        
        crate::println!("  [DEBUG] io_uring_setup returned {:#x}", result);
        
        Ok(Self {
            ring_base: result as *mut u8,
            pending: 0,
            next_user_data: 1,
        })
    }

    /// Get the next unique user data value
    pub fn alloc_user_data(&mut self) -> u64 {
        let ud = self.next_user_data;
        self.next_user_data = self.next_user_data.wrapping_add(1);
        ud
    }

    /// Get the submission queue header
    fn sq_header(&self) -> &RingHeader {
        use async_io_offsets::SQ_HEADER_OFFSET;
        unsafe { &*(self.ring_base.add(SQ_HEADER_OFFSET) as *const RingHeader) }
    }

    /// Get the submission queue entries
    fn sq_entries(&self) -> *mut SubmissionEntry {
        use async_io_offsets::SQ_ENTRIES_OFFSET;
        unsafe { self.ring_base.add(SQ_ENTRIES_OFFSET) as *mut SubmissionEntry }
    }

    /// Get the completion queue header
    fn cq_header(&self) -> &RingHeader {
        use async_io_offsets::CQ_HEADER_OFFSET;
        unsafe { &*(self.ring_base.add(CQ_HEADER_OFFSET) as *const RingHeader) }
    }

    /// Get the completion queue entries
    fn cq_entries(&self) -> *const CompletionEntry {
        use async_io_offsets::CQ_ENTRIES_OFFSET;
        unsafe { self.ring_base.add(CQ_ENTRIES_OFFSET) as *const CompletionEntry }
    }

    /// Check available space in submission queue
    pub fn available(&self) -> u32 {
        let header = self.sq_header();
        let head = header.head.load(Ordering::Acquire);
        let tail = header.tail.load(Ordering::Relaxed);
        RING_SIZE as u32 - (tail.wrapping_sub(head))
    }

    /// Submit an operation to the queue
    ///
    /// Returns the user_data that can be used to correlate completions.
    pub fn submit(&mut self, op: AsyncOp) -> Result<u64, ()> {
        crate::println!("  [DEBUG] submit: checking available...");
        if self.available() == 0 {
            return Err(());
        }

        crate::println!("  [DEBUG] submit: creating sqe...");
        let sqe = op.to_sqe();
        let user_data = sqe.user_data;
        
        crate::println!("  [DEBUG] submit: getting sq_header at {:#x}...", self.ring_base as u64);
        let header = self.sq_header();
        crate::println!("  [DEBUG] submit: loading tail...");
        let tail = header.tail.load(Ordering::Relaxed);
        let index = (tail as usize) & RING_MASK;
        
        crate::println!("  [DEBUG] submit: writing sqe to index {}...", index);
        unsafe {
            *self.sq_entries().add(index) = sqe;
        }
        
        core::sync::atomic::fence(Ordering::Release);
        header.tail.store(tail.wrapping_add(1), Ordering::Release);
        
        self.pending += 1;
        crate::println!("  [DEBUG] submit: done");
        Ok(user_data)
    }

    /// Submit multiple operations at once
    pub fn submit_batch(&mut self, ops: &[AsyncOp]) -> Result<u32, ()> {
        if self.available() < ops.len() as u32 {
            return Err(());
        }

        let header = self.sq_header();
        let tail = header.tail.load(Ordering::Relaxed);
        
        for (i, op) in ops.iter().enumerate() {
            let sqe = op.to_sqe();
            let index = ((tail as usize) + i) & RING_MASK;
            unsafe {
                *self.sq_entries().add(index) = sqe;
            }
        }
        
        core::sync::atomic::fence(Ordering::Release);
        header.tail.store(tail.wrapping_add(ops.len() as u32), Ordering::Release);
        
        self.pending += ops.len() as u32;
        Ok(ops.len() as u32)
    }

    /// Flush pending submissions and wait for completions
    ///
    /// Processes all pending submissions with a single syscall.
    pub fn flush(&mut self) -> SyscallResult<u32> {
        if self.pending == 0 {
            return Ok(0);
        }

        let to_submit = self.pending;
        let result = syscall::io_uring_enter(0, to_submit, to_submit, 0)?;
        self.pending = 0;
        Ok(result)
    }

    /// Get the number of available completions
    pub fn completions_available(&self) -> u32 {
        let header = self.cq_header();
        let head = header.head.load(Ordering::Relaxed);
        let tail = header.tail.load(Ordering::Acquire);
        tail.wrapping_sub(head)
    }

    /// Get the next completion result
    pub fn get_completion(&self) -> Option<AsyncResult> {
        let header = self.cq_header();
        
        let head = header.head.load(Ordering::Relaxed);
        let tail = header.tail.load(Ordering::Acquire);
        
        if head == tail {
            return None;
        }
        
        let index = (head as usize) & RING_MASK;
        let cqe = unsafe { *self.cq_entries().add(index) };
        
        core::sync::atomic::fence(Ordering::Acquire);
        header.head.store(head.wrapping_add(1), Ordering::Release);
        
        Some(AsyncResult {
            user_data: cqe.user_data,
            result: cqe.result,
            flags: cqe.flags,
        })
    }

    /// Drain all available completions
    pub fn drain_completions<F>(&self, mut callback: F) 
    where
        F: FnMut(AsyncResult),
    {
        while let Some(result) = self.get_completion() {
            callback(result);
        }
    }

    /// Get pending submission count
    pub const fn pending(&self) -> u32 {
        self.pending
    }
}

// ============================================================================
// Convenience Functions (Synchronous wrappers using io_uring)
// ============================================================================

/// Write to a file descriptor using io_uring (single operation)
///
/// This is a synchronous wrapper that uses io_uring for a single write.
/// For multiple writes, use `AsyncContext` directly for better performance.
pub fn write(fd: i32, buf: &[u8]) -> SyscallResult<usize> {
    let mut ctx = AsyncContext::new()?;
    let user_data = ctx.alloc_user_data();
    
    ctx.submit(AsyncOp::write(fd, buf, user_data)).map_err(|_| SyscallError::new(12))?; // ENOMEM
    ctx.flush()?;
    
    if let Some(result) = ctx.get_completion() {
        result.as_usize()
    } else {
        Err(SyscallError::new(5)) // EIO
    }
}

/// Read from a file descriptor using io_uring (single operation)
pub fn read(fd: i32, buf: &mut [u8]) -> SyscallResult<usize> {
    let mut ctx = AsyncContext::new()?;
    let user_data = ctx.alloc_user_data();
    
    ctx.submit(AsyncOp::read(fd, buf, user_data)).map_err(|_| SyscallError::new(12))?;
    ctx.flush()?;
    
    if let Some(result) = ctx.get_completion() {
        result.as_usize()
    } else {
        Err(SyscallError::new(5))
    }
}

/// Allocate memory using io_uring mmap
pub fn alloc(size: u64) -> SyscallResult<u64> {
    use crate::mem::{PROT_READ, PROT_WRITE, MAP_PRIVATE, MAP_ANONYMOUS};
    
    let mut ctx = AsyncContext::new()?;
    let user_data = ctx.alloc_user_data();
    
    ctx.submit(AsyncOp::mmap(
        0, 
        size as u32, 
        (PROT_READ | PROT_WRITE) as u32, 
        (MAP_PRIVATE | MAP_ANONYMOUS) as u32,
        user_data
    )).map_err(|_| SyscallError::new(12))?;
    
    ctx.flush()?;
    
    if let Some(result) = ctx.get_completion() {
        if result.is_ok() {
            Ok(result.result as u64)
        } else {
            Err(SyscallError::new((-result.result) as i64))
        }
    } else {
        Err(SyscallError::new(5))
    }
}

/// Deallocate memory using io_uring munmap
pub fn dealloc(addr: u64, size: u64) -> SyscallResult<()> {
    let mut ctx = AsyncContext::new()?;
    let user_data = ctx.alloc_user_data();
    
    ctx.submit(AsyncOp::munmap(addr, size as u32, user_data))
        .map_err(|_| SyscallError::new(12))?;
    
    ctx.flush()?;
    
    if let Some(result) = ctx.get_completion() {
        if result.is_ok() {
            Ok(())
        } else {
            Err(SyscallError::new((-result.result) as i64))
        }
    } else {
        Err(SyscallError::new(5))
    }
}

// ============================================================================
// Batch Operations
// ============================================================================

/// Write multiple buffers with a single syscall
///
/// # Arguments
/// * `fd` - File descriptor to write to
/// * `bufs` - Array of buffers to write
///
/// # Returns
/// Array of results (bytes written or error)
pub fn write_batch(fd: i32, bufs: &[&[u8]]) -> SyscallResult<[i32; MAX_BATCH_SIZE]> {
    let mut results = [0i32; MAX_BATCH_SIZE];
    
    if bufs.is_empty() {
        return Ok(results);
    }
    
    let mut ctx = AsyncContext::new()?;
    
    // Submit all writes
    for (i, buf) in bufs.iter().enumerate().take(MAX_BATCH_SIZE) {
        ctx.submit(AsyncOp::write(fd, buf, i as u64))
            .map_err(|_| SyscallError::new(12))?;
    }
    
    // Flush and wait for completions
    ctx.flush()?;
    
    // Collect results
    while let Some(result) = ctx.get_completion() {
        let idx = result.user_data as usize;
        if idx < MAX_BATCH_SIZE {
            results[idx] = result.result;
        }
    }
    
    Ok(results)
}
