// src/kernel/mod.rs
//! カーネル抽象化
//! 
//! このモジュールは、カーネル全体で使用する基本的な trait、型、
pub mod core;
pub mod driver;
pub mod mmio;
pub mod mm;
pub mod task;
pub mod r#async;
pub mod shell;
pub mod bench;
