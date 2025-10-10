// src/panic/mod.rs

//! Panic handling utilities
//!
//! This module provides panic state tracking and nested panic detection.

pub mod state;

// Re-export commonly used types
pub use state::{PanicLevel, enter_panic, current_level, is_panicking};
