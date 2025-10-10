// src/sync/mod.rs

//! Synchronization primitives and lock management

pub mod lock_manager;

// Re-export commonly used types
pub use lock_manager::{acquire_lock, lock_stats, record_contention, LockId, LockStats};
