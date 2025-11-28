//! Ring-based Async I/O API
//!
//! This module provides userspace API for the revolutionary ring-based
//! syscall system that completely replaces traditional syscalls with
//! io_uring-style asynchronous message passing.
//!
//! # Architecture
//!
//! Instead of traditional syscalls where each operation requires a kernel
//! entry/exit cycle, this API uses shared memory ring buffers:
//!
//! ```text
//! User Space                  Kernel Space
//! +--------------+            +--------------+
//! | Submit Queue |  ------>   | Poll & Process|
//! | (write ops)  |            |              |
//! +--------------+            +--------------+
//! | Complete Queue|  <------  | Write results|
//! | (read results)|           |              |
//! +--------------+            +--------------+
//! ```
//!
//! # Benefits
//!
//! 1. **Zero syscalls for I/O**: After setup, all I/O goes through shared memory
//! 2. **Batched operations**: Submit multiple ops, wait once
//! 3. **Pre-validated buffers**: Register buffers once, use forever
//! 4. **SQPOLL mode**: Kernel polls automatically, no user action needed
//!
//! # Usage Example
//!
//! ```no_run
//! use libuser::ring_io::{Ring, RingBuilder, Op};
//!
//! // Create a ring with SQPOLL enabled
//! let ring = RingBuilder::new()
//!     .sqpoll(true)
//!     .build()
//!     .expect("Failed to create ring");
//!
//! // Register a buffer for I/O (validated once, used forever)
//! let buf_id = ring.register_buffer(&my_buffer).expect("Failed to register");
//!
//! // Submit a write operation
//! let user_data = 42;
//! ring.submit(Op::write(1, buf_id, 0, data.len() as u32, user_data));
//!
//! // With SQPOLL, no need to kick - kernel polls automatically
//! // Without SQPOLL, call ring.enter() to notify kernel
//!
//! // Wait for completion
//! let cqe = ring.wait_cqe();
//! assert_eq!(cqe.user_data, 42);
//! ```

use core::sync::atomic::{AtomicU32, Ordering};
use crate::syscall::{SyscallResult, SyscallError, errno};

// =============================================================================
// Constants
// =============================================================================

/// Ring buffer size (must match kernel's RING_SIZE)
pub const RING_SIZE: usize = 256;
const RING_MASK: u32 = (RING_SIZE - 1) as u32;

/// Maximum number of registered buffers
pub const MAX_BUFFERS: usize = 64;

/// Syscall numbers for ring operations
/// Ring enter syscall - signals kernel to process pending operations
pub const SYSCALL_RING_ENTER: u64 = 2000;
/// Ring register syscall - registers a memory buffer for zero-copy I/O
pub const SYSCALL_RING_REGISTER: u64 = 2001;
/// Ring setup syscall - initializes ring context for a process
pub const SYSCALL_RING_SETUP: u64 = 2002;

// =============================================================================
// Operation Codes
// =============================================================================

/// Operation codes matching kernel's RingOpcode
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opcode {
    /// No operation
    Nop = 0,
    /// Write to file descriptor
    Write = 1,
    /// Read from file descriptor
    Read = 2,
    /// Get timestamp
    GetTime = 3,
    /// Get process ID
    GetPid = 4,
    /// Yield CPU
    Yield = 5,
    /// Memory fence
    Fence = 6,
    /// Exit process
    Exit = 7,
    /// Register buffer
    RegisterBuffer = 8,
    /// Unregister buffer
    UnregisterBuffer = 9,
    /// Console write
    ConsoleWrite = 15,
}

// =============================================================================
// Submission Queue Entry (User View)
// =============================================================================

/// Submission Queue Entry for user code
///
/// This structure is cache-line aligned and uses handle-based
/// addressing instead of raw pointers.
#[repr(C, align(64))]
#[derive(Debug, Clone, Copy)]
pub struct Sqe {
    /// Operation code
    pub opcode: u8,
    /// Flags
    pub flags: u8,
    /// I/O priority
    pub ioprio: u16,
    /// File descriptor
    pub fd: u32,
    /// Registered buffer index
    pub buf_index: u16,
    /// Offset within buffer
    pub buf_offset: u32,
    /// Operation length
    pub len: u32,
    /// Generic argument 1
    pub arg1: u64,
    /// Generic argument 2
    pub arg2: u64,
    /// User data (returned in completion)
    pub user_data: u64,
    /// Padding
    _padding: [u8; 14],
}

impl Sqe {
    /// Create an empty SQE
    pub const fn empty() -> Self {
        Self {
            opcode: 0,
            flags: 0,
            ioprio: 0,
            fd: 0,
            buf_index: 0,
            buf_offset: 0,
            len: 0,
            arg1: 0,
            arg2: 0,
            user_data: 0,
            _padding: [0; 14],
        }
    }
    
