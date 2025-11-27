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
//! - **Zero-syscall mode**: Kernel can poll SQ without syscall
//! - **Cache efficiency**: Sequential memory access patterns
//! - **Lock-free**: Atomic operations for synchronization
//!
//! # Security Model
//!
//! - All user pointers are validated before access
//! - SQEs are copied to kernel memory before processing (TOCTOU protection)
//! - Operations are executed with process credentials
//! - Resource limits are enforced

pub mod ring;
pub mod handlers;
pub mod context;

pub use ring::{IoUring, IoUringError};
pub use context::IoUringContext;

use crate::abi::io_uring::{SubmissionEntry, CompletionEntry, RingHeader, OpCode, RING_SIZE, RING_MASK};
