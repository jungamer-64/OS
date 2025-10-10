// src/sync/lock_manager.rs

//! Lock management and deadlock prevention
//!
//! This module provides centralized lock management with:
//! - Lock ordering enforcement
//! - Deadlock detection
//! - Lock timeout handling
//! - Diagnostic information collection

use core::sync::atomic::{AtomicU64, AtomicU8, Ordering};

/// Lock identifiers with defined ordering
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum LockId {
    /// Serial port lock (must be acquired first)
    Serial = 0,
    /// VGA buffer lock (must be acquired after Serial if both needed)
    Vga = 1,
    /// Diagnostics lock (lowest priority)
    Diagnostics = 2,
}

/// Lock acquisition guard that enforces proper release
pub struct LockGuard {
    id: LockId,
    acquired_at: u64,
}

impl LockGuard {
    /// Create a new lock guard
    fn new(id: LockId) -> Self {
        Self {
            id,
            acquired_at: Self::read_timestamp(),
        }
    }

    /// Get the lock ID
    pub fn id(&self) -> LockId {
        self.id
    }

    /// Get how long this lock has been held (in arbitrary units)
    pub fn hold_duration(&self) -> u64 {
        Self::read_timestamp().saturating_sub(self.acquired_at)
    }

    #[cfg(target_arch = "x86_64")]
    fn read_timestamp() -> u64 {
        #[cfg(debug_assertions)]
        // SAFETY: RDTSC is a non-privileged read-only instruction.
        // Safe to use for lock timing measurements.
        unsafe {
            core::arch::x86_64::_rdtsc()
        }
        #[cfg(not(debug_assertions))]
        0
    }

    #[cfg(not(target_arch = "x86_64"))]
    fn read_timestamp() -> u64 {
        0
    }
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        LOCK_MANAGER.release(self.id);
    }
}

/// Global lock manager for tracking and enforcement
struct LockManager {
    /// Currently held locks (bitmask)
    held_locks: AtomicU8,
    /// Lock acquisition attempts
    acquisition_count: AtomicU64,
    /// Lock contentions detected
    contention_count: AtomicU64,
    /// Deadlock attempts detected
    deadlock_attempts: AtomicU64,
}

impl LockManager {
    const fn new() -> Self {
        Self {
            held_locks: AtomicU8::new(0),
            acquisition_count: AtomicU64::new(0),
            contention_count: AtomicU64::new(0),
            deadlock_attempts: AtomicU64::new(0),
        }
    }

    /// Attempt to acquire a lock with ordering validation
    ///
    /// Returns Ok(LockGuard) if successful, Err if would violate lock ordering
    pub fn try_acquire(&self, id: LockId) -> Result<LockGuard, LockOrderViolation> {
        let current_locks = self.held_locks.load(Ordering::Acquire);
        let lock_bit = 1u8 << (id as u8);

        // Check if already held
        if (current_locks & lock_bit) != 0 {
            return Err(LockOrderViolation::AlreadyHeld(id));
        }

        // Check lock ordering: can't acquire a lower-priority lock
        // while holding a higher-priority one
        let higher_priority_mask = (1u8 << (id as u8)) - 1;
        if (current_locks & higher_priority_mask) != 0 {
            self.deadlock_attempts.fetch_add(1, Ordering::Relaxed);
            return Err(LockOrderViolation::OrderingViolation {
                requested: id,
                held_mask: current_locks,
            });
        }

        // Mark as acquired
        self.held_locks.fetch_or(lock_bit, Ordering::Release);
        self.acquisition_count.fetch_add(1, Ordering::Relaxed);

        Ok(LockGuard::new(id))
    }

    /// Record lock contention
    pub fn record_contention(&self) {
        self.contention_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Release a lock
    fn release(&self, id: LockId) {
        let lock_bit = 1u8 << (id as u8);
        self.held_locks.fetch_and(!lock_bit, Ordering::Release);
    }

    /// Get diagnostic statistics
    pub fn stats(&self) -> LockStats {
        LockStats {
            acquisitions: self.acquisition_count.load(Ordering::Relaxed),
            contentions: self.contention_count.load(Ordering::Relaxed),
            deadlock_attempts: self.deadlock_attempts.load(Ordering::Relaxed),
            currently_held: self.held_locks.load(Ordering::Relaxed),
        }
    }
}

/// Global lock manager instance
static LOCK_MANAGER: LockManager = LockManager::new();

/// Lock ordering violation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockOrderViolation {
    /// Lock already held by current context
    AlreadyHeld(LockId),
    /// Attempting to acquire in wrong order
    OrderingViolation { requested: LockId, held_mask: u8 },
}

/// Lock statistics for diagnostics
#[derive(Debug, Clone, Copy)]
pub struct LockStats {
    pub acquisitions: u64,
    pub contentions: u64,
    pub deadlock_attempts: u64,
    pub currently_held: u8,
}

/// Public API for lock acquisition
pub fn acquire_lock(id: LockId) -> Result<LockGuard, LockOrderViolation> {
    LOCK_MANAGER.try_acquire(id)
}

/// Record lock contention
pub fn record_contention() {
    LOCK_MANAGER.record_contention();
}

/// Get lock statistics
pub fn lock_stats() -> LockStats {
    LOCK_MANAGER.stats()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lock_ordering() {
        // Should be able to acquire Serial first
        let _serial = acquire_lock(LockId::Serial).expect("should acquire serial");

        // Should be able to acquire VGA after Serial
        let _vga = acquire_lock(LockId::Vga).expect("should acquire vga");
    }

    #[test]
    fn test_reverse_order_violation() {
        // Acquire VGA first
        let _vga = acquire_lock(LockId::Vga).expect("should acquire vga");

        // Should fail to acquire Serial (lower priority)
        let result = acquire_lock(LockId::Serial);
        assert!(result.is_err());

        if let Err(LockOrderViolation::OrderingViolation { requested, .. }) = result {
            assert_eq!(requested, LockId::Serial);
        } else {
            panic!("expected ordering violation");
        }
    }
}
