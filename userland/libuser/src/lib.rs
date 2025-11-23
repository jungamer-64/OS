//! Userland library for Tiny OS
//!
//! This library provides system call wrappers and high-level APIs
//! for user programs running in Ring 3.

#![no_std]
#![deny(unsafe_op_in_unsafe_fn)]

pub mod syscall;
pub mod io;
pub mod process;
pub mod mem;

// Re-export commonly used items
pub use io::{print, println};
pub use process::{exit, getpid};
