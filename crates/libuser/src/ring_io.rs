//! Ring-based Async I/O API (V2)
//!
//! This module provides userspace API for the V2 io_uring syscall system
//! with capability-based security.
//!
//! # Architecture
//!
//! Instead of traditional syscalls, this API uses shared memory ring buffers:
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
//! # V2 Features
//!
//! 1. **Capability-based**: Uses capability IDs instead of file descriptors
//! 2. **Type-safe results**: AbiResult<T, E> instead of errno
//! 3. **Doorbell notifications**: Zero-syscall mode with shared memory flags
//! 4. **64-byte SQE**: Extended submission entries with auxiliary fields
//! 5. **40-byte CQE**: Completion entries with typed results

#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_const_for_fn)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::volatile_composites)]

use core::sync::atomic::{AtomicU32, AtomicBool, Ordering};
use crate::syscall::{SyscallResult, SyscallError, errno};
use crate::abi::io_uring_common::{OpCode, RING_MASK, RING_SIZE as COMMON_RING_SIZE};
use crate::abi::io_uring_v2::{SubmissionEntryV2, CompletionEntryV2, RingHeaderV2};

// =============================================================================
// Re-exports (V2 Types)
// =============================================================================

/// Submission Queue Entry (V2)
pub type Sqe = SubmissionEntryV2;

/// Completion Queue Entry (V2)
pub type Cqe = CompletionEntryV2;

/// Ring Header (V2)
pub type RingHeader = RingHeaderV2;

// =============================================================================
// Constants
// =============================================================================

/// Maximum number of registered buffers
pub const MAX_BUFFERS: usize = 64;

/// Ring setup syscall - V2 io_uring setup
pub const SYSCALL_IO_URING_SETUP: u64 = 12;

/// Ring enter syscall - signals kernel to process pending operations
pub const SYSCALL_IO_URING_ENTER: u64 = 13;

/// Ring register syscall - registers a memory buffer for zero-copy I/O
pub const SYSCALL_IO_URING_REGISTER: u64 = 14;

// =============================================================================
// Doorbell Layout
// =============================================================================

/// Doorbell for zero-syscall notifications
///
/// Shared between user and kernel for lock-free communication.
#[repr(C, align(64))]
pub struct DoorbellLayout {
    /// CQ has entries ready (kernel writes, user reads)
    pub cq_ready: AtomicBool,
    /// Kernel needs wakeup (user writes, kernel reads)
    pub needs_wakeup: AtomicBool,
    /// Padding to 64 bytes
    _padding: [u8; 62],
}

impl DoorbellLayout {
    /// Check if completions are ready
    #[inline]
    pub fn is_cq_ready(&self) -> bool {
        self.cq_ready.load(Ordering::Acquire)
    }
    
    /// Clear CQ ready flag
    #[inline]
    pub fn clear_cq_ready(&self) {
        self.cq_ready.store(false, Ordering::Release);
    }
    
    /// Check if kernel needs wakeup
    #[inline]
    pub fn check_needs_wakeup(&self) -> bool {
        self.needs_wakeup.load(Ordering::Acquire)
    }
    
    /// Set needs wakeup flag
    #[inline]
    pub fn set_needs_wakeup(&self, value: bool) {
        self.needs_wakeup.store(value, Ordering::Release);
    }
}

// =============================================================================
// Helper Methods for V2 Types
// =============================================================================

impl Sqe {
    /// Create a write operation (capability-based)
    #[must_use]
    pub const fn write_cap(capability_id: u64, buf_index: u32, len: u32, offset: u64, user_data: u64) -> Self {
        Self::write(capability_id, buf_index, len, offset, user_data)
    }
    
    /// Create a read operation (capability-based)
    #[must_use]
    pub const fn read_cap(capability_id: u64, buf_index: u32, len: u32, offset: u64, user_data: u64) -> Self {
        Self::read(capability_id, buf_index, len, offset, user_data)
    }
    
