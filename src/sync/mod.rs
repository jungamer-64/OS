// src/sync/mod.rs

//! Synchronization primitives and lock management
//!
//! This module provides deadlock-prevention mechanisms through enforced lock ordering.
//!
//! # Lock Ordering
//!
//! Locks must be acquired in a specific order to prevent deadlocks:
//! 1. VGA buffer lock
//! 2. Serial port lock
//! 3. Other system locks
//!
//! # Example
//!
//! ```no_run
//! use tiny_os::sync::{acquire_lock, LockId};
//!
//! // Acquire locks in the correct order
//! let vga_lock = acquire_lock(LockId::Vga)?;
//! let serial_lock = acquire_lock(LockId::Serial)?;
//! // ... use locks ...
//! // Locks are automatically released when dropped
//! # Ok::<(), tiny_os::sync::lock_manager::LockOrderViolation>(())
//! ```
//!
//! # Safety
//!
//! Lock ordering is enforced at runtime. Attempting to acquire locks out of order
//! will result in a `LockOrderViolation` error.

pub mod lock_manager;
pub mod interrupt;

// Re-export commonly used types
pub use lock_manager::{acquire_lock, lock_stats, record_contention, LockId, LockStats};
