// libuser/src/ring_io.rs
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
use crate::syscall::SyscallResult;
use crate::abi::error::SyscallError;
use crate::abi::io_uring_common::{RING_MASK, RING_SIZE as COMMON_RING_SIZE};
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



// =============================================================================
// Doorbell Layout
// =============================================================================

/// Doorbell for zero-syscall notifications
///
/// Shared between user and kernel for lock-free communication.
#[repr(C, align(4096))]
pub struct DoorbellLayout {
    /// Doorbell counter (user writes, kernel reads)
    pub ring: AtomicU32,
    /// Needs wakeup flag (kernel sets, user reads)
    pub needs_wakeup: AtomicBool,
    /// CQ has entries ready (kernel sets, user reads)
    pub cq_ready: AtomicBool,
    /// SQPOLL running flag (kernel sets, user reads)
    pub sqpoll_running: AtomicBool,
    /// Padding to fill the page
    _pad: [u8; 4096 - 10],
}

// Ensure userspace doorbell (page) is exactly one page in size to match kernel
const _: () = assert!(core::mem::size_of::<DoorbellLayout>() == 4096);

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

    /// Increment the doorbell counter (user-space ring)
    #[inline]
    pub fn ring_doorbell(&self) {
        self.ring.fetch_add(1, Ordering::Release);
    }

    /// Peek at the current doorbell counter without changing it
    #[inline]
    pub fn peek_ring(&self) -> u32 {
        self.ring.load(Ordering::Acquire)
    }

    /// Check if SQPOLL is running on the kernel side
    #[inline]
    pub fn is_sqpoll_running(&self) -> bool {
        self.sqpoll_running.load(Ordering::Acquire)
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
        self.into_result()
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
    pub fn setup(sqpoll: bool) -> SyscallResult<Self> {
        // Entries is currently fixed by the ABI (COMMON_RING_SIZE)
        let entries = RING_SIZE as u32;
        let flags = if sqpoll { 1u32 } else { 0u32 };

        // Call the kernel to set up the ring and map it to the fixed user address
        let user_addr = crate::syscall::io_uring_setup(entries, flags)?;

        // Create Ring from the mapped address
        unsafe { Ring::from_address(user_addr, sqpoll) }
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
                return Err(SyscallError::WouldBlock);
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
                crate::syscall::io_uring_enter(0, 0)?;
                // We don't get the number of processed items from enter anymore in V2?
                // Actually sys_io_uring_enter_v2 returns 0 on success.
                // The completion count is in the CQ.
                return Ok(0);
            }
            // Kernel is actively polling, no syscall needed!
            Ok(0)
        } else {
            // Non-SQPOLL: must call syscall
            crate::syscall::io_uring_enter(0, 0)?;
            Ok(0)
        }
    }

    /// Ring the doorbell to notify kernel about new submissions (no syscall)
    ///
    /// In SQPOLL mode the kernel will poll the submission queue and process
    /// new submissions; userspace sets the `needs_wakeup` flag to ask the
    /// kernel poller to wake up if it is idle.
    pub fn ring_doorbell(&self) {
        self.doorbell().ring_doorbell()
    }

    /// Check if the kernel has set the CQ ready flag via the doorbell
    #[inline]
    pub fn check_cq_ready(&self) -> bool {
        self.doorbell().is_cq_ready()
    }

    /// Clear the CQ ready flag
    #[inline]
    pub fn clear_cq_ready(&self) {
        self.doorbell().clear_cq_ready();
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
        // from_fn returns an iterator that yields N CQEs
    }
}

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