    /// Create a close operation (capability-based)
    #[must_use]
    pub const fn close_cap(capability_id: u64, user_data: u64) -> Self {
        Self::close(capability_id, user_data)
    }
}

impl Cqe {
    /// Get the result as a `SyscallResult`
    pub fn to_syscall_result(&self) -> SyscallResult<i32> {
        match self.into_result() {
            Ok(val) => Ok(val),
            Err(e) => Err(SyscallError::from_raw(e as i64)),
        }
    }
}

// =============================================================================
// Ring Context Layout
// =============================================================================

/// Fixed user-space address where RingContext is mapped
/// This must match USER_IO_URING_BASE in kernel
pub const USER_IO_URING_BASE: u64 = 0x2000_0000_0000;

/// Doorbell offset within ring context
const DOORBELL_OFFSET: u64 = 0x7000;

/// Ring size
const RING_SIZE: usize = COMMON_RING_SIZE as usize;

/// RingContext layout in user memory
///
/// This mirrors the kernel's memory layout for io_uring V2.
#[repr(C)]
struct RingContextLayout {
    sq_header: RingHeaderV2,        // Offset 0x0000 (1 page)
    cq_header: RingHeaderV2,        // Offset 0x1000 (1 page)
    sq_entries: [SubmissionEntryV2; RING_SIZE],  // Offset 0x2000 (4 pages)
    cq_entries: [CompletionEntryV2; RING_SIZE],  // Offset 0x6000 (3 pages)
    // Doorbell at offset 0x7000 (1 page)
}

// =============================================================================
// Ring Structure (Main API)
// =============================================================================

/// Ring-based I/O context (V2)
///
/// This is the main interface for async I/O operations using
/// V2 io_uring with capability-based security.
pub struct Ring {
    /// Submission queue header
    sq: *mut RingHeaderV2,
    /// Completion queue header
    cq: *mut RingHeaderV2,
    /// Submission queue entries
    sq_entries: *mut SubmissionEntryV2,
    /// Completion queue entries
    cq_entries: *mut CompletionEntryV2,
    /// Doorbell for zero-syscall mode
    doorbell: *mut DoorbellLayout,
    /// Ring size
    #[allow(dead_code)]
    ring_size: u32,
    /// SQPOLL enabled
    sqpoll: bool,
    /// Registered buffer IDs
    registered_buffers: [bool; MAX_BUFFERS],
}

impl Ring {
    /// Set up a new ring via syscall
    ///
    /// This calls the kernel's V2 `io_uring_setup` syscall which:
    /// 1. Allocates ring buffers in kernel memory
    /// 2. Maps them to USER_IO_URING_BASE in user address space
    /// 3. Sets up doorbell for zero-syscall mode
    ///
    /// # Errors
    ///
    /// Returns an error if the syscall fails.
    #[allow(clippy::cast_possible_truncation)]
    pub fn setup(sqpoll: bool) -> SyscallResult<Self> {
        // Call io_uring_setup V2
        let entries = COMMON_RING_SIZE as i64;
        let flags = if sqpoll { 1 } else { 0 };
        
        let result = unsafe {
            crate::syscall::syscall2(SYSCALL_IO_URING_SETUP, entries, flags)
        };
        
        if result < 0 {
            return Err(SyscallError::from_raw(-result));
        }
        
        // The kernel returns the base address
        #[allow(clippy::cast_sign_loss)]
        let base_addr = result as u64;
        
        // SAFETY: Kernel has mapped this address properly
        unsafe { Self::from_address(base_addr, sqpoll) }
    }
    
