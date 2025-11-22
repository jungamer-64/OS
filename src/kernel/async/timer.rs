//! 非同期タイマー
//!
//! 指定時間後に完了する Future。

use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

/// 非同期タイマー
///
/// 現在は即座に完了する簡易実装。
/// 将来、割り込みタイマーと連携して実際の遅延を実装する。
pub struct Timer {
    duration_ms: u64,
    started: bool,
}

impl Timer {
    /// 新しいタイマーを作成
    pub fn new(duration_ms: u64) -> Self {
        Self {
            duration_ms,
            started: false,
        }
    }

    /// 指定時間後に完了
    pub fn after(duration_ms: u64) -> Self {
        Self::new(duration_ms)
    }
}

impl Future for Timer {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        if !self.started {
            self.started = true;
            // TODO: ハードウェアタイマーとの統合
            // 現在は即座に完了
            Poll::Ready(())
        } else {
            Poll::Ready(())
        }
    }
}

/// 非同期的に yield する
///
/// 他のタスクに実行を譲る。
pub struct Yield {
    yielded: bool,
}

impl Yield {
    pub fn new() -> Self {
        Self { yielded: false }
    }
}

impl Default for Yield {
    fn default() -> Self {
        Self::new()
    }
}

impl Future for Yield {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if !self.yielded {
            self.yielded = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        } else {
            Poll::Ready(())
        }
    }
}

/// 非同期的に yield する（関数）
pub fn yield_now() -> Yield {
    Yield::new()
}
