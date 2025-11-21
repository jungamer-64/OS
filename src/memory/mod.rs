// src/memory/mod.rs

//! Memory subsystem primitives and abstractions.
//!
//! The memory module exposes reusable building blocks for safe, low-level
//! operations. It is intentionally lightweight so that other subsystems like
//! VGA handling or device drivers can share the same contracts when dealing
//! with raw pointers.

pub mod access;
pub mod safety;

pub use access::{MemoryAccess, MemoryAccessExt, SliceMemoryAccess};
pub use safety::{BufferError, MemoryRegion, SafeBuffer};
