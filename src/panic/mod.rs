//! Panic handling
//!
//! シンプルな panic 処理モジュール

pub mod state;

pub use state::{current_level, enter_panic, is_panicking, PanicLevel};
