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

pub mod io_uring;

pub use io_uring::{
    SubmissionEntry, CompletionEntry, RingHeader,
    OpCode, IoUringFlags, RING_SIZE, RING_MASK,
};
