// src/errors/mod.rs

//! Unified error handling module
//!
//! This module provides both legacy error types (for backward compatibility)
//! and unified error types (for new code).
//!
//! # Migration Guide
//!
//! ## For New Code
//!
//! Use the unified error types with the `Unified` prefix:
//!
//! ```no_run
//! use tiny_os::errors::{UnifiedResult, UnifiedKernelError};
//!
//! fn my_function() -> UnifiedResult<()> {
//!     // Your code here
//!     Ok(())
//! }
//! ```
//!
//! ## For Legacy Code
//!
//! Legacy error types remain available through their original paths:
//! - `vga_buffer::writer::VgaError`
//! - `init::InitError`
//! - `serial::SerialError`
//!
//! # Error Conversion
//!
//! All legacy error types implement `Into<UnifiedKernelError>` for seamless
//! migration to the unified error handling system.

pub mod unified;

// Re-export unified types for new code
pub use unified::{
    DisplayError as UnifiedDisplayError, ErrorContext, InitError as UnifiedInitError,
    KernelError as UnifiedKernelError, Result as UnifiedResult, SerialError as UnifiedSerialError,
    VgaError as UnifiedVgaError,
};

// Legacy error types remain available via their original paths
// (vga_buffer::writer::VgaError, init::InitError, etc.)
