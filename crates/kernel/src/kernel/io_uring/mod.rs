// kernel/src/kernel/io_uring/mod.rs
//! io_uring-style asynchronous I/O subsystem
//!
//! This module implements a high-performance, batched I/O mechanism inspired by
//! Linux's io_uring. Instead of one syscall per operation, users can submit
//! multiple operations at once and collect results asynchronously.
//!
//! # Architecture
//!
//! ```text
//! User Space                          Kernel Space
//! ┌─────────────────┐                ┌─────────────────┐
//! │   Application   │                │    Executor     │
//! │                 │                │                 │
//! │ ┌─────────────┐ │  shared mem    │ ┌─────────────┐ │
//! │ │ SQ (Submit) │◄┼────────────────┼►│  SQ Reader  │ │
//! │ └─────────────┘ │                │ └──────┬──────┘ │
//! │                 │                │        │        │
//! │ ┌─────────────┐ │                │        ▼        │
//! │ │ CQ (Complet)│◄┼────────────────┼─│  Handlers   │ │
//! │ └─────────────┘ │                │ └──────┬──────┘ │
//! │                 │                │        │        │
//! │   syscall       │                │        ▼        │
//! │   (doorbell)  ──┼───────────────►│  CQ Writer    │ │
//! └─────────────────┘                └─────────────────┘
//! ```
//!
//! # Performance Benefits
//!
//! - **Batching**: Submit 100s of operations with one syscall
//! - **Zero-syscall mode**: Kernel can poll SQ without syscall (SQPOLL)
//! - **Registered buffers**: Pre-validated memory for zero-copy I/O
//! - **Cache efficiency**: Sequential memory access patterns
//! - **Lock-free**: Atomic operations for synchronization
//!
//! # Security Model
//!
//! - All user pointers are validated before access
//! - SQEs are copied to kernel memory before processing (TOCTOU protection)
//! - Operations are executed with process credentials
//! - Resource limits are enforced
//! - Registered buffers are validated once at registration

pub mod ring;
pub mod handlers_v2;
pub mod context;
pub mod sqpoll;
pub mod registered_buffers;
pub mod doorbell;

pub use ring::{IoUring, IoUringError};
pub use context::IoUringContext;
pub use sqpoll::{SqPollConfig, SqPollState, SqPollStats};
pub use registered_buffers::{RegisteredBufferTable, RegisteredBufferStats};
pub use handlers_v2::dispatch_sqe_v2;
pub use doorbell::{Doorbell, DoorbellManager};

use crate::abi::io_uring::{RING_SIZE, RING_MASK};

/// Initialize the io_uring subsystem
pub fn init() {
    sqpoll::init();
    crate::debug_println!("[io_uring] Subsystem initialized");
}
