// crates/kernel/src/kernel/async/timer.rs
//! 非同期タイマー
//!
//! 指定時間後に完了する Future。
//! ハードウェア割り込み（PIT）と連携して動作します。
//!
//! # デッドロック回避設計
//!
//! 割り込みハンドラ（`tick()`）からのロック取得はデッドロックの原因となるため、
//! 以下の設計を採用しています：
//!
//! 1. `tick()` - 割り込みハンドラから呼ばれ、アトミックカウンタのみを更新（ロックフリー）
//! 2. `timer_task()` - システムタスクとして動作し、安全なコンテキストでWakerを処理
//!
//! ```text
//! Timer Interrupt → tick() → TICKS++
//!                              ↓
//! timer_task() ← poll ← Executor ← kernel_idle()
//!      ↓
//!   WAKERS.lock() (安全)
//!      ↓
//!   wake expired timers
//! ```

use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll, Waker};
use core::sync::atomic::{AtomicU64, AtomicBool, Ordering};
use alloc::vec::Vec;
use spin::Mutex;

/// グローバルティックカウンタ (10ms 単位)
/// 
/// 割り込みハンドラからアトミックに更新される。
pub static TICKS: AtomicU64 = AtomicU64::new(0);

/// タイマー待機中の Waker リスト
/// (deadline, waker) のペア
/// 
/// 注意: 割り込みハンドラからは絶対にロックしてはいけない！
/// `timer_task()` からのみ安全にロックされる。
static WAKERS: Mutex<Vec<(u64, Waker)>> = Mutex::new(Vec::new());

/// 新しいタイマーイベントがあることを示すフラグ
/// 
/// Timer::poll() で Waker が登録されたときに true になり、
/// timer_task() が処理したときに false になる。
static HAS_PENDING_TIMERS: AtomicBool = AtomicBool::new(false);

/// 割り込みハンドラから呼ばれる関数（ロックフリー）
///
/// ティックカウンタをインクリメントするだけ。
/// Wakerの処理は `timer_task()` に委譲する。
///
/// # Safety
/// この関数は割り込みハンドラから呼ばれるため、
/// いかなるロックも取得してはならない。
#[inline]
pub fn tick() {
    TICKS.fetch_add(1, Ordering::Relaxed);
    // ここで WAKERS をロックしてはいけない！
}

/// タイマー管理タスク
///
/// システム起動時に spawn され、期限切れタイマーの Waker を起こす。
/// 安全なコンテキスト（Executor）で動作するため、ロックが安全。
///
/// # Example
/// ```ignore
/// // カーネル起動時
/// spawn_task(timer_task());
/// ```
pub async fn timer_task() {
    let mut last_tick = 0u64;
    
    crate::debug_println!("[Timer] Timer task started");
    
    loop {
        let current = TICKS.load(Ordering::Relaxed);
        
        // ティックが進んだか、ペンディングタイマーがある場合に処理
        if current > last_tick || HAS_PENDING_TIMERS.load(Ordering::Relaxed) {
            last_tick = current;
            
            // 安全なコンテキストでロックを取得
            let mut wakers = WAKERS.lock();
            
            // 期限切れの Waker を収集して起こす
            // retain で期限が来ていないものだけ残す
            let mut woke_count = 0u32;
            wakers.retain(|(deadline, waker)| {
                if *deadline <= current {
                    waker.wake_by_ref();
                    woke_count += 1;
                    false // リストから削除
                } else {
                    true // リストに残す
                }
            });
            
            // 処理済みならフラグをクリア
            if wakers.is_empty() {
                HAS_PENDING_TIMERS.store(false, Ordering::Relaxed);
            }
            
            drop(wakers); // 早期にロック解放
            
            if woke_count > 0 {
                crate::debug_println!("[Timer] Woke {} timers at tick {}", woke_count, current);
            }
        }
        
        // 次のチェックまで yield
        // これにより他のタスクに実行機会を与える
        crate::kernel::r#async::yield_now().await;
    }
}

/// 非同期タイマー
/// 
/// 指定した時間（ミリ秒）後に完了する Future。
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

    /// 指定時間後に完了するタイマーを作成
    ///
    /// # Example
    /// ```ignore
    /// Timer::after(100).await; // 100ms 待機
    /// ```
    pub fn after(duration_ms: u64) -> Self {
        Self::new(duration_ms)
    }
}

impl Future for Timer {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let current_ticks = TICKS.load(Ordering::Relaxed);
        
        // 初回ポーリング時に期限を設定
        if self.deadline.is_none() {
            // 1 tick = 10ms (100Hz)
            let ticks_needed = (self.duration_ms + 9) / 10; // 切り上げ
            self.deadline = Some(current_ticks + ticks_needed);
        }

        let deadline = self.deadline.unwrap();

        if current_ticks >= deadline {
            Poll::Ready(())
        } else {
            // Waker を登録
            // ここでのロックは安全（Executor コンテキストで実行されるため）
            let mut wakers = WAKERS.lock();
            wakers.push((deadline, cx.waker().clone()));
            
            // ペンディングタイマーがあることをマーク
            HAS_PENDING_TIMERS.store(true, Ordering::Relaxed);
            
            Poll::Pending
        }
    }
}

/// 非同期的に yield する
/// 
/// 現在のタスクの実行を一時停止し、他のタスクに実行機会を与える。
pub struct Yield {
    yielded: bool,
}

impl Default for Yield {
    fn default() -> Self {
        Self::new()
    }
}

impl Yield {
    /// Creates a new Yield future.
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

/// Yields control to the scheduler, allowing other tasks to run.
///
/// # Example
/// ```ignore
/// loop {
///     do_work();
///     yield_now().await; // 他のタスクに実行機会を与える
/// }
/// ```
pub fn yield_now() -> Yield {
    Yield::new()
}

/// 現在のティック数を取得
#[inline]
pub fn current_ticks() -> u64 {
    TICKS.load(Ordering::Relaxed)
}

/// ティックをミリ秒に変換
#[inline]
pub const fn ticks_to_ms(ticks: u64) -> u64 {
    ticks * 10 // 1 tick = 10ms
}

/// ミリ秒をティックに変換
#[inline]
pub const fn ms_to_ticks(ms: u64) -> u64 {
    (ms + 9) / 10 // 切り上げ
}
