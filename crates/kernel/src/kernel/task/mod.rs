// kernel/src/kernel/task/mod.rs
//! タスク管理
use crate::kernel::core::{Task, TaskId, TaskState, Priority};

pub mod scheduler;
pub mod context;

/// 簡易タスク実装（デモ用）
pub struct SimpleTask {
    id: TaskId,
    priority: Priority,
    state: TaskState,
    name: &'static str,
}

impl SimpleTask {
    /// Creates a new task with the specified ID, priority, and name.
    pub fn new(id: u64, priority: Priority, name: &'static str) -> Self {
        Self {
            id: TaskId(id),
            priority,
            state: TaskState::Ready,
            name,
        }
    }
}

impl Task for SimpleTask {
    fn id(&self) -> TaskId {
        self.id
    }
    
    fn priority(&self) -> Priority {
        self.priority
    }
    
    fn name(&self) -> &'static str {
        self.name
    }
    
    fn state(&self) -> TaskState {
        self.state
    }
}
