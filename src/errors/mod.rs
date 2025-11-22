// src/errors/mod.rs

//! Unified error handling module
//!
//! This module provides a consistent error handling approach across
//! all kernel subsystems.
//!
//! # Usage
//!
//! Use the unified error types:
//!
//! ```no_run
//! use tiny_os::errors::{UnifiedResult, UnifiedKernelError};
//!
//! fn my_function() -> UnifiedResult<()> {
//!     // Your code here
//!     Ok(())
//! }
//! ```

pub mod unified;

// Re-export unified types
pub use unified::{
    DisplayError as UnifiedDisplayError, ErrorContext, InitError as UnifiedInitError,
    KernelError as UnifiedKernelError, Result as UnifiedResult, SerialError as UnifiedSerialError,
    VgaError as UnifiedVgaError,
};
