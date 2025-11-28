// crates/kernel/src/kernel/async/executor.rs
//! Future Executor
//!
//! 非同期タスクを実行するための Executor。
//! crossbeam-queue を使用したロックフリーキューで効率的にタスクを管理。
//!
//! # 最適化
//!
//! タスクを `Arc<Mutex<Option<Task>>>` で保持することで、
//! Poll のたびに BTreeMap から remove/insert するオーバーヘッドを回避。
//! タスクはマップに残したまま、インプレースで poll される。

use core::future::Future;
use core::task::{Context, Poll, Waker};
use core::pin::Pin;
use core::sync::atomic::{AtomicU64, AtomicBool, Ordering};
use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::task::Wake;
use alloc::collections::BTreeMap;
use spin::{Mutex, Lazy};
use crossbeam_queue::ArrayQueue;

// ============================================================================
// Task ID
// ============================================================================

/// タスク ID - 各タスクを一意に識別
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TaskId(u64);

/// 次のタスク ID
static NEXT_TASK_ID: AtomicU64 = AtomicU64::new(1);

impl TaskId {
    /// 新しいユニークな TaskId を生成
    pub fn new() -> Self {
        Self(NEXT_TASK_ID.fetch_add(1, Ordering::Relaxed))
    }

    /// 内部の u64 値を取得
    #[inline]
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

impl Default for TaskId {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Task
// ============================================================================

/// 非同期タスク
/// 
/// Box 化された Future を保持し、TaskId で識別される。
pub struct Task {
    /// タスク ID
    id: TaskId,
    /// Box 化された Future
    future: Pin<Box<dyn Future<Output = ()> + Send>>,
}

impl Task {
    /// 新しいタスクを作成
    pub fn new(future: impl Future<Output = ()> + 'static + Send) -> Self {
        Self {
            id: TaskId::new(),
            future: Box::pin(future),
        }
    }

    /// タスク ID を取得
    #[inline]
    pub const fn id(&self) -> TaskId {
        self.id
    }

    /// タスクをポーリング
    fn poll(&mut self, context: &mut Context) -> Poll<()> {
        self.future.as_mut().poll(context)
    }
}

/// タスクへの共有参照（インプレース poll 用）
type SharedTask = Arc<Mutex<Option<Task>>>;

/// タスクスロット - タスクを O(1) で取り出さずに poll できる
struct TaskSlot {
    /// タスクを保持（poll 時に一時的に take）
    task: SharedTask,
}

impl TaskSlot {
    /// 新しいタスクスロットを作成
    fn new(task: Task) -> Self {
        Self {
            task: Arc::new(Mutex::new(Some(task))),
        }
    }
    
    /// 共有参照を取得
    fn shared(&self) -> SharedTask {
        Arc::clone(&self.task)
    }
    
    /// タスクが完了したかどうか
    fn is_completed(&self) -> bool {
        self.task.lock().is_none()
    }
}

// ============================================================================
// TaskWaker - Wake trait 実装
// ============================================================================

/// タスクを起こすための Waker 実装
struct TaskWaker {
    /// 対象タスクの ID
    task_id: TaskId,
    /// タスクキューへの参照
    task_queue: Arc<ArrayQueue<TaskId>>,
}

impl TaskWaker {
    /// 新しい TaskWaker を作成
    fn new(task_id: TaskId, task_queue: Arc<ArrayQueue<TaskId>>) -> Self {
        Self { task_id, task_queue }
    }

