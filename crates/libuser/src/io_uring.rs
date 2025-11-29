#![allow(
    clippy::missing_const_for_fn,
    clippy::cast_ptr_alignment,
    clippy::must_use_candidate,
    clippy::mut_from_ref,
    clippy::ptr_as_ptr,
    clippy::result_unit_err,
    clippy::cast_possible_truncation
)]
//! io_uring user-space interface
//!
//! This module provides the user-space API for `io_uring`-style async I/O.
//! It mirrors the kernel's ABI definitions to ensure compatibility.

use core::sync::atomic::{AtomicU32, AtomicBool, Ordering};

/// Size of the ring buffer (power of 2 for efficient modulo)
pub const RING_SIZE: usize = 256;
/// Mask for ring index calculations
pub const RING_MASK: usize = RING_SIZE - 1;

/// Operation codes for `io_uring` submissions
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpCode {
    /// No operation (for testing)
    Nop = 0,
    /// Read from file descriptor
    Read = 1,
    /// Write to file descriptor
    Write = 2,
    /// Open a file
    Open = 3,
    /// Close a file descriptor
    Close = 4,
    /// Sync file to disk
    Fsync = 5,
    /// Poll for events
    Poll = 6,
    /// Cancel a pending operation
    Cancel = 7,
    /// Memory map
    Mmap = 8,
    /// Memory unmap
    Munmap = 9,
    /// Get process ID
    GetPid = 10,
    /// Timeout operation
    Timeout = 11,
}

impl OpCode {
    /// Convert from u8 to OpCode
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(OpCode::Nop),
            1 => Some(OpCode::Read),
            2 => Some(OpCode::Write),
            3 => Some(OpCode::Open),
            4 => Some(OpCode::Close),
            5 => Some(OpCode::Fsync),
            6 => Some(OpCode::Poll),
            7 => Some(OpCode::Cancel),
            8 => Some(OpCode::Mmap),
            9 => Some(OpCode::Munmap),
            10 => Some(OpCode::GetPid),
            11 => Some(OpCode::Timeout),
            _ => None,
        }
    }
}

/// Flags for `io_uring` operations
#[repr(transparent)]
#[derive(Debug, Clone, Copy, Default)]
pub struct IoUringFlags(pub u32);

impl IoUringFlags {
    /// No flags
    pub const NONE: Self = Self(0);
    /// Operation should be linked to next
    pub const LINK: Self = Self(1 << 0);
    /// Drain all pending operations first
    pub const DRAIN: Self = Self(1 << 1);
    /// Don't generate completion event
    pub const NO_COMPLETION: Self = Self(1 << 2);
    /// Operation is fixed buffer
    pub const FIXED_BUFFER: Self = Self(1 << 3);
    /// Operation is async (don't wait for completion)
    pub const ASYNC: Self = Self(1 << 4);
}

/// Submission Queue Entry (SQE)
///
/// This is the structure used to submit operations to the kernel.
/// Each entry is 64 bytes, cache-line aligned for performance.
#[repr(C, align(64))]
#[derive(Debug, Clone, Copy)]
pub struct SubmissionEntry {
    /// Operation code (`OpCode`)
    pub opcode: u8,
    /// Operation flags
    pub flags: u8,
    /// I/O priority
    pub ioprio: u16,
    /// File descriptor for I/O operations
    pub fd: i32,
    /// Offset for read/write operations
    pub offset: u64,
    /// Buffer address for read/write operations
    pub addr: u64,
    /// Length of the buffer
    pub len: u32,
    /// Operation-specific flags (e.g., `O_RDONLY` for open)
    pub op_flags: u32,
    /// User data (passed back in completion)
    pub user_data: u64,
    /// Index into fixed buffer array
    pub buf_index: u16,
    /// Personality ID for credentials
    pub personality: u16,
    /// Splice destination offset
    pub splice_fd_in: i32,
    /// Reserved for future use
    pub _reserved: [u64; 2],
}

impl SubmissionEntry {
    /// Create a new zeroed submission entry
    pub const fn new() -> Self {
        Self {
            opcode: 0,
            flags: 0,
            ioprio: 0,
            fd: 0,
            offset: 0,
            addr: 0,
            len: 0,
            op_flags: 0,
            user_data: 0,
            buf_index: 0,
            personality: 0,
            splice_fd_in: 0,
            _reserved: [0; 2],
        }
    }

