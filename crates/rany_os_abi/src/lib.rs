//! RanY OS Shared ABI Definitions
//!
//! This crate provides type-safe Application Binary Interface (ABI) definitions
//! shared between the kernel and userspace programs.
//!
//! # Architecture
//!
//! The io_uring style interface uses two ring buffers:
//! - **Submission Queue (SQ)**: User writes requests, kernel reads
//! - **Completion Queue (CQ)**: Kernel writes results, user reads
//!
//! This design allows batching of I/O operations, reducing syscall overhead
//! from O(n) to O(1) for n operations.
//!
//! # Modules
//!
//! - [`error`]: Type-safe syscall error types
//! - [`native`]: Rust-native syscall numbers and handle types
//! - [`result`]: ABI-safe Result types
//! - [`io_uring_common`]: Common io_uring constants and opcodes
//! - [`io_uring_v2`]: V2 io_uring entry structures

#![no_std]
#![warn(missing_docs)]
#![deny(unsafe_op_in_unsafe_fn)]

pub mod error;
pub mod io_uring_common;
pub mod io_uring_v2;
pub mod native;
pub mod result;

// Re-export commonly used types
pub use error::{ErrorCategory, SyscallError, SyscallResult};
pub use io_uring_common::{IoUringFlags, OpCode, RING_MASK, RING_SIZE};
pub use io_uring_v2::{CompletionEntryV2, RingHeaderV2, SubmissionEntryV2, V2Features};
pub use native::{
    BufferHandle, BufferMarker, DirectoryHandle, DirectoryMarker, FileHandle, FileMarker, Handle,
    PipeHandle, PipeMarker, ResourceId, ResourceMarker, SocketHandle, SocketMarker,
    SyscallCategory, SyscallNumber,
};
pub use result::{AbiResult, AbiResultI32, AbiResultI64, AbiResultU64, AbiResultUnit, AbiResultUsize, CompactResult};
