//! Rust-Native ABI Definitions for User Space
//!
//! This module mirrors the kernel's `abi::native` module, providing
//! the same type-safe ABI types for user-space programs.
//!
//! These types are designed for the next-generation syscall interface.

pub mod native;
pub mod error;
pub mod result;

pub use native::{
    SyscallNumber, SyscallCategory, ResourceId, ResourceMarker,
    FileMarker, SocketMarker, PipeMarker, BufferMarker, DirectoryMarker,
    Handle, FileHandle, SocketHandle, PipeHandle, BufferHandle, DirectoryHandle,
};

pub use error::{SyscallError, ErrorCategory};
pub use result::{SyscallResult, AbiResult};
