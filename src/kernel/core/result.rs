// src/kernel/core/result.rs
//! カーネル共通エラーハンドリング
//!
//! コンテキスト情報付きエラーで、デバッグを容易にします。

use core::fmt;

/// カーネル Result 型
pub type KernelResult<T> = Result<T, KernelError>;

/// カーネルエラー（コンテキスト情報付き）
#[derive(Debug, Clone)]
pub struct KernelError {
    kind: ErrorKind,
    context: Option<&'static str>,
}

impl KernelError {
    /// 新しいエラーを作成
    #[inline]
    pub const fn new(kind: ErrorKind) -> Self {
        Self { kind, context: None }
    }
    
    /// コンテキスト情報付きエラーを作成
    #[inline]
    pub const fn with_context(kind: ErrorKind, ctx: &'static str) -> Self {
        Self { kind, context: Some(ctx) }
    }
    
    /// エラー種類を取得
    #[inline]
    pub const fn kind(&self) -> &ErrorKind {
        &self.kind
    }
    
    /// コンテキストを取得
    #[inline]
    pub const fn context(&self) -> Option<&'static str> {
        self.context
    }
    
    /// デバイスエラーか確認
    #[inline]
    pub const fn is_device_error(&self) -> bool {
        matches!(self.kind, ErrorKind::Device(_))
    }
    
    /// メモリエラーか確認
    #[inline]
    pub const fn is_memory_error(&self) -> bool {
        matches!(self.kind, ErrorKind::Memory(_))
    }
    
    /// タスクエラーか確認
    #[inline]
    #[must_use]
    pub const fn is_task_error(&self) -> bool {
        matches!(self.kind, ErrorKind::Task(_))
    }
    
    /// 再試行可能なエラーかどうかをチェック
    #[inline]
    #[must_use]
    pub const fn is_retryable(&self) -> bool {
        self.kind.is_retryable()
    }
    
    /// 致命的なエラーかどうかをチェック
    #[inline]
    #[must_use]
    pub const fn is_fatal(&self) -> bool {
        self.kind.is_fatal()
    }
}

/// エラー種類
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// デバイスエラー
    Device(DeviceError),
    /// メモリエラー
    Memory(MemoryError),
    /// タスクエラー
    Task(TaskError),
    /// 不正な引数
    InvalidArgument,
    /// リソースが利用不可
    ResourceUnavailable,
    /// 未実装
    NotImplemented,
}

impl ErrorKind {
    /// 再試行可能なエラーかどうかをチェック
    #[inline]
    #[must_use]
    pub const fn is_retryable(self) -> bool {
        match self {
            Self::Device(e) => e.is_retryable(),
            Self::Task(e) => e.is_retryable(),
            Self::ResourceUnavailable => true,
            _ => false,
        }
    }
    
    /// 致命的なエラーかどうかをチェック
    #[inline]
    #[must_use]
    pub const fn is_fatal(self) -> bool {
        match self {
            Self::Memory(e) => e.is_fatal(),
            _ => false,
        }
    }
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Device(e) => write!(f, "Device error: {}", e),
            Self::Memory(e) => write!(f, "Memory error: {}", e),
            Self::Task(e) => write!(f, "Task error: {}", e),
            Self::InvalidArgument => write!(f, "Invalid argument"),
            Self::ResourceUnavailable => write!(f, "Resource unavailable"),
            Self::NotImplemented => write!(f, "Not implemented"),
        }
    }
}

/// デバイスエラー
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceError {
    /// 初期化失敗
    InitFailed,
    /// ハードウェアが応答しない
    Timeout,
    /// デバイスが見つからない
    NotFound,
    /// I/O エラー
    IoError,
    /// バッファが小さすぎる
    BufferTooSmall,
}

impl DeviceError {
    /// エラーの説明文字列を取得
    #[inline]
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InitFailed => "device initialization failed",
            Self::Timeout => "device operation timed out",
            Self::NotFound => "device not found",
            Self::IoError => "I/O error occurred",
            Self::BufferTooSmall => "buffer too small for operation",
        }
    }
    
    /// 再試行可能なエラーかどうかをチェック
    #[inline]
    #[must_use]
    pub const fn is_retryable(self) -> bool {
        matches!(self, Self::Timeout | Self::IoError)
    }
}

impl fmt::Display for DeviceError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// メモリエラー
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryError {
    /// メモリ不足
    OutOfMemory,
    /// 不正なアドレス
    InvalidAddress,
    /// アライメント違反
    MisalignedAccess,
}

impl MemoryError {
    /// エラーの説明文字列を取得
    #[inline]
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OutOfMemory => "out of memory",
            Self::InvalidAddress => "invalid memory address",
            Self::MisalignedAccess => "misaligned memory access",
        }
    }
    
    /// 致命的なエラーかどうかをチェック
    #[inline]
    #[must_use]
    pub const fn is_fatal(self) -> bool {
        matches!(self, Self::OutOfMemory)
    }
}

impl fmt::Display for MemoryError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// タスクエラー
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskError {
    /// タスクが見つからない
    NotFound,
    /// タスクキューが満杯
    QueueFull,
    /// 無効な状態遷移
    InvalidStateTransition,
}

impl TaskError {
    /// エラーの説明文字列を取得
    #[inline]
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotFound => "task not found",
            Self::QueueFull => "task queue is full",
            Self::InvalidStateTransition => "invalid task state transition",
        }
    }
    
    /// 再試行可能なエラーかどうかをチェック
    #[inline]
    #[must_use]
    pub const fn is_retryable(self) -> bool {
        matches!(self, Self::QueueFull)
    }
}

impl fmt::Display for TaskError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Display for KernelError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.kind)?;
        
        if let Some(ctx) = self.context {
            write!(f, " (context: {})", ctx)?;
        }
        
        Ok(())
    }
}

impl From<DeviceError> for KernelError {
    #[inline]
    fn from(e: DeviceError) -> Self {
        KernelError::new(ErrorKind::Device(e))
    }
}

impl From<MemoryError> for KernelError {
    #[inline]
    fn from(e: MemoryError) -> Self {
        KernelError::new(ErrorKind::Memory(e))
    }
}

impl From<TaskError> for KernelError {
    #[inline]
    fn from(e: TaskError) -> Self {
        KernelError::new(ErrorKind::Task(e))
    }
}

impl From<ErrorKind> for KernelError {
    #[inline]
    fn from(kind: ErrorKind) -> Self {
        KernelError::new(kind)
    }
}
