// kernel/src/abi/mod.rs
//! Application Binary Interface (ABI) definitions
//!
//! This module defines shared data structures between user space and kernel space
//! for the io_uring-style asynchronous I/O mechanism.
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
//! # V2 Architecture (Next Generation)
//!
//! The V2 ABI introduces:
//! - **Capability-based resources**: Type-safe handles instead of integer FDs
//! - **Typed errors**: `SyscallError` enum instead of errno
//! - **ABI-safe Result**: `AbiResult<T, E>` for safe Result passing
//! - **Registered buffers only**: No raw pointers in V2 mode

// Next-generation ABI (V2)
pub mod error;
pub mod io_uring_common;
pub mod io_uring_v2;
pub mod native;
pub mod result;

// Re-export V1 types for compatibility
pub use io_uring_common::{
    IoUringFlags, OpCode, RING_MASK, RING_SIZE,
};

// Re-export V2 types
pub use error::{ErrorCategory, SyscallError, SyscallResult};
pub use io_uring_v2::{
    CompletionEntryV2, RingHeaderV2, SubmissionEntryV2, V2Features,
};
pub use native::{
    BufferHandle, BufferMarker, DirectoryHandle, DirectoryMarker, FileHandle, FileMarker, Handle,
    PipeHandle, PipeMarker, ResourceId, ResourceMarker, SocketHandle, SocketMarker, SyscallCategory,
    SyscallNumber,
};
pub use result::{AbiResult, AbiResultI32, AbiResultI64, AbiResultU64, AbiResultUnit, AbiResultUsize, CompactResult};
