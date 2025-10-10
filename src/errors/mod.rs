// src/errors/mod.rs

//! Unified error handling module
//!
//! This module provides both legacy error types (for backward compatibility)
//! and unified error types (for new code).

pub mod unified;

// Re-export unified types for new code
pub use unified::{
    DisplayError as UnifiedDisplayError, ErrorContext, InitError as UnifiedInitError,
    KernelError as UnifiedKernelError, Result as UnifiedResult, SerialError as UnifiedSerialError,
    VgaError as UnifiedVgaError,
};

// Legacy error types remain available via their original paths
// (vga_buffer::writer::VgaError, init::InitError, etc.)