    /// Waker として使える形に変換
    fn waker(self: Arc<Self>) -> Waker {
        Waker::from(self)
    }
}

impl Wake for TaskWaker {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        // キューに TaskId を追加（ロックフリー）
        // キューがフルの場合は無視（次のポーリングで再試行される）
        let _ = self.task_queue.push(self.task_id);
    }
}

// ============================================================================
// Executor
// ============================================================================

/// タスクキューのデフォルトサイズ
const DEFAULT_QUEUE_SIZE: usize = 256;

/// Future Executor
///
/// crossbeam-queue を使用したロックフリーキューでタスクを管理。
/// カーネル内の非同期処理の中心となるコンポーネント。
///
/// # 最適化ポイント
///
/// - タスクは `TaskSlot` (Arc<Mutex<Option<Task>>>) で保持
/// - Poll 時に BTreeMap から remove/insert しない（O(1) アクセス）
/// - 完了したタスクのみ削除（遅延 GC）
pub struct Executor {
    /// 実行待ちタスクキュー（ロックフリー）
    task_queue: Arc<ArrayQueue<TaskId>>,
    /// タスクスロットマップ（TaskId -> TaskSlot）
    task_slots: Mutex<BTreeMap<TaskId, TaskSlot>>,
    /// 実行中フラグ
    running: AtomicBool,
    /// 完了タスク数（統計用）
    completed_count: AtomicU64,
}

impl Executor {
    /// 新しい Executor を作成
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_QUEUE_SIZE)
    }

    /// 指定したキャパシティで Executor を作成
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            task_queue: Arc::new(ArrayQueue::new(capacity)),
            task_slots: Mutex::new(BTreeMap::new()),
            running: AtomicBool::new(false),
            completed_count: AtomicU64::new(0),
        }
    }

    /// タスクを追加（spawn）
    ///
    /// 任意の Future を Executor に追加し、すぐにキューに入れる。
    pub fn spawn(&self, future: impl Future<Output = ()> + 'static + Send) -> TaskId {
        let task = Task::new(future);
        let task_id = task.id();
        let slot = TaskSlot::new(task);

        // タスクスロットマップに追加
        self.task_slots.lock().insert(task_id, slot);
        
        // 実行キューに追加
        let _ = self.task_queue.push(task_id);

        crate::debug_println!("[Executor] Spawned task {}", task_id.as_u64());
        task_id
    }

    /// 1つのタスクをポーリング（最適化版）
    ///
    /// キューからタスクを1つ取り出してインプレースでポーリングする。
    /// タスクを BTreeMap から remove/insert しないため高速。
    /// 
    /// # Returns
    /// - `Some(true)` - タスクが完了した
    /// - `Some(false)` - タスクはまだ Pending
    /// - `None` - キューが空だった
    pub fn run_one(&self) -> Option<bool> {
        // キューからタスク ID を取得
        let task_id = self.task_queue.pop()?;

        // タスクスロットを取得（共有参照）
        let shared_task = {
            let slots = self.task_slots.lock();
            slots.get(&task_id).map(|slot| slot.shared())
        };

        let shared_task = shared_task?;

        // Waker を作成
        let waker = Arc::new(TaskWaker::new(task_id, Arc::clone(&self.task_queue)));
        let waker = waker.waker();
        let mut context = Context::from_waker(&waker);

        // タスクをインプレースでポーリング
        // ロック中に poll することで、他のスレッドからの干渉を防ぐ
        let completed = {
            let mut task_guard = shared_task.lock();
            
            if let Some(task) = task_guard.as_mut() {
                match task.poll(&mut context) {
                    Poll::Ready(()) => {
                        // 完了: タスクを削除マーク
                        *task_guard = None;
                        true
                    }
                    Poll::Pending => {
                        // Pending: そのまま（Waker が次回 wake するとキューに戻る）
                        false
                    }
                }
            } else {
                // すでに完了済み（二重 wake の場合）
                true
            }
        };

        if completed {
            // 完了したタスクをスロットマップから削除
            self.task_slots.lock().remove(&task_id);
            self.completed_count.fetch_add(1, Ordering::Relaxed);
            crate::debug_println!("[Executor] Task {} completed", task_id.as_u64());
        }

        Some(completed)
    }

    /// アイドル状態になるまで実行
    ///
    /// キューが空になるまでタスクを実行し続ける。
    /// すべてのタスクが完了するわけではなく、I/O 待ちなどで
    /// Pending になったタスクは Waker で再度キューに入れられる。
    ///
    /// # Returns
    /// 完了したタスクの数
    pub fn run_until_idle(&self) -> usize {
        let mut completed = 0;

        while let Some(was_completed) = self.run_one() {
            if was_completed {
                completed += 1;
            }
        }

        completed
    }

    /// すべてのタスクが完了するまで実行
    ///
    /// タスクマップが空になるまで実行し続ける。
    /// 無限ループに注意（I/O 待ちタスクがある場合など）。
    pub fn run(&self) {
        self.running.store(true, Ordering::SeqCst);

        while self.running.load(Ordering::SeqCst) {
            // キューからタスクを処理
            if self.run_one().is_none() {
                // キューが空の場合
                if self.task_slots.lock().is_empty() {
                    // すべてのタスクが完了
                    break;
                }
                // タスクはあるがキューが空（I/O 待ちなど）
                // 少し待ってから再試行
                core::hint::spin_loop();
            }
        }

        self.running.store(false, Ordering::SeqCst);
    }

    /// Executor を停止
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    /// 現在のタスク数を取得
    pub fn task_count(&self) -> usize {
        self.task_slots.lock().len()
    }

    /// キュー内のタスク数を取得
    pub fn queued_count(&self) -> usize {
        self.task_queue.len()
    }

    /// 実行中かどうか
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
    
    /// 完了したタスクの総数
    pub fn completed_count(&self) -> u64 {
        self.completed_count.load(Ordering::Relaxed)
    }
}

impl Default for Executor {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// グローバル RUNTIME
// ============================================================================

/// グローバルカーネルランタイム
///
/// どこからでも `spawn_task()` でタスクを追加できる。
pub static RUNTIME: Lazy<Executor> = Lazy::new(Executor::new);

/// グローバルランタイムにタスクを追加
///
/// # Example
/// ```ignore
/// spawn_task(async {
///     debug_println!("Hello from async!");
///     Timer::after(100).await;
///     debug_println!("100ms later!");
/// });
/// ```
pub fn spawn_task(future: impl Future<Output = ()> + 'static + Send) -> TaskId {
    RUNTIME.spawn(future)
}

/// グローバルランタイムを1ステップ実行
///
/// カーネルのアイドルループから呼び出すことを想定。
pub fn poll_runtime() -> Option<bool> {
    RUNTIME.run_one()
}

/// グローバルランタイムをアイドルまで実行
pub fn run_runtime_until_idle() -> usize {
    RUNTIME.run_until_idle()
}

/// グローバルランタイムのタスク数を取得
pub fn runtime_task_count() -> usize {
    RUNTIME.task_count()
}

