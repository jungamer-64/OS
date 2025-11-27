// kernel/src/abi/io_uring.rs
//! io_uring-style ABI definitions
//!
//! Shared data structures for asynchronous I/O between user space and kernel space.
//!
//! # Memory Layout
//!
//! All structures are `#[repr(C)]` for stable ABI across Rust versions
//! and potential future C interoperability.
//!
//! # Safety Invariants
//!
//! - All atomic operations use appropriate memory ordering
//! - Ring indices are always masked with `RING_MASK` before use
//! - User space can only modify SQ tail and CQ head
//! - Kernel can only modify SQ head and CQ tail

use core::sync::atomic::{AtomicU32, Ordering};

/// Ring buffer size (must be power of 2)
/// 
/// 256 entries provides good batching while keeping memory usage reasonable.
/// Each SQ entry is 64 bytes, CQ entry is 16 bytes.
/// Total: 256 * (64 + 16) = 20KB per ring pair
pub const RING_SIZE: u32 = 256;

/// Ring mask for efficient modulo operation
pub const RING_MASK: u32 = RING_SIZE - 1;

/// I/O operation codes
///
/// These correspond to the operations that can be submitted via the ring buffer.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpCode {
    /// No operation (for padding/alignment)
    Nop = 0,
    /// Read from file descriptor
    Read = 1,
    /// Write to file descriptor  
    Write = 2,
    /// Open file
    Open = 3,
    /// Close file descriptor
    Close = 4,
    /// Synchronous file data
    Fsync = 5,
    /// Poll for events
    Poll = 6,
    /// Cancel a pending request
    Cancel = 7,
    /// Link timeout to operation
    LinkTimeout = 8,
    /// Connect socket
    Connect = 9,
    /// Accept connection
    Accept = 10,
    /// Send data
    Send = 11,
    /// Receive data
    Recv = 12,
    /// Memory map
    Mmap = 13,
    /// Memory unmap
    Munmap = 14,
    /// Exit process (immediate, doesn't use ring)
    Exit = 255,
}

impl OpCode {
    /// Convert from raw u8 value
    #[must_use]
    pub const fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Nop),
            1 => Some(Self::Read),
            2 => Some(Self::Write),
            3 => Some(Self::Open),
            4 => Some(Self::Close),
            5 => Some(Self::Fsync),
            6 => Some(Self::Poll),
            7 => Some(Self::Cancel),
            8 => Some(Self::LinkTimeout),
            9 => Some(Self::Connect),
            10 => Some(Self::Accept),
            11 => Some(Self::Send),
            12 => Some(Self::Recv),
            13 => Some(Self::Mmap),
            14 => Some(Self::Munmap),
            255 => Some(Self::Exit),
            _ => None,
        }
    }
}

/// Submission entry flags
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IoUringFlags(pub u8);

impl IoUringFlags {
    /// No special flags
    pub const NONE: Self = Self(0);
    
    /// Link this request with the next one
    /// If this request fails, the linked request is cancelled
    pub const LINK: Self = Self(1 << 0);
    
    /// Hardlink - always submit next request regardless of this one's result
    pub const HARDLINK: Self = Self(1 << 1);
    
    /// Force async execution (don't try synchronous fast path)
    pub const ASYNC: Self = Self(1 << 2);
    
    /// Buffer is registered (zero-copy optimization)
    pub const FIXED_BUFFER: Self = Self(1 << 3);
    
    /// File descriptor is registered
    pub const FIXED_FILE: Self = Self(1 << 4);
    
    /// Drain - wait for all prior requests to complete
    pub const DRAIN: Self = Self(1 << 5);
}

