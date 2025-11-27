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
    /// 予約された無効なID
    pub const INVALID: Self = Self(0);
    
    /// カーネルタスクIDの開始
    pub const KERNEL_START: u64 = 1;
    
    /// ユーザータスクIDの開始
    pub const USER_START: u64 = 1000;
    
    /// 新しいタスク ID を作成
    #[inline]
    #[must_use]
    pub const fn new(id: u64) -> Self {
        Self(id)
    }
    
    /// 検証付きでタスクIDを作成
    #[inline]
    #[must_use]
    pub const fn new_checked(id: u64) -> Option<Self> {
        if id == 0 {
            None
        } else {
            Some(Self(id))
        }
    }
    
    /// ID を取得
    #[inline]
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
    
    /// 有効なIDかどうかをチェック
    #[inline]
    #[must_use]
    pub const fn is_valid(self) -> bool {
        self.0 != 0
    }
    
    /// カーネルタスクIDかどうかをチェック
    #[inline]
    #[must_use]
    pub const fn is_kernel(self) -> bool {
        self.0 >= Self::KERNEL_START && self.0 < Self::USER_START
    }
    
    /// ユーザータスクIDかどうかをチェック
    #[inline]
    #[must_use]
    pub const fn is_user(self) -> bool {
        self.0 >= Self::USER_START
    }
}

/// プロセス ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct ProcessId(pub u64);

impl ProcessId {
    /// 予約された無効なID
    pub const INVALID: Self = Self(0);
    
    /// initプロセスのID
    pub const INIT: Self = Self(1);
    
    /// カーネルプロセスIDの開始
    pub const KERNEL_START: u64 = 1;
    
    /// ユーザープロセスIDの開始
    pub const USER_START: u64 = 1000;
    
    /// 新しいプロセス ID を作成
    #[inline]
    #[must_use]
    pub const fn new(id: u64) -> Self {
        Self(id)
    }
    
    /// 検証付きでプロセスIDを作成
    #[inline]
    #[must_use]
    pub const fn new_checked(id: u64) -> Option<Self> {
        if id == 0 {
            None
        } else {
            Some(Self(id))
        }
    }
    
    /// ID を取得
    #[inline]
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
    
    /// 有効なIDかどうかをチェック
    #[inline]
    #[must_use]
    pub const fn is_valid(self) -> bool {
        self.0 != 0
    }
    
    /// カーネルプロセスIDかどうかをチェック
    #[inline]
    #[must_use]
    pub const fn is_kernel(self) -> bool {
        self.0 >= Self::KERNEL_START && self.0 < Self::USER_START
    }
    
    /// ユーザープロセスIDかどうかをチェック
    #[inline]
    #[must_use]
    pub const fn is_user(self) -> bool {
        self.0 >= Self::USER_START
    }
}

/// タスク優先度
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum Priority {
    /// Idle priority (lowest)
    Idle = 0,
    /// Low priority
    Low = 1,
    /// Normal priority
    Normal = 2,
    /// High priority
    High = 3,
    /// Critical priority (highest)
    Critical = 4,
}

impl Priority {
    /// 最低優先度
    pub const MIN: Self = Self::Idle;
    
    /// 最高優先度
    pub const MAX: Self = Self::Critical;
    
    /// 優先度の数値を取得
    #[inline]
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }
    
    /// 数値から優先度を作成
    #[inline]
    #[must_use]
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
    
    /// 優先度を上げる（境界チェック付き）
    #[inline]
    #[must_use]
    pub const fn increase(self) -> Option<Self> {
        Self::from_u8(self.as_u8().saturating_add(1))
    }
    
    /// 優先度を下げる（境界チェック付き）
    #[inline]
    #[must_use]
    pub const fn decrease(self) -> Option<Self> {
        if self.as_u8() > 0 {
            Self::from_u8(self.as_u8() - 1)
        } else {
            None
        }
    }
    
    /// 指定された優先度より高いかどうかをチェック
    #[inline]
    #[must_use]
    pub const fn is_higher_than(self, other: Self) -> bool {
        self.as_u8() > other.as_u8()
    }
    
    /// 指定された優先度より低いかどうかをチェック
    #[inline]
    #[must_use]
    pub const fn is_lower_than(self, other: Self) -> bool {
        self.as_u8() < other.as_u8()
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
            Self::Idle => write!(f, "Idle"),
            Self::Low => write!(f, "Low"),
            Self::Normal => write!(f, "Normal"),
            Self::High => write!(f, "High"),
            Self::Critical => write!(f, "Critical"),
        }
    }
}