    /// Create a write operation
    pub const fn write(fd: u32, buf_index: u16, offset: u32, len: u32, user_data: u64) -> Self {
        Self {
            opcode: Opcode::Write as u8,
            flags: 0,
            ioprio: 0,
            fd,
            buf_index,
            buf_offset: offset,
            len,
            arg1: 0,
            arg2: 0,
            user_data,
            _padding: [0; 14],
        }
    }
    
    /// Create a read operation
    pub const fn read(fd: u32, buf_index: u16, offset: u32, len: u32, user_data: u64) -> Self {
        Self {
            opcode: Opcode::Read as u8,
            flags: 0,
            ioprio: 0,
            fd,
            buf_index,
            buf_offset: offset,
            len,
            arg1: 0,
            arg2: 0,
            user_data,
            _padding: [0; 14],
        }
    }
    
    /// Create a console write operation
    pub const fn console_write(buf_index: u16, offset: u32, len: u32, user_data: u64) -> Self {
        Self {
            opcode: Opcode::ConsoleWrite as u8,
            flags: 0,
            ioprio: 0,
            fd: 1, // stdout
            buf_index,
            buf_offset: offset,
            len,
            arg1: 0,
            arg2: 0,
            user_data,
            _padding: [0; 14],
        }
    }
    
    /// Create a getpid operation
    pub const fn getpid(user_data: u64) -> Self {
        Self {
            opcode: Opcode::GetPid as u8,
            flags: 0,
            ioprio: 0,
            fd: 0,
            buf_index: 0,
            buf_offset: 0,
            len: 0,
            arg1: 0,
            arg2: 0,
            user_data,
            _padding: [0; 14],
        }
    }
    
    /// Create an exit operation
    pub const fn exit(code: i32, user_data: u64) -> Self {
        Self {
            opcode: Opcode::Exit as u8,
            flags: 0,
            ioprio: 0,
            fd: 0,
            buf_index: 0,
            buf_offset: 0,
            len: 0,
            arg1: code as u64,
            arg2: 0,
            user_data,
            _padding: [0; 14],
        }
    }
    
    /// Create a yield operation
    pub const fn yield_cpu(user_data: u64) -> Self {
        Self {
            opcode: Opcode::Yield as u8,
            flags: 0,
            ioprio: 0,
            fd: 0,
            buf_index: 0,
            buf_offset: 0,
            len: 0,
            arg1: 0,
            arg2: 0,
            user_data,
            _padding: [0; 14],
        }
    }
    
    /// Create a timestamp operation
    pub const fn get_time(user_data: u64) -> Self {
        Self {
            opcode: Opcode::GetTime as u8,
            flags: 0,
            ioprio: 0,
            fd: 0,
            buf_index: 0,
            buf_offset: 0,
            len: 0,
            arg1: 0,
            arg2: 0,
            user_data,
            _padding: [0; 14],
        }
    }
}

// =============================================================================
// Completion Queue Entry (User View)
// =============================================================================

/// Completion Queue Entry
#[repr(C, align(16))]
#[derive(Debug, Clone, Copy)]
pub struct Cqe {
    /// User data from submission
    pub user_data: u64,
    /// Result (positive = success, negative = -errno)
    pub result: i64,
}

impl Cqe {
    /// Check if this completion indicates success
    #[inline]
    pub fn is_ok(&self) -> bool {
        self.result >= 0
    }
    
    /// Check if this completion indicates an error
    #[inline]
    pub fn is_err(&self) -> bool {
        self.result < 0
    }
    
    /// Get the result as a SyscallResult
    pub fn to_result(&self) -> SyscallResult<i64> {
        if self.result >= 0 {
            Ok(self.result)
        } else {
            Err(SyscallError::from_raw(-self.result as i64))
        }
    }
}

// =============================================================================
// Ring Header
// =============================================================================

/// Ring header for lock-free producer/consumer
#[repr(C, align(64))]
pub struct RingHeader {
    /// Head (consumer reads here)
    pub head: AtomicU32,
    /// Tail (producer writes here)
    pub tail: AtomicU32,
    /// Ring mask (size - 1)
    pub ring_mask: u32,
    /// Flags
    pub flags: AtomicU32,
    /// Padding
    _padding: [u32; 12],
}

impl RingHeader {
    /// Check if entries are available
    #[inline]
    pub fn has_entries(&self) -> bool {
        self.head.load(Ordering::Acquire) != self.tail.load(Ordering::Acquire)
    }
    