    /// Create a ring from a raw address
    ///
    /// # Safety
    ///
    /// The address must point to a valid, mapped RingContext structure.
    #[must_use]
    pub unsafe fn from_address(base_addr: u64, sqpoll: bool) -> SyscallResult<Self> {
        let ctx = base_addr as *mut RingContextLayout;
        
        // SAFETY: The caller guarantees base_addr points to a valid RingContextLayout
        unsafe {
            let sq = core::ptr::addr_of_mut!((*ctx).sq_header);
            let cq = core::ptr::addr_of_mut!((*ctx).cq_header);
            let sq_entries = core::ptr::addr_of_mut!((*ctx).sq_entries).cast::<SubmissionEntryV2>();
            let cq_entries = core::ptr::addr_of_mut!((*ctx).cq_entries).cast::<CompletionEntryV2>();
            let doorbell = (base_addr + DOORBELL_OFFSET) as *mut DoorbellLayout;
            
            Ok(Self {
                sq,
                cq,
                sq_entries,
                cq_entries,
                doorbell,
                ring_size: COMMON_RING_SIZE,
                sqpoll,
                registered_buffers: [false; MAX_BUFFERS],
            })
        }
    }
    
    /// Get a reference to the doorbell
    #[inline]
    fn doorbell(&self) -> &DoorbellLayout {
        unsafe { &*self.doorbell }
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
    /// In SQPOLL mode, checks doorbell's needs_wakeup flag.
    pub fn enter(&self) -> SyscallResult<u64> {
        if self.sqpoll {
            // Check if kernel poller needs wakeup via doorbell
            if self.doorbell().check_needs_wakeup() {
                // Kernel poller sleeping, need to wake it
                unsafe {
                    let result = crate::syscall::syscall1(SYSCALL_IO_URING_ENTER, 0);
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
                let result = crate::syscall::syscall1(SYSCALL_IO_URING_ENTER, 0);
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
        unsafe { (*self.cq).pending_count() > 0 }
    }
    
    /// Get number of pending completions
    #[inline]
    pub fn pending_completions(&self) -> u32 {
        unsafe { (*self.cq).pending_count() }
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
            
            // Clear doorbell CQ ready flag if no more entries
            if cq.head.load(Ordering::Acquire) == cq.tail.load(Ordering::Acquire) {
                self.doorbell().clear_cq_ready();
            }
            
            Some(cqe)
        }
    }
    
    /// Wait for a completion (busy-wait)
    pub fn wait_cqe(&mut self) -> Cqe {
        loop {
            // Check doorbell first for efficiency
            if self.sqpoll && !self.doorbell().is_cq_ready() {
                core::hint::spin_loop();
                continue;
            }
            
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
    #[allow(clippy::bool_to_int_with_if)]
    pub fn register_buffer(&mut self, addr: u64, len: u64, read: bool, write: bool) -> SyscallResult<u16> {
        // Find free slot
        let slot = self.registered_buffers.iter()
            .position(|&used| !used)
            .ok_or(SyscallError::from_raw(errno::ENOSPC))?;
        
        // Call kernel to register
        let flags = (if read { 1 } else { 0 }) | (if write { 2 } else { 0 });
        unsafe {
            let result = crate::syscall::syscall4(
                SYSCALL_IO_URING_REGISTER,
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
    #[must_use]
    pub fn is_sqpoll(&self) -> bool {
        self.sqpoll
    }
    
    /// Get number of free slots in submission queue
    #[inline]
    #[must_use]
    pub fn sq_free_slots(&self) -> u32 {
        unsafe { (*self.sq).available_count() }
    }
}

// =============================================================================
// Builder Pattern
// =============================================================================

/// Builder for creating Ring instances
pub struct RingBuilder {
    sqpoll: bool,
}

impl RingBuilder {
    /// Create a new builder with default settings
    #[must_use]
    pub fn new() -> Self {
        Self {
            sqpoll: false,
        }
    }
    
    /// Enable SQPOLL mode (kernel polling)
    #[must_use]
    pub fn sqpoll(mut self, enable: bool) -> Self {
        self.sqpoll = enable;
        self
    }
    
    /// Build the ring
    ///
    /// # Errors
    ///
    /// Returns an error if the ring setup syscall fails.
    pub fn build(self) -> SyscallResult<Ring> {
        Ring::setup(self.sqpoll)
    }
}

impl Default for RingBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// Note: Ring is not Send/Sync due to raw pointers
// This is intentional as it should only be used from the creating thread
