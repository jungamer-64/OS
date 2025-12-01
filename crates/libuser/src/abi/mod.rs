//! Rust-Native ABI Definitions for User Space
//!
//! This module re-exports shared ABI types from the `rany_os_abi` crate
//! and provides userspace-specific extensions.

// Re-export all types from the shared ABI crate
pub use rany_os_abi::*;

// Re-export stdio module (userspace only)
#[cfg(feature = "userspace")]
pub use rany_os_abi::native::stdio;
