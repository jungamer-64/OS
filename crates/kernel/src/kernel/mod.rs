// kernel/src/kernel/mod.rs
//! カーネル抽象化
//!
//! このモジュールは、カーネル全体で使用する基本的な trait、型、
//! エラーハンドリング機構を提供します。
//!
//! # モジュール構成
//!
//! - `core`: 基本的なカーネル機能
//! - `capability`: Capability-based セキュリティモデル (V2)
//! - `io_uring`: 非同期 I/O リングバッファ
//! - `process`: プロセス管理
//! - `syscall`: システムコールハンドラ
//! - `scheduler`: タスクスケジューラ

pub mod core;
pub mod driver;
pub mod mmio;
pub mod mm;
pub mod task;
pub mod r#async;
// pub mod shell;
pub mod bench;
pub mod syscall;
pub mod fs;
pub mod process;
pub mod scheduler;
pub mod security;  // Phase 3: Security module
// pub mod usermode;
pub mod loader;
pub mod io_uring;  // io_uring-style async I/O
pub mod capability; // Next-gen capability-based resource management
