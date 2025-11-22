//! 非同期タイマー
//!
//! 指定時間後に完了する Future。
//! ハードウェア割り込み（PIT）と連携して動作します。

use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll, Waker};
use core::sync::atomic::{AtomicU64, Ordering};
use alloc::vec::Vec;
use spin::Mutex;

/// グローバルティックカウンタ (10ms 単位)
pub static TICKS: AtomicU64 = AtomicU64::new(0);

/// タイマー待機中の Waker リスト
/// (deadline, waker) のペア
static WAKERS: Mutex<Vec<(u64, Waker)>> = Mutex::new(Vec::new());

/// 割り込みハンドラから呼ばれる関数
/// ティックを更新し、期限が来たタスクを起こす
pub fn tick() {
    let current_ticks = TICKS.fetch_add(1, Ordering::Relaxed);
    let mut wakers = WAKERS.lock();
    
    // 期限が来た Waker を取り出して起こす
    // retain を使って、まだ期限が来ていないものだけ残す
    wakers.retain(|(deadline, waker)| {
        if *deadline <= current_ticks + 1 {
            waker.wake_by_ref();
            false // リストから削除
        } else {
            true // リストに残す
        }
    });
}

/// 非同期タイマー
pub struct Timer {
    deadline: Option<u64>,
    duration_ms: u64,
}

impl Timer {
    /// 新しいタイマーを作成
    pub fn new(duration_ms: u64) -> Self {
        Self {
            deadline: None,
            duration_ms,
        }
    }

    /// 指定時間後に完了
    pub fn after(duration_ms: u64) -> Self {
        Self::new(duration_ms)
    }
}

impl Future for Timer {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // 初回ポーリング時に期限を設定
        if self.deadline.is_none() {
            let current_ticks = TICKS.load(Ordering::Relaxed);
            // 1 tick = 10ms (100Hz)
            let ticks_needed = self.duration_ms / 10;
            self.deadline = Some(current_ticks + ticks_needed);
        }

        // Safety: deadline is always Some after the check above
        // Using if-let for additional safety and clarity
        if let Some(deadline) = self.deadline {
            let current_ticks = TICKS.load(Ordering::Relaxed);

            if current_ticks >= deadline {
                Poll::Ready(())
            } else {
                // Waker を登録
                let mut wakers = WAKERS.lock();
                wakers.push((deadline, cx.waker().clone()));
                Poll::Pending
            }
        } else {
            // This should never happen due to the check above, but handle it safely
            // Set deadline now and return Pending to poll again
            let current_ticks = TICKS.load(Ordering::Relaxed);
            let ticks_needed = self.duration_ms / 10;
            self.deadline = Some(current_ticks + ticks_needed);
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

/// 非同期的に yield する
pub struct Yield {
    yielded: bool,
}

impl Yield {
    pub fn new() -> Self {
        Self { yielded: false }
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

pub fn yield_now() -> Yield {
    Yield::new()
}