impl core::ops::BitOr for IoUringFlags {
    type Output = Self;
    
    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl core::ops::BitAnd for IoUringFlags {
    type Output = Self;
    
    fn bitand(self, rhs: Self) -> Self::Output {
        Self(self.0 & rhs.0)
    }
}

/// Submission Queue Entry (SQE)
///
/// This structure represents a single I/O request submitted by user space.
///
/// # Size
/// 64 bytes, cache-line aligned for optimal performance
///
/// # Memory Ordering
/// User writes all fields, then updates SQ tail with Release ordering.
/// Kernel reads SQ head with Acquire ordering before reading entries.
#[repr(C, align(64))]
#[derive(Debug, Clone, Copy)]
pub struct SubmissionEntry {
    /// Operation code
    pub opcode: u8,
    
    /// Operation flags (IoUringFlags)
    pub flags: u8,
    
    /// I/O priority (lower = higher priority)
    pub ioprio: u16,
    
    /// Target file descriptor
    pub fd: i32,
    
    /// Offset in file (for read/write)
    /// Also used as: address for mmap
    pub off: u64,
    
    /// Buffer address in user space
    /// Must be validated by kernel before use
    pub addr: u64,
    
    /// Buffer length
    pub len: u32,
    
    /// Operation-specific flags
    /// - For open: O_RDONLY, O_WRONLY, etc.
    /// - For poll: POLLIN, POLLOUT, etc.
    pub op_flags: u32,
    
    /// User data - passed back unchanged in completion
    /// Allows user space to identify which request completed
    pub user_data: u64,
    
    /// Buffer index (for registered buffers)
    pub buf_index: u16,
    
    /// Personality (credentials) to use
    pub personality: u16,
    
    /// Splice fd in (for splice operations)
    pub splice_fd_in: i32,
    
    /// Reserved for future use (padding to 64 bytes)
    pub _reserved: [u64; 2],
}

impl SubmissionEntry {
    /// Create a NOP entry (useful for padding)
    #[must_use]
    pub const fn nop(user_data: u64) -> Self {
        Self {
            opcode: OpCode::Nop as u8,
            flags: 0,
            ioprio: 0,
            fd: -1,
            off: 0,
            addr: 0,
            len: 0,
            op_flags: 0,
            user_data,
            buf_index: 0,
            personality: 0,
            splice_fd_in: 0,
            _reserved: [0; 2],
        }
    }
    
    /// Create a read entry
    #[must_use]
    pub const fn read(fd: i32, buf: u64, len: u32, offset: u64, user_data: u64) -> Self {
        Self {
            opcode: OpCode::Read as u8,
            flags: 0,
            ioprio: 0,
            fd,
            off: offset,
            addr: buf,
            len,
            op_flags: 0,
            user_data,
            buf_index: 0,
            personality: 0,
            splice_fd_in: 0,
            _reserved: [0; 2],
        }
    }
    
    /// Create a write entry
    #[must_use]
    pub const fn write(fd: i32, buf: u64, len: u32, offset: u64, user_data: u64) -> Self {
        Self {
            opcode: OpCode::Write as u8,
            flags: 0,
            ioprio: 0,
            fd,
            off: offset,
            addr: buf,
            len,
            op_flags: 0,
            user_data,
            buf_index: 0,
            personality: 0,
            splice_fd_in: 0,
            _reserved: [0; 2],
        }
    }
    
    /// Get the operation code
    #[must_use]
    pub const fn op(&self) -> Option<OpCode> {
        OpCode::from_u8(self.opcode)
    }
}

impl Default for SubmissionEntry {
    fn default() -> Self {
        Self::nop(0)
    }
}

/// Completion Queue Entry (CQE)
///
/// This structure represents the result of a completed I/O request.
///
/// # Size
/// 16 bytes, compact for cache efficiency
///
/// # Memory Ordering
/// Kernel writes all fields, then updates CQ tail with Release ordering.
/// User reads CQ head with Acquire ordering before reading entries.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CompletionEntry {
    /// User data from the corresponding SQE
    /// Allows user space to correlate completions with requests
    pub user_data: u64,
    
    /// Result of the operation
    /// - >= 0: Success (typically bytes transferred)
    /// - < 0: Negated errno value
    pub result: i32,
    
