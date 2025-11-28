// crates/kernel/src/kernel/async/mod.rs
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
//! use crate::kernel::r#async::{spawn_task, Timer, read_key_async};
//!
//! // タスクを追加
//! spawn_task(async {
//!     debug_println!("Task started!");
//!     Timer::after(100).await;  // 100ms 待機
//!     debug_println!("Task completed!");
//! });
//!
//! // 非同期キーボード入力
//! spawn_task(async {
//!     loop {
//!         if let Some(ch) = read_key_async().await {
//!             debug_println!("Key pressed: {}", ch);
//!         }
//!     }
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
pub mod io_uring_future;

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
pub use timer::{Timer, Yield, yield_now, TICKS, tick, timer_task, current_ticks, ticks_to_ms, ms_to_ticks};
pub use io_uring_future::{
    IoUringOp, IoUringFuture,
    submit_async, write_async, read_async, close_async,
    mmap_async, munmap_async,
    complete_operation,
};

// ============================================================================
// 非同期キーボード入力ヘルパー
// ============================================================================

/// 非同期でキー入力を待ち、ASCII文字に変換して返す
///
/// キーのリリースイベント（最上位ビットが1）は無視し、
/// 変換可能なキーのみを返します。
///
/// # Returns
/// - `Some(char)` - ASCII に変換可能なキーが押された
/// - `None` - 変換不可能なキー（Shift, Ctrl など）
///
/// # Example
/// ```ignore
/// spawn_task(async {
///     loop {
///         if let Some(ch) = read_key_async().await {
///             print!("{}", ch);
///         }
///     }
/// });
/// ```
pub async fn read_key_async() -> Option<char> {
    use crate::kernel::driver::keyboard::{ScancodeStream, KeyCode};
    
    loop {
        let scancode = ScancodeStream::new().await;
        
        // キーリリースイベントは無視（最上位ビットが1）
        if scancode & 0x80 != 0 {
            continue;
        }
        
        // スキャンコードをキーコードに変換
        if let Some(keycode) = KeyCode::from_scancode(scancode) {
            // ASCII に変換（Shift なし）
            if let Some(ch) = keycode.to_ascii(false) {
                return Some(ch);
            }
        }
        
        // 変換不可能なキーは None を返す
        return None;
    }
}

/// 非同期でキー入力を待ち、スキャンコードをそのまま返す
///
/// キーの押下・リリースを区別して処理したい場合に使用。
///
/// # Returns
/// - スキャンコード（0-127: 押下, 128-255: リリース）
pub async fn read_scancode_async() -> u8 {
    use crate::kernel::driver::keyboard::ScancodeStream;
    ScancodeStream::new().await
}

/// 非同期で行入力を待つ
///
/// Enter キーが押されるまで入力を受け付け、文字列として返す。
/// Backspace でカーソルを戻す。最大 `max_len` 文字まで。
///
/// # Example
/// ```ignore
/// spawn_task(async {
///     let line = read_line_async(256).await;
///     debug_println!("Input: {}", line);
/// });
/// ```
pub async fn read_line_async(max_len: usize) -> alloc::string::String {
    use alloc::string::String;
    
    let mut buffer = String::with_capacity(max_len);
    
    loop {
        if let Some(ch) = read_key_async().await {
            match ch {
                '\n' => {
                    // Enter: 入力完了
                    return buffer;
                }
                '\x08' | '\x7f' => {
                    // Backspace: 最後の文字を削除
                    buffer.pop();
                }
                _ if buffer.len() < max_len => {
                    // 通常の文字: バッファに追加
                    buffer.push(ch);
                }
                _ => {
                    // バッファフル: 無視
                }
            }
        }
    }
}
