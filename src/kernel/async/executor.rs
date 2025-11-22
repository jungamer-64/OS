//! Future Executor
//!
//! 非同期タスクを実行するための Executor。
//! VecDeque をタスクキューとして使用した簡易実装。

use core::future::Future;
use core::task::{Context, Poll, Waker};
use core::pin::Pin;
use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::sync::Arc;
use alloc::task::Wake;
use spin::Mutex;

/// 非同期タスク
struct Task {
    future: Pin<Box<dyn Future<Output = ()> + Send>>,
}

impl Task {
    /// 新しいタスクを作成
    fn new(future: impl Future<Output = ()> + 'static + Send) -> Self {
        Self {
            future: Box::pin(future),
        }
    }

    /// タスクをポーリング
    fn poll(&mut self, context: &mut Context) -> Poll<()> {
        self.future.as_mut().poll(context)
    }
}

/// タスク ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct TaskId(u64);

static NEXT_TASK_ID: Mutex<u64> = Mutex::new(0);

impl TaskId {
    fn new() -> Self {
        let mut id = NEXT_TASK_ID.lock();
        let task_id = Self(*id);
        *id += 1;
        task_id
    }
}

/// タスク Waker
struct TaskWaker {
    task_id: TaskId,
    queue: Arc<Mutex<VecDeque<TaskId>>>,
}

impl Wake for TaskWaker {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.queue.lock().push_back(self.task_id);
    }
}

/// Future Executor
pub struct Executor {
    /// タスクキュー
    task_queue: Arc<Mutex<VecDeque<TaskId>>>,
    /// タスクマップ
    tasks: Mutex<alloc::collections::BTreeMap<TaskId, Task>>,
}

impl Executor {
    /// 新しい Executor を作成
    pub fn new() -> Self {
        Self {
            task_queue: Arc::new(Mutex::new(VecDeque::new())),
            tasks: Mutex::new(alloc::collections::BTreeMap::new()),
        }
    }

    /// タスクを追加
    pub fn spawn(&self, future: impl Future<Output = ()> + 'static + Send) {
        let task_id = TaskId::new();
        let task = Task::new(future);
        
        self.tasks.lock().insert(task_id, task);
        self.task_queue.lock().push_back(task_id);
    }

    /// Executor を実行（すべてのタスクが完了するまで）
    pub fn run(&self) {
        loop {
            // キューからタスクを取得
            let task_id = match self.task_queue.lock().pop_front() {
                Some(id) => id,
                None => break, // キューが空なら終了
            };

            // タスクを取得
            let mut tasks = self.tasks.lock();
            let mut task = match tasks.remove(&task_id) {
                Some(task) => task,
                None => continue,
            };
            drop(tasks);

            // Waker を作成
            let waker = Arc::new(TaskWaker {
                task_id,
                queue: Arc::clone(&self.task_queue),
            });
            let waker = Waker::from(waker);
            let mut context = Context::from_waker(&waker);

            // タスクをポーリング
            match task.poll(&mut context) {
                Poll::Ready(()) => {
                    // タスク完了
                }
                Poll::Pending => {
                    // タスクを再度マップに追加
                    self.tasks.lock().insert(task_id, task);
                }
            }
        }
    }

    /// タスク数を取得
    pub fn task_count(&self) -> usize {
        self.tasks.lock().len()
    }
}

impl Default for Executor {
    fn default() -> Self {
        Self::new()
    }
}
