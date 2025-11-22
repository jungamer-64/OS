//! ラウンドロビンスケジューラ
//!
//! 単純なラウンドロビンスケジューリングを実装します。

use crate::kernel::core::{Scheduler, Task, TaskId, TaskState, KernelResult, TaskError};
use alloc::collections::VecDeque;
use alloc::boxed::Box;
use spin::Mutex;

/// ラウンドロビンスケジューラ
pub struct RoundRobinScheduler {
    tasks: VecDeque<Box<dyn Task>>,
    current_task: Option<TaskId>,
}

impl RoundRobinScheduler {
    /// 新しいスケジューラを作成
    pub fn new() -> Self {
        Self {
            tasks: VecDeque::new(),
            current_task: None,
        }
    }
}

impl Scheduler for RoundRobinScheduler {
    fn schedule(&mut self) -> Option<TaskId> {
        if self.tasks.is_empty() {
            return None;
        }

        // 現在のタスクを末尾に移動（ラウンドロビン）
        if let Some(current_id) = self.current_task
            && let Some(pos) = self.tasks.iter().position(|t| t.id() == current_id)
                && let Some(task) = self.tasks.remove(pos) {
                    if task.state() == TaskState::Running || task.state() == TaskState::Ready {
                        self.tasks.push_back(task);
                    } else {
                        // 終了したタスクなどは戻さない（または別のリストで管理）
                        // ここでは単純化のため、Running/Ready 以外はキューから外れたままにする
                        // 実際には Blocked リストなどが必要
                        if task.state() == TaskState::Blocked {
                             self.tasks.push_back(task);
                        }
                    }
                }

        // 次の実行可能タスクを探す
        for task in self.tasks.iter() {
            if task.state() == TaskState::Ready || task.state() == TaskState::Running {
                let id = task.id();
                self.current_task = Some(id);
                return Some(id);
            }
        }

        None
    }

    fn switch_to(&mut self, id: TaskId) -> KernelResult<()> {
        // 実際のコンテキストスイッチはここで行うか、
        // アーキテクチャ依存のコードを呼び出す
        // ここでは論理的な切り替えのみ
        self.current_task = Some(id);
        Ok(())
    }

    fn add_task(&mut self, task: Box<dyn Task>) -> KernelResult<TaskId> {
        let id = task.id();
        self.tasks.push_back(task);
        Ok(id)
    }

    fn remove_task(&mut self, id: TaskId) -> KernelResult<()> {
        if let Some(pos) = self.tasks.iter().position(|t| t.id() == id) {
            self.tasks.remove(pos);
            if self.current_task == Some(id) {
                self.current_task = None;
            }
            Ok(())
        } else {
            Err(TaskError::NotFound.into())
        }
    }

    fn task_count(&self) -> usize {
        self.tasks.len()
    }

    fn set_task_state(&mut self, _id: TaskId, _state: TaskState) -> KernelResult<()> {
        // Note: Task trait は不変参照しか返さないため、
        // 状態を変更するには内部可変性か、Task trait に set_state を追加する必要がある
        // ここでは簡易実装のためスキップ
        Ok(())
    }
}

/// グローバルスケジューラ
pub static SCHEDULER: Mutex<RoundRobinScheduler> = Mutex::new(RoundRobinScheduler {
    tasks: VecDeque::new(),
    current_task: None,
});

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::task::SimpleTask;
    use crate::kernel::core::Priority;

    #[test_case]
    fn test_scheduler_add_remove() {
        let mut scheduler = RoundRobinScheduler::new();
        let task1 = Box::new(SimpleTask::new(1, Priority::Normal, "task1"));
        
        assert_eq!(scheduler.task_count(), 0);
        scheduler.add_task(task1).unwrap();
        assert_eq!(scheduler.task_count(), 1);
        
        scheduler.remove_task(TaskId(1)).unwrap();
        assert_eq!(scheduler.task_count(), 0);
    }

    #[test_case]
    fn test_scheduler_round_robin() {
        let mut scheduler = RoundRobinScheduler::new();
        let task1 = Box::new(SimpleTask::new(1, Priority::Normal, "task1"));
        let task2 = Box::new(SimpleTask::new(2, Priority::Normal, "task2"));
        let task3 = Box::new(SimpleTask::new(3, Priority::Normal, "task3"));

        scheduler.add_task(task1).unwrap();
        scheduler.add_task(task2).unwrap();
        scheduler.add_task(task3).unwrap();

        // 1 -> 2 -> 3 -> 1 ...
        assert_eq!(scheduler.schedule(), Some(TaskId(1)));
        assert_eq!(scheduler.schedule(), Some(TaskId(2)));
        assert_eq!(scheduler.schedule(), Some(TaskId(3)));
        assert_eq!(scheduler.schedule(), Some(TaskId(1)));
    }
}
