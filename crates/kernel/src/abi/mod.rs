// kernel/src/abi/mod.rs
//! Application Binary Interface (ABI) definitions
//!
//! This module re-exports shared ABI types from the `rany_os_abi` crate
//! and provides kernel-specific extensions.

// Re-export all types from the shared ABI crate
pub use rany_os_abi::*;