    /// Create a NOP entry
    pub fn nop(user_data: u64) -> Self {
        Self {
            opcode: OpCode::Nop as u8,
            user_data,
            ..Self::new()
        }
    }

    /// Create a read entry
    pub fn read(fd: i32, buf: *mut u8, len: u32, offset: u64, user_data: u64) -> Self {
        Self {
            opcode: OpCode::Read as u8,
            fd,
            addr: buf as u64,
            len,
            offset,
            user_data,
            ..Self::new()
        }
    }

    /// Create a write entry
    pub fn write(fd: i32, buf: *const u8, len: u32, offset: u64, user_data: u64) -> Self {
        Self {
            opcode: OpCode::Write as u8,
            fd,
            addr: buf as u64,
            len,
            offset,
            user_data,
            ..Self::new()
        }
    }

    /// Create a close entry
    pub fn close(fd: i32, user_data: u64) -> Self {
        Self {
            opcode: OpCode::Close as u8,
            fd,
            user_data,
            ..Self::new()
        }
    }

    /// Create an mmap entry
    pub fn mmap(addr: u64, len: u32, prot: u32, flags: u32, user_data: u64) -> Self {
        Self {
            opcode: OpCode::Mmap as u8,
            addr,
            len,
            op_flags: prot | (flags << 16),
            user_data,
            ..Self::new()
        }
    }

    /// Create a munmap entry
    pub fn munmap(addr: u64, len: u32, user_data: u64) -> Self {
        Self {
            opcode: OpCode::Munmap as u8,
            addr,
            len,
            user_data,
            ..Self::new()
        }
    }

    /// Create a getpid entry
    pub fn getpid(user_data: u64) -> Self {
        Self {
            opcode: OpCode::GetPid as u8,
            user_data,
            ..Self::new()
        }
    }
}

impl Default for SubmissionEntry {
    fn default() -> Self {
        Self::new()
    }
}

/// Completion Queue Entry (CQE)
///
/// This is the structure returned by the kernel when an operation completes.
/// Each entry is 16 bytes.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CompletionEntry {
    /// User data from the submission (for correlation)
    pub user_data: u64,
    /// Result of the operation (like syscall return value)
    pub result: i32,
    /// Completion flags
    pub flags: u32,
}

impl CompletionEntry {
    /// Create a new completion entry
    pub const fn new(user_data: u64, result: i32, flags: u32) -> Self {
        Self {
            user_data,
            result,
            flags,
        }
    }

    /// Check if the operation was successful
    pub fn is_success(&self) -> bool {
        self.result >= 0
    }

    /// Check if more completions are available
    pub fn has_more(&self) -> bool {
        self.flags & 1 != 0
    }
}

/// Ring buffer header (shared between user and kernel)
///
/// This structure is placed at the beginning of each ring buffer.
/// Uses atomic operations for lock-free synchronization.
#[repr(C)]
pub struct RingHeader {
    /// Head index (consumer reads from here)
    pub head: AtomicU32,
    /// Tail index (producer writes here)
    pub tail: AtomicU32,
    /// Ring size mask (RING_SIZE - 1)
    pub ring_mask: u32,
    /// Number of entries in the ring
    pub ring_entries: u32,
    /// Flags for the ring
    pub flags: AtomicU32,
    /// Reserved for cache alignment
    pub _reserved: [u32; 3],
}

impl RingHeader {
    /// Create a new ring header
    pub const fn new() -> Self {
        Self {
            head: AtomicU32::new(0),
            tail: AtomicU32::new(0),
            ring_mask: RING_MASK as u32,
            ring_entries: RING_SIZE as u32,
            flags: AtomicU32::new(0),
            _reserved: [0; 3],
        }
    }

    /// Get the number of available entries (for producer)
    pub fn available(&self) -> u32 {
        let head = self.head.load(Ordering::Acquire);
        let tail = self.tail.load(Ordering::Relaxed);
        RING_SIZE as u32 - (tail.wrapping_sub(head))
    }

    /// Get the number of pending entries (for consumer)
    pub fn pending(&self) -> u32 {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);
        tail.wrapping_sub(head)
    }
}

