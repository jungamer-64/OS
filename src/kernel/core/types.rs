// src/kernel/core/types.rs
//! カーネル共通型定義

use core::fmt;

/// デバイス ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DeviceId(pub u32);

/// タスク ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TaskId(pub u64);

/// プロセス ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProcessId(pub u64);

/// タスク優先度
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum Priority {
    Idle = 0,
    Low = 1,
    Normal = 2,
    High = 3,
    Critical = 4,
}

impl fmt::Display for Priority {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Priority::Idle => write!(f, "Idle"),
            Priority::Low => write!(f, "Low"),
            Priority::Normal => write!(f, "Normal"),
            Priority::High => write!(f, "High"),
            Priority::Critical => write!(f, "Critical"),
        }
    }
}
