// kernel/src/kernel/io_uring/doorbell.rs
//! Doorbell Mechanism for Zero-Syscall I/O
//!
//! This module implements a shared memory doorbell that allows user-space
//! to notify the kernel without making a syscall. Combined with SQPOLL,
//! this enables truly syscall-free I/O operations.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                         User Space                               │
//! │                                                                  │
//! │  1. Write SQE to SQ (shared memory)                             │
//! │  2. Update SQ tail (atomic)                                     │
//! │  3. Write to doorbell (shared memory) ← NO SYSCALL              │
//! │  4. Poll CQ tail (atomic)                                       │
//! │  5. Read CQE from CQ (shared memory)                            │
//! │                                                                  │
//! └───────────────────────────────────────────────────────────────-─┘
//!                               │ doorbell write detected
//!                               ▼
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                        Kernel Space (SQPOLL)                     │
//! │                                                                  │
//! │  SQPOLL Worker:                                                  │
//! │  1. Poll doorbell / SQ tail                                     │
//! │  2. Copy SQE to kernel (TOCTOU protection)                      │
//! │  3. Process operation                                           │
//! │  4. Write CQE to CQ                                             │
//! │  5. Set cq_ready flag                                           │
//! │                                                                  │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Usage
//!
//! 1. Kernel allocates a page for the doorbell and maps it to user space
//! 2. User writes non-zero to `ring` field to notify kernel
//! 3. SQPOLL worker polls the doorbell and processes submissions
//! 4. Kernel sets `cq_ready` when completions are available

use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// Doorbell structure (mapped to shared memory)
///
/// This structure is page-aligned and designed to be mapped into user space.
/// Users can ring the doorbell without syscalls, and the kernel can signal
/// when completions are ready.
#[repr(C, align(4096))]
pub struct Doorbell {
    /// Doorbell counter (user writes, kernel reads)
    ///
    /// User increments this to notify the kernel of new submissions.
    /// Kernel reads and clears to acknowledge.
    pub ring: AtomicU32,

    /// Needs wakeup flag (kernel sets, user reads)
    ///
    /// Set by kernel when SQPOLL worker is idle and syscall is needed
    /// to wake it up. User should check this before calling io_uring_enter().
    pub needs_wakeup: AtomicBool,

    /// CQ ready flag (kernel sets, user reads)
    ///
    /// Set by kernel when new completions are available in CQ.
    /// User can poll this instead of the CQ tail for simple cases.
    pub cq_ready: AtomicBool,

    /// SQPOLL running flag (kernel sets, user reads)
    ///
    /// Indicates whether SQPOLL worker is actively running.
    pub sqpoll_running: AtomicBool,

    /// Padding to fill the page
    _pad: [u8; 4096 - 10], // 4 (ring) + 1 (needs_wakeup) + 1 (cq_ready) + 1 (sqpoll_running) + 3 alignment
}

// Ensure the structure is exactly one page
const _: () = assert!(core::mem::size_of::<Doorbell>() == 4096);

impl Doorbell {
    /// Create a new doorbell with default values
    pub const fn new() -> Self {
        Self {
            ring: AtomicU32::new(0),
            needs_wakeup: AtomicBool::new(false),
            cq_ready: AtomicBool::new(false),
            sqpoll_running: AtomicBool::new(false),
            _pad: [0; 4096 - 10],
        }
    }

    // =========================================================================
    // User-space operations (also called from kernel for testing)
    // =========================================================================

    /// Ring the doorbell (user-space operation)
    ///
    /// Increments the doorbell counter to notify the kernel of new submissions.
    /// This is the syscall-free notification mechanism.
    #[inline]
    pub fn ring_doorbell(&self) {
        self.ring.fetch_add(1, Ordering::Release);
    }

    /// Check if needs wakeup (user-space operation)
    ///
    /// Returns true if the SQPOLL worker is idle and the user should
    /// call io_uring_enter() to wake it up.
    #[inline]
    pub fn check_needs_wakeup(&self) -> bool {
        self.needs_wakeup.load(Ordering::Acquire)
    }

    /// Check if CQ has completions ready (user-space operation)
    #[inline]
    pub fn check_cq_ready(&self) -> bool {
        self.cq_ready.load(Ordering::Acquire)
    }

    /// Clear CQ ready flag after reading completions (user-space operation)
    #[inline]
    pub fn clear_cq_ready(&self) {
        self.cq_ready.store(false, Ordering::Release);
    }

    /// Check if SQPOLL is running (user-space operation)
    #[inline]
    pub fn is_sqpoll_running(&self) -> bool {
        self.sqpoll_running.load(Ordering::Acquire)
    }

    // =========================================================================
    // Kernel-space operations
    // =========================================================================

    /// Check and clear the doorbell (kernel operation)
    ///
    /// Returns the number of times the doorbell was rung since last check.
    /// This atomically reads and clears the counter.
    #[inline]
    pub fn check_and_clear(&self) -> u32 {
        self.ring.swap(0, Ordering::AcqRel)
    }

    /// Peek at doorbell without clearing (kernel operation)
    ///
    /// Useful for checking if there are new submissions without
    /// acknowledging them.
    #[inline]
    pub fn peek(&self) -> u32 {
        self.ring.load(Ordering::Acquire)
    }

    /// Set needs wakeup flag (kernel operation)
    ///
    /// Called when SQPOLL worker goes idle.
    #[inline]
    pub fn set_needs_wakeup(&self, value: bool) {
        self.needs_wakeup.store(value, Ordering::Release);
    }