/// Parameters for `io_uring_setup` syscall
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct IoUringParams {
    /// Number of submission queue entries
    pub sq_entries: u32,
    /// Number of completion queue entries
    pub cq_entries: u32,
    /// Setup flags
    pub flags: u32,
    /// Thread CPU affinity
    pub sq_thread_cpu: u32,
    /// Thread idle timeout
    pub sq_thread_idle: u32,
    /// Features supported
    pub features: u32,
    /// Submission queue offset
    pub sq_off: u64,
    /// Completion queue offset
    pub cq_off: u64,
}

impl IoUringParams {
    /// Create default parameters
    pub const fn new() -> Self {
        Self {
            sq_entries: RING_SIZE as u32,
            cq_entries: RING_SIZE as u32,
            flags: 0,
            sq_thread_cpu: 0,
            sq_thread_idle: 0,
            features: 0,
            sq_off: 0,
            cq_off: 0,
        }
    }
}

impl Default for IoUringParams {
    fn default() -> Self {
        Self::new()
    }
}

/// High-level `io_uring` instance
///
/// Provides a safe interface for submitting operations and retrieving completions.
pub struct IoUring {
    /// Submission queue header address
    sq_header_ptr: *mut u8,
    /// Completion queue header address  
    cq_header_ptr: *mut u8,
    /// Submission queue entries base address
    sq_entries_ptr: *mut u8,
    /// Completion queue entries base address
    cq_entries_ptr: *mut u8,
    /// Doorbell page address for zero-syscall signaling
    doorbell_ptr: *mut u8,
}

/// Fixed offsets for io_uring memory layout in user space
/// These must match the kernel's USER_IO_URING_BASE layout
mod io_uring_offsets {
    /// Offset to sq_header from base
    pub const SQ_HEADER_OFFSET: usize = 0x0000;
    /// Offset to cq_header from base
    pub const CQ_HEADER_OFFSET: usize = 0x1000;
    /// Offset to sq_entries from base
    pub const SQ_ENTRIES_OFFSET: usize = 0x2000;
    /// Offset to cq_entries from base
    pub const CQ_ENTRIES_OFFSET: usize = 0x6000;
    /// Offset to doorbell page from base
    pub const DOORBELL_OFFSET: usize = 0x7000;
}

/// Doorbell layout (shared with kernel). Must exactly match kernel's Doorbell struct
#[repr(C, align(4096))]
pub struct DoorbellLayout {
    pub ring: AtomicU32,
    pub needs_wakeup: AtomicBool,
    pub cq_ready: AtomicBool,
    pub sqpoll_running: AtomicBool,
    _pad: [u8; 4096 - 10],
}

impl IoUring {
    /// Setup a new `io_uring` instance
    ///
    /// # Safety
    ///
    /// The returned ring must be properly initialized before use.
    pub unsafe fn setup(entries: u32, params: &mut IoUringParams) -> Result<Self, i32> {
        use crate::syscall::io_uring_setup;
        use io_uring_offsets::{SQ_HEADER_OFFSET, CQ_HEADER_OFFSET, SQ_ENTRIES_OFFSET, CQ_ENTRIES_OFFSET, DOORBELL_OFFSET};
        
        let result = match io_uring_setup(entries) {
            Ok(addr) => addr,
            Err(e) => return Err(e.code() as i32),
        };
        
        // The kernel returns the base address of the ring region (USER_IO_URING_BASE)
        let base = result as *mut u8;
        
        // Calculate addresses based on fixed offsets
        // SAFETY: we are computing addresses within the mapped io_uring region
        let sq_header_ptr = unsafe { base.add(SQ_HEADER_OFFSET) };
        let cq_header_ptr = unsafe { base.add(CQ_HEADER_OFFSET) };
        let sq_entries_ptr = unsafe { base.add(SQ_ENTRIES_OFFSET) };
        let cq_entries_ptr = unsafe { base.add(CQ_ENTRIES_OFFSET) };
        let doorbell_ptr = unsafe { base.add(DOORBELL_OFFSET) };
        
        // Update params with actual values (for compatibility)
        params.sq_off = SQ_HEADER_OFFSET as u64;
        params.cq_off = CQ_HEADER_OFFSET as u64;
        
        Ok(Self {
            sq_header_ptr,
            cq_header_ptr,
            sq_entries_ptr,
            cq_entries_ptr,
            doorbell_ptr,
        })
    }

    // DoorbellLayout is declared at module scope

    /// Get a reference to the doorbell layout
    #[allow(clippy::cast_ptr_alignment)]
    #[allow(clippy::missing_const_for_fn)]
    #[inline]
    fn doorbell(&self) -> &DoorbellLayout {
        // SAFETY: doorbell_ptr is a page-aligned address provided by the kernel
        unsafe { &*(self.doorbell_ptr as *const DoorbellLayout) }
    }