    /// Get number of available entries
    #[inline]
    pub fn available(&self) -> u32 {
        let tail = self.tail.load(Ordering::Acquire);
        let head = self.head.load(Ordering::Acquire);
        tail.wrapping_sub(head)
    }
    
    /// Get number of free slots
    #[inline]
    pub fn free_slots(&self) -> u32 {
        RING_SIZE as u32 - self.available()
    }
}

/// Ring flags
pub mod flags {
    /// Kernel polling active
    pub const SQPOLL: u32 = 1 << 0;
    /// Kernel poller needs wakeup
    pub const NEED_WAKEUP: u32 = 1 << 3;
}

// =============================================================================
// Ring Structure (Main API)
// =============================================================================

/// Ring-based I/O context
///
/// This is the main interface for async I/O operations.
pub struct Ring {
    /// Submission queue header
    sq: *mut RingHeader,
    /// Completion queue header
    cq: *mut RingHeader,
    /// Submission queue entries
    sq_entries: *mut Sqe,
    /// Completion queue entries
    cq_entries: *mut Cqe,
    /// Ring size
    ring_size: u32,
    /// SQPOLL enabled
    sqpoll: bool,
    /// Registered buffer IDs
    registered_buffers: [bool; MAX_BUFFERS],
}

impl Ring {
    /// Set up a new ring via syscall
    pub fn setup(sqpoll: bool) -> SyscallResult<Self> {
        // Call ring setup syscall
        let result = unsafe {
            super::syscall::syscall1(SYSCALL_RING_SETUP, if sqpoll { 1 } else { 0 })
        };
        
        if result < 0 {
            return Err(SyscallError::from_raw(-result));
        }
        
        // The kernel returns the address of the ring structure
        // For now, we create a dummy ring for testing
        // In full implementation, kernel would return addresses
        
        Err(SyscallError::from_raw(errno::ENOSYS))
    }
    
    /// Create a ring from raw pointers (for testing/debugging)
    ///
    /// # Safety
    ///
    /// All pointers must be valid and point to properly initialized structures.
    pub unsafe fn from_raw(
        sq: *mut RingHeader,
        cq: *mut RingHeader,
        sq_entries: *mut Sqe,
        cq_entries: *mut Cqe,
        ring_size: u32,
        sqpoll: bool,
    ) -> Self {
        Self {
            sq,
            cq,
            sq_entries,
            cq_entries,
            ring_size,
            sqpoll,
            registered_buffers: [false; MAX_BUFFERS],
        }
    }
    
    /// Submit an operation to the ring
    ///
    /// Returns the slot index on success.
    pub fn submit(&mut self, sqe: Sqe) -> SyscallResult<u32> {
        unsafe {
            let sq = &*self.sq;
            let tail = sq.tail.load(Ordering::Acquire);
            let head = sq.head.load(Ordering::Acquire);
            
            // Check for full ring
            if tail.wrapping_sub(head) >= self.ring_size {
                return Err(SyscallError::from_raw(errno::EAGAIN));
            }
            
            // Write entry
            let idx = (tail & RING_MASK) as usize;
            let entry = self.sq_entries.add(idx);
            core::ptr::write_volatile(entry, sqe);
            
            // Memory barrier
            core::sync::atomic::fence(Ordering::Release);
            
            // Advance tail
            sq.tail.store(tail.wrapping_add(1), Ordering::Release);
            
            Ok(idx as u32)
        }
    }
    
    /// Submit multiple operations at once
    pub fn submit_all(&mut self, sqes: &[Sqe]) -> SyscallResult<u32> {
        let mut submitted = 0;
        for sqe in sqes {
            match self.submit(*sqe) {
                Ok(_) => submitted += 1,
                Err(e) if submitted == 0 => return Err(e),
                Err(_) => break,
            }
        }
        Ok(submitted)
    }
    
    /// Kick the kernel to process submissions (for non-SQPOLL mode)
    ///
    /// In SQPOLL mode, this checks if kernel needs wakeup.
    pub fn enter(&self) -> SyscallResult<u64> {
        if self.sqpoll {
            // Check if kernel poller needs wakeup
            let sq = unsafe { &*self.sq };
            if sq.flags.load(Ordering::Acquire) & flags::NEED_WAKEUP != 0 {
                // Kernel poller sleeping, need to wake it
                unsafe {
                    let result = super::syscall::syscall1(SYSCALL_RING_ENTER, 0);
                    if result < 0 {
                        return Err(SyscallError::from_raw(-result));
                    }
                    return Ok(result as u64);
                }
            }
            // Kernel is actively polling, no syscall needed!
            Ok(0)
        } else {
            // Non-SQPOLL: must call syscall
            unsafe {
                let result = super::syscall::syscall1(SYSCALL_RING_ENTER, 0);
                if result < 0 {
                    return Err(SyscallError::from_raw(-result));
                }
                Ok(result as u64)
            }
        }
    }
    
