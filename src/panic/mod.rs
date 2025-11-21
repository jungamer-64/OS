// src/panic/mod.rs

//! Panic handling utilities
//!
//! This module provides panic state tracking and nested panic detection.
//!
//! # Panic Levels
//!
//! The kernel supports multiple panic levels:
//! - **`NotPanicking`**: Normal operation
//! - **`Panicking`**: Initial panic state
//! - **`NestedPanic`**: Panic occurred while handling another panic (critical)
//!
//! # Usage
//!
//! ```no_run
//! use tiny_os::panic::{is_panicking, current_level};
//!
//! if is_panicking() {
//!     // Take emergency action
//!     // Do NOT allocate or use complex operations
//! }
//! ```
//!
//! # Safety
//!
//! During a panic, many normal operations become unsafe:
//! - No allocations
//! - No lock acquisitions (deadlock risk)
//! - Minimal stack usage
//! - Emergency output only
//!
//! Nested panics are especially dangerous and will trigger emergency shutdown.

pub mod handler;
pub mod state;

// Re-export commonly used types
pub use handler::{handle_panic, PanicOutputStatus};
pub use state::{current_level, enter_panic, is_panicking, PanicLevel};