    /// Completion flags
    /// - Bit 0: IORING_CQE_F_BUFFER - buffer index is set
    /// - Bit 1: IORING_CQE_F_MORE - more completions coming
    pub flags: u32,
}

impl CompletionEntry {
    /// Create a new completion entry
    #[must_use]
    pub const fn new(user_data: u64, result: i32, flags: u32) -> Self {
        Self {
            user_data,
            result,
            flags,
        }
    }
    
    /// Create a success completion
    #[must_use]
    pub const fn success(user_data: u64, result: i32) -> Self {
        Self::new(user_data, result, 0)
    }
    
    /// Create an error completion
    #[must_use]
    pub const fn error(user_data: u64, errno: i32) -> Self {
        Self::new(user_data, -errno, 0)
    }
    
    /// Check if this is an error result
    #[must_use]
    pub const fn is_error(&self) -> bool {
        self.result < 0
    }
    
    /// Get the errno if this is an error
    #[must_use]
    pub const fn errno(&self) -> Option<i32> {
        if self.result < 0 {
            Some(-self.result)
        } else {
            None
        }
    }
}

impl Default for CompletionEntry {
    fn default() -> Self {
        Self::new(0, 0, 0)
    }
}

/// Ring buffer header
///
/// Shared between user space and kernel space for lock-free coordination.
///
/// # Memory Layout
/// ```text
/// +-------------------+
/// | head (4 bytes)    |  <- Consumer index (kernel for SQ, user for CQ)
/// | tail (4 bytes)    |  <- Producer index (user for SQ, kernel for CQ)
/// | ring_mask (4)     |  <- RING_SIZE - 1
/// | ring_entries (4)  |  <- RING_SIZE
/// | flags (4 bytes)   |  <- Ring-specific flags
/// | dropped (4 bytes) |  <- Dropped submissions counter
/// | padding (8)       |  <- Alignment padding
/// +-------------------+
/// Total: 32 bytes
/// ```
///
/// # Synchronization Protocol
///
/// ## For Submission Queue (user produces, kernel consumes):
/// 1. User checks `tail - head < ring_entries` (space available)
/// 2. User writes entry at `entries[tail & ring_mask]`
/// 3. User updates tail with `Release` ordering
/// 4. (Optional) User calls syscall to notify kernel
/// 5. Kernel reads head, then reads entries with `Acquire` ordering
/// 6. Kernel processes entries
/// 7. Kernel updates head with `Release` ordering
///
/// ## For Completion Queue (kernel produces, user consumes):
/// Same protocol with roles reversed.
#[repr(C, align(32))]
#[derive(Debug)]
pub struct RingHeader {
    /// Consumer index (reader position)
    /// - SQ: Kernel increments after processing
    /// - CQ: User increments after reading
    pub head: AtomicU32,
    
    /// Producer index (writer position)
    /// - SQ: User increments after submitting
    /// - CQ: Kernel increments after completing
    pub tail: AtomicU32,
    
    /// Ring mask for index calculation
    /// Always `RING_SIZE - 1`, cached here to avoid recalculation
    pub ring_mask: u32,
    
    /// Number of entries in the ring
    /// Always `RING_SIZE`, cached here for user space
    pub ring_entries: u32,
    
    /// Ring flags
    /// - Bit 0: IORING_SQ_NEED_WAKEUP - kernel needs syscall to wake
    /// - Bit 1: IORING_SQ_CQ_OVERFLOW - CQ has overflowed
    pub flags: AtomicU32,
    
    /// Number of dropped submissions (SQ overflow)
    pub dropped: AtomicU32,
    
    /// Padding to 32-byte alignment
    _padding: [u32; 2],
}