    /// Check if completions are available
    #[inline]
    pub fn has_completions(&self) -> bool {
        unsafe { (*self.cq).has_entries() }
    }
    
    /// Get number of pending completions
    #[inline]
    pub fn pending_completions(&self) -> u32 {
        unsafe { (*self.cq).available() }
    }
    
    /// Try to get a completion without blocking
    pub fn try_get_cqe(&mut self) -> Option<Cqe> {
        unsafe {
            let cq = &*self.cq;
            let head = cq.head.load(Ordering::Acquire);
            let tail = cq.tail.load(Ordering::Acquire);
            
            if head == tail {
                return None;
            }
            
            // Read entry
            let idx = (head & RING_MASK) as usize;
            let entry = self.cq_entries.add(idx);
            let cqe = core::ptr::read_volatile(entry);
            
            // Advance head
            cq.head.store(head.wrapping_add(1), Ordering::Release);
            
            Some(cqe)
        }
    }
    
    /// Wait for a completion (busy-wait)
    pub fn wait_cqe(&mut self) -> Cqe {
        loop {
            if let Some(cqe) = self.try_get_cqe() {
                return cqe;
            }
            core::hint::spin_loop();
        }
    }
    
    /// Wait for N completions
    pub fn wait_cqes(&mut self, n: u32) -> impl Iterator<Item = Cqe> + '_ {
        let mut count = 0;
        core::iter::from_fn(move || {
            if count >= n {
                return None;
            }
            count += 1;
            Some(self.wait_cqe())
        })
    }
    
    /// Register a buffer for zero-copy I/O
    ///
    /// After registration, use the returned buffer ID instead of pointers.
    pub fn register_buffer(&mut self, addr: u64, len: u64, read: bool, write: bool) -> SyscallResult<u16> {
        // Find free slot
        let slot = self.registered_buffers.iter()
            .position(|&used| !used)
            .ok_or(SyscallError::from_raw(errno::ENOSPC))?;
        
        // Call kernel to register
        let flags = (if read { 1 } else { 0 }) | (if write { 2 } else { 0 });
        unsafe {
            let result = super::syscall::syscall4(
                SYSCALL_RING_REGISTER,
                addr as i64,
                len as i64,
                flags,
                slot as i64,
            );
            
            if result < 0 {
                return Err(SyscallError::from_raw(-result));
            }
            
            self.registered_buffers[slot] = true;
            Ok(slot as u16)
        }
    }
    
    /// Unregister a buffer
    pub fn unregister_buffer(&mut self, buf_id: u16) -> SyscallResult<()> {
        if buf_id as usize >= MAX_BUFFERS || !self.registered_buffers[buf_id as usize] {
            return Err(SyscallError::from_raw(errno::EINVAL));
        }
        
        // Call kernel to unregister
        // For now, just mark as free
        self.registered_buffers[buf_id as usize] = false;
        Ok(())
    }
    
    /// Check if SQPOLL is enabled
    #[inline]
    pub fn is_sqpoll(&self) -> bool {
        self.sqpoll
    }
}

// =============================================================================
// Builder Pattern
// =============================================================================

/// Builder for creating Ring instances
pub struct RingBuilder {
    sqpoll: bool,
    ring_size: u32,
}

impl RingBuilder {
    /// Create a new builder with default settings
    pub fn new() -> Self {
        Self {
            sqpoll: false,
            ring_size: RING_SIZE as u32,
        }
    }
    
    /// Enable SQPOLL mode (kernel polling)
    pub fn sqpoll(mut self, enable: bool) -> Self {
        self.sqpoll = enable;
        self
    }
    
    /// Build the ring
    pub fn build(self) -> SyscallResult<Ring> {
        Ring::setup(self.sqpoll)
    }
}

impl Default for RingBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Convenience Functions
// =============================================================================

/// Quick write using ring I/O
///
/// This creates a temporary ring, performs the write, and returns.
/// For repeated operations, create a Ring and reuse it.
pub fn ring_write(fd: u32, buf_index: u16, offset: u32, data: &[u8]) -> SyscallResult<usize> {
    // This would need a pre-initialized ring
    // For now, return not implemented
    Err(SyscallError::from_raw(errno::ENOSYS))
}

/// Get current timestamp using ring I/O
pub fn ring_timestamp() -> SyscallResult<u64> {
    Err(SyscallError::from_raw(errno::ENOSYS))
}