    /// Set CQ ready flag (kernel operation)
    ///
    /// Called when new completions are posted to CQ.
    #[inline]
    pub fn set_cq_ready(&self) {
        self.cq_ready.store(true, Ordering::Release);
    }

    /// Set SQPOLL running state (kernel operation)
    #[inline]
    pub fn set_sqpoll_running(&self, running: bool) {
        self.sqpoll_running.store(running, Ordering::Release);
    }

    /// Reset all flags (kernel operation)
    ///
    /// Called during initialization or cleanup.
    pub fn reset(&self) {
        self.ring.store(0, Ordering::Release);
        self.needs_wakeup.store(false, Ordering::Release);
        self.cq_ready.store(false, Ordering::Release);
        self.sqpoll_running.store(false, Ordering::Release);
    }
}

impl Default for Doorbell {
    fn default() -> Self {
        Self::new()
    }
}

/// Doorbell page allocator
///
/// Provides functionality to allocate and map doorbell pages.
pub struct DoorbellManager {
    /// Next doorbell ID to allocate
    next_id: AtomicU32,
}

impl DoorbellManager {
    /// Create a new doorbell manager
    pub const fn new() -> Self {
        Self {
            next_id: AtomicU32::new(0),
        }
    }

    /// Allocate a new doorbell
    ///
    /// Returns the doorbell ID and kernel virtual address.
    /// The caller is responsible for mapping this to user space.
    pub fn allocate(
        &self,
        frame_allocator: &mut crate::kernel::mm::BootInfoFrameAllocator,
    ) -> Option<(u32, *mut Doorbell)> {
        use x86_64::structures::paging::FrameAllocator;

        // Allocate a physical frame for the doorbell
        let frame = frame_allocator.allocate_frame()?;

        // Get the kernel virtual address
        let phys_offset = crate::kernel::mm::PHYS_MEM_OFFSET.load(Ordering::Relaxed);
        let virt_addr = phys_offset + frame.start_address().as_u64();
        let doorbell_ptr = virt_addr as *mut Doorbell;

        // Initialize the doorbell
        unsafe {
            core::ptr::write(doorbell_ptr, Doorbell::new());
        }

        let id = self.next_id.fetch_add(1, Ordering::Relaxed);

        Some((id, doorbell_ptr))
    }

    /// Get the physical address of a doorbell for user-space mapping
    pub fn get_phys_addr(doorbell_ptr: *const Doorbell) -> u64 {
        let phys_offset = crate::kernel::mm::PHYS_MEM_OFFSET.load(Ordering::Relaxed);
        (doorbell_ptr as u64).wrapping_sub(phys_offset)
    }

    /// Free an allocated doorbell
    ///
    /// Deallocates the physical frame backing the given kernel virtual pointer.
    pub fn free(
        &self,
        doorbell_ptr: *const Doorbell,
        frame_allocator: &mut crate::kernel::mm::BootInfoFrameAllocator,
    ) {
        use x86_64::PhysAddr;
        use x86_64::structures::paging::PhysFrame;
        use x86_64::structures::paging::Size4KiB;

        let phys_addr = Self::get_phys_addr(doorbell_ptr);
        let frame = PhysFrame::<Size4KiB>::containing_address(PhysAddr::new(phys_addr));
        unsafe {
            frame_allocator.deallocate_frame(frame);
        }
    }
}

impl Default for DoorbellManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Global doorbell manager
static DOORBELL_MANAGER: DoorbellManager = DoorbellManager::new();

/// Get the global doorbell manager
pub fn manager() -> &'static DoorbellManager {
    &DOORBELL_MANAGER
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_doorbell_size() {
        assert_eq!(core::mem::size_of::<Doorbell>(), 4096);
    }

    #[test]
    fn test_doorbell_new() {
        let db = Doorbell::new();
        assert_eq!(db.ring.load(Ordering::Relaxed), 0);
        assert!(!db.needs_wakeup.load(Ordering::Relaxed));
        assert!(!db.cq_ready.load(Ordering::Relaxed));
        assert!(!db.sqpoll_running.load(Ordering::Relaxed));
    }

    #[test]
    fn test_ring_doorbell() {
        let db = Doorbell::new();
        db.ring_doorbell();
        assert_eq!(db.peek(), 1);
        db.ring_doorbell();
        assert_eq!(db.peek(), 2);
    }

    #[test]
    fn test_check_and_clear() {
        let db = Doorbell::new();
        db.ring_doorbell();
        db.ring_doorbell();
        db.ring_doorbell();

        let count = db.check_and_clear();
        assert_eq!(count, 3);
        assert_eq!(db.peek(), 0);
    }

    #[test]
    fn test_cq_ready() {
        let db = Doorbell::new();
        assert!(!db.check_cq_ready());
        db.set_cq_ready();
        assert!(db.check_cq_ready());
        db.clear_cq_ready();
        assert!(!db.check_cq_ready());
    }

    #[test]
    fn test_needs_wakeup() {
        let db = Doorbell::new();
        assert!(!db.check_needs_wakeup());
        db.set_needs_wakeup(true);
        assert!(db.check_needs_wakeup());
        db.set_needs_wakeup(false);
        assert!(!db.check_needs_wakeup());
    }

    #[test]
    fn test_reset() {
        let db = Doorbell::new();
        db.ring_doorbell();
        db.set_needs_wakeup(true);
        db.set_cq_ready();
        db.set_sqpoll_running(true);

        db.reset();

        assert_eq!(db.peek(), 0);
        assert!(!db.check_needs_wakeup());
        assert!(!db.check_cq_ready());
        assert!(!db.is_sqpoll_running());
    }
}
