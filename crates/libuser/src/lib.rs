//! Userland library for Tiny OS
//!
//! `libuser` provides system call wrappers and high-level APIs for user programs
//! running in Ring 3 (user mode) on Tiny OS.
//!
//! # Architecture
//!
//! This library is designed to be the **only** interface between user programs
//! and the kernel. It provides:
//!
//! - **Low-level system call wrappers** ([`syscall`] module)
//! - **High-level I/O functions** ([`io`] module)
//! - **Process management**  ([`process`] module)
//! - **Memory management** ([`mem`] module)
//!
//! # Design Principles
//!
//! 1. **Type Safety**: All APIs use Rust's type system to prevent errors
//! 2. **Error Handling**: System calls return `Result` types for proper error handling
//! 3. **No Dependencies**: Pure `no_std` library with no external dependencies
//! 4. **Zero Cost**: Abstractions compile down to direct system calls
//!
//! # Usage
//!
//! Add `libuser` as a dependency in your `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! libuser = { path = "../../userland/libuser" }
//! ```
//!
//! Then use the high-level APIs:
//!
//! ```no_run
//! use libuser::{println, process};
//!
//! fn main() {
//!     println!("Hello from userland!");
//!    
//!     let pid = process::getpid();
//!     println!("My PID is:");
//!     // Note: println! only supports string literals in no_std
//! }
//! ```
//!
//! # System Call Interface
//!
//! For direct system call access, use the [`syscall`] module:
//!
//! ```no_run
//! use libuser::syscall;
//!
//! let result = syscall::write(1, b"Hello\n");
//! match result {
//!     Ok(n) => { /* wrote n bytes */ }
//!     Err(e) => { /* handle error */ }
//! }
//! ```
//!
//! # Error Handling
//!
//! All system calls that can fail return `SyscallResult<T>`, which is a type alias
//! for `Result<T, SyscallError>`:
//!
//! ```no_run
//! use libuser::process::fork;
//! use libuser::syscall::errno;
//!
//! match fork() {
//!     Ok(0) => { /* child process */ }
//!     Ok(pid) => { /* parent process, pid = child */ }
//!     Err(e) => {
//!         // Check specific error
//!         if e.is(errno::ENOMEM) {
//!             // Out of memory
//!         }
//!         // Or get description
//!         let desc = e.description();
//!     }
//! }
//! ```
//!
//! # Module Overview
//!
//! ## [`syscall`]
//!
//! Low-level system call interface. Use this when you need direct control or
//! when higher-level APIs don't provide the functionality you need.
//!
//! ## [`io`]
//!
//! High-level I/O functions:
//! - `print()`, `println()` - Output to stdout
//! - `eprint()`, `eprintln()` - Output to stderr  
//! - `read()`, `write()` - File descriptor I/O
//!
//! ## [`process`]
//!
//! Process management:
//! - `exit()` - Terminate process
//! - `getpid()` - Get process ID
//! - `fork()` - Create child process
//! - `exec()` - Replace process image
//! - `wait()` - Wait for child termination
//! - `spawn()` - Convenient fork+exec
//!
//! ## [`mem`]
//!
//! Memory management:
//! - `alloc()`, `dealloc()` - Simple allocation
//! - `mmap()` - Flexible memory mapping
//! - `MemoryRegion` - RAII memory handle
//!
//! ## [`alloc`]
//!
//! Global allocator for userland programs:
//! - `MmapAllocator` - Uses mmap/munmap for heap allocation
//! - Automatic integration with Rust's `alloc` crate
//!
//! ## [`constants`]
//!
//! System-wide constants:
//! - `PAGE_SIZE` - Standard page size (4KB)
//! - Address range constants
//! - Exit codes
//!
//! ## [`util`]
//!
//! Utility functions:
//! - Error handling helpers
//! - Convenient macros for syscall results

#![no_std]
#![feature(alloc_error_handler)]
#![deny(unsafe_op_in_unsafe_fn)]
#![warn(missing_docs)]
#![doc(html_root_url = "https://docs.rs/libuser/0.1.0")]

pub mod abi;
pub mod syscall;
pub mod io;
pub mod process;
pub mod mem;
pub mod alloc;
pub mod constants;
pub mod util;
pub mod sync;
// pub mod io_uring; // Removed legacy V1 API
// pub mod async_io; // Removed legacy V1 API
pub mod ring_io;
// pub mod testing;  // TODO: Fix compilation
// pub mod debug;    // TODO: Fix compilation

// Re-export commonly used items for convenience
pub use io::{print, println};
pub use process::{exit, getpid};
pub use syscall::{SyscallResult, SyscallError};
pub use constants::PAGE_SIZE;

// Re-export ABI types for next-gen syscall interface
pub use abi::{
    SyscallNumber, SyscallCategory, ResourceId, ResourceMarker,
    FileHandle, SocketHandle, PipeHandle, BufferHandle, DirectoryHandle,
};