impl RingHeader {
    /// Create a new ring header
    #[must_use]
    pub const fn new() -> Self {
        Self {
            head: AtomicU32::new(0),
            tail: AtomicU32::new(0),
            ring_mask: RING_MASK,
            ring_entries: RING_SIZE,
            flags: AtomicU32::new(0),
            dropped: AtomicU32::new(0),
            _padding: [0; 2],
        }
    }
    
    /// Get the number of entries ready to be consumed
    ///
    /// For SQ: entries submitted by user but not yet processed by kernel
    /// For CQ: completions written by kernel but not yet read by user
    #[must_use]
    pub fn pending_count(&self) -> u32 {
        let tail = self.tail.load(Ordering::Acquire);
        let head = self.head.load(Ordering::Acquire);
        tail.wrapping_sub(head)
    }
    
    /// Get the number of free slots available for the producer
    #[must_use]
    pub fn available_count(&self) -> u32 {
        self.ring_entries - self.pending_count()
    }
    
    /// Check if the ring is empty
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pending_count() == 0
    }
    
    /// Check if the ring is full
    #[must_use]
    pub fn is_full(&self) -> bool {
        self.pending_count() >= self.ring_entries
    }
    
    /// Advance the consumer (head) index
    /// 
    /// # Safety
    /// Must only be called by the consumer (kernel for SQ, user for CQ)
    pub fn advance_head(&self, count: u32) {
        let old = self.head.load(Ordering::Relaxed);
        self.head.store(old.wrapping_add(count), Ordering::Release);
    }
    
    /// Advance the producer (tail) index
    ///
    /// # Safety  
    /// Must only be called by the producer (user for SQ, kernel for CQ)
    pub fn advance_tail(&self, count: u32) {
        let old = self.tail.load(Ordering::Relaxed);
        self.tail.store(old.wrapping_add(count), Ordering::Release);
    }
    
    /// Get the current head index (consumer position)
    #[must_use]
    pub fn head(&self) -> u32 {
        self.head.load(Ordering::Acquire)
    }
    
    /// Get the current tail index (producer position)
    #[must_use]
    pub fn tail(&self) -> u32 {
        self.tail.load(Ordering::Acquire)
    }
}

impl Default for RingHeader {
    fn default() -> Self {
        Self::new()
    }
}

// Compile-time size assertions
const _: () = {
    assert!(core::mem::size_of::<SubmissionEntry>() == 64, "SQE must be 64 bytes");
    assert!(core::mem::size_of::<CompletionEntry>() == 16, "CQE must be 16 bytes");
    assert!(core::mem::size_of::<RingHeader>() == 32, "RingHeader must be 32 bytes");
    assert!(RING_SIZE.is_power_of_two(), "RING_SIZE must be power of 2");
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ring_header_operations() {
        let header = RingHeader::new();
        
        assert!(header.is_empty());
        assert!(!header.is_full());
        assert_eq!(header.pending_count(), 0);
        assert_eq!(header.available_count(), RING_SIZE);
        
        // Simulate producer writing 10 entries
        header.advance_tail(10);
        assert_eq!(header.pending_count(), 10);
        assert_eq!(header.available_count(), RING_SIZE - 10);
        
        // Simulate consumer processing 5 entries
        header.advance_head(5);
        assert_eq!(header.pending_count(), 5);
        assert_eq!(header.available_count(), RING_SIZE - 5);
    }
    
    #[test]
    fn test_submission_entry_creation() {
        let sqe = SubmissionEntry::write(1, 0x1000, 100, 0, 42);
        
        assert_eq!(sqe.op(), Some(OpCode::Write));
        assert_eq!(sqe.fd, 1);
        assert_eq!(sqe.addr, 0x1000);
        assert_eq!(sqe.len, 100);
        assert_eq!(sqe.user_data, 42);
    }
    
    #[test]
    fn test_completion_entry_error() {
        let cqe = CompletionEntry::error(42, 14); // EFAULT
        
        assert!(cqe.is_error());
        assert_eq!(cqe.errno(), Some(14));
        assert_eq!(cqe.user_data, 42);
    }
}
