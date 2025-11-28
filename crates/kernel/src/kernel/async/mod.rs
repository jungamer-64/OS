//! 非同期処理基盤
//!
//! Future executor, Waker, 非同期 I/O の基本構造を提供します。
//!
//! # Overview
//!
//! このモジュールは、カーネル内で非同期処理を行うための基盤を提供します。
//! `crossbeam-queue` を使用したロックフリーキューにより、効率的なタスク管理を実現。
//!
//! # Usage
//!
//! ```ignore
//! use crate::kernel::r#async::{spawn_task, Timer};
//!
//! // タスクを追加
//! spawn_task(async {
//!     debug_println!("Task started!");
//!     Timer::after(100).await;  // 100ms 待機
//!     debug_println!("Task completed!");
//! });
//!
//! // カーネルのアイドルループで実行
//! while let Some(_) = poll_runtime() {
//!     // タスクを処理中
//! }
//! ```

pub mod executor;
pub mod waker;
pub mod timer;

pub use executor::{
    Executor, 
    Task, 
    TaskId, 
    RUNTIME,
    spawn_task, 
    poll_runtime, 
    run_runtime_until_idle,
    runtime_task_count,
};
pub use waker::{dummy_waker, WakerBuilder};
pub use timer::{Timer, Yield, yield_now, TICKS, tick};
