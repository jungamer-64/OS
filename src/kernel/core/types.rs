//! カーネル共通型定義

use core::fmt;

/// デバイス ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct DeviceId(pub u32);

impl DeviceId {
    /// 新しいデバイス ID を作成
    #[inline]
    pub const fn new(id: u32) -> Self {
        Self(id)
    }
    
    /// ID を取得
    #[inline]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// タスク ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct TaskId(pub u64);

impl TaskId {
    /// 新しいタスク ID を作成
    #[inline]
    pub const fn new(id: u64) -> Self {
        Self(id)
    }
    
    /// ID を取得
    #[inline]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// プロセス ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct ProcessId(pub u64);

impl ProcessId {
    /// 新しいプロセス ID を作成
    #[inline]
    pub const fn new(id: u64) -> Self {
        Self(id)
    }
    
    /// ID を取得
    #[inline]
    pub const fn get(self) -> u64 {
        self.0
    }
}

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

impl Priority {
    /// 優先度の数値を取得
    #[inline]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }
    
    /// 数値から優先度を作成
    #[inline]
    pub const fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Idle),
            1 => Some(Self::Low),
            2 => Some(Self::Normal),
            3 => Some(Self::High),
            4 => Some(Self::Critical),
            _ => None,
        }
    }
}

impl Default for Priority {
    #[inline]
    fn default() -> Self {
        Self::Normal
    }
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