    /// Ring the doorbell to notify kernel (no syscall)
    pub fn ring_doorbell(&self) {
        let db = self.doorbell();
        db.ring.fetch_add(1, Ordering::Release);
    }

    /// Check if CQ is ready via doorbell
    pub fn check_cq_ready(&self) -> bool {
        let db = self.doorbell();
        db.cq_ready.load(Ordering::Acquire)
    }

    /// Clear doorbell CQ ready flag
    pub fn clear_cq_ready(&self) {
        let db = self.doorbell();
        db.cq_ready.store(false, Ordering::Release);
    }

    /// Get the submission queue header
    pub fn sq_header(&self) -> &RingHeader {
        unsafe { &*(self.sq_header_ptr as *const RingHeader) }
    }

    /// Get the submission queue entry array
    pub fn sq_entries(&self) -> &mut [SubmissionEntry] {
        unsafe { core::slice::from_raw_parts_mut(self.sq_entries_ptr as *mut SubmissionEntry, RING_SIZE) }
    }

    /// Get the completion queue header
    pub fn cq_header(&self) -> &RingHeader {
        unsafe { &*(self.cq_header_ptr as *const RingHeader) }
    }

    /// Get the completion queue entry array
    pub fn cq_entries(&self) -> &[CompletionEntry] {
        unsafe { core::slice::from_raw_parts(self.cq_entries_ptr as *const CompletionEntry, RING_SIZE) }
    }

    /// Submit a single entry
    ///
    /// Returns the index where the entry was placed.
    pub fn submit(&self, sqe: SubmissionEntry) -> Result<u32, ()> {
        let header = self.sq_header();
        
        if header.available() == 0 {
            return Err(());
        }
        
        let tail = header.tail.load(Ordering::Relaxed);
        let index = (tail as usize) & RING_MASK;
        
        self.sq_entries()[index] = sqe;
        
        // Ensure the entry is visible before updating tail
        core::sync::atomic::fence(Ordering::Release);
        header.tail.store(tail.wrapping_add(1), Ordering::Release);
        
        Ok(index as u32)
    }

    /// Submit multiple entries
    pub fn submit_batch(&self, entries: &[SubmissionEntry]) -> Result<u32, ()> {
        let header = self.sq_header();
        let available = header.available() as usize;
        
        if available < entries.len() {
            return Err(());
        }
        
        let tail = header.tail.load(Ordering::Relaxed);
        
        for (i, sqe) in entries.iter().enumerate() {
            let index = ((tail as usize) + i) & RING_MASK;
            self.sq_entries()[index] = *sqe;
        }
        
        core::sync::atomic::fence(Ordering::Release);
        header.tail.store(tail.wrapping_add(entries.len() as u32), Ordering::Release);
        
        Ok(entries.len() as u32)
    }

    /// Wait for completions
    ///
    /// Returns the number of completions processed.
    pub fn wait(&self, min_complete: u32) -> Result<u32, i32> {
        use crate::syscall::io_uring_enter;
        
        match io_uring_enter(0, 0, min_complete, 0) {
            Ok(n) => Ok(n),
            Err(e) => Err(e.code() as i32),
        }
    }

    /// Get next completion entry
    pub fn get_completion(&self) -> Option<CompletionEntry> {
        let header = self.cq_header();
        
        if header.pending() == 0 {
            return None;
        }
        
        let head = header.head.load(Ordering::Relaxed);
        let index = (head as usize) & RING_MASK;
        
        let cqe = self.cq_entries()[index];
        
        // Advance head after reading
        core::sync::atomic::fence(Ordering::Acquire);
        header.head.store(head.wrapping_add(1), Ordering::Release);
        
        Some(cqe)
    }

    /// Get all available completions
    pub fn drain_completions(&self) -> impl Iterator<Item = CompletionEntry> + '_ {
        core::iter::from_fn(move || self.get_completion())
    }
}

// Compile-time size assertions
const _: () = {
    assert!(core::mem::size_of::<SubmissionEntry>() == 64);
    assert!(core::mem::size_of::<CompletionEntry>() == 16);
    assert!(core::mem::size_of::<RingHeader>() == 32);
    assert!(RING_SIZE.is_power_of_two());
};
