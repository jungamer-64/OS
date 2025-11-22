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
    pub const fn new(kind: ErrorKind) -> Self {
        Self { kind, context: None }
    }
    
    /// コンテキスト情報付きエラーを作成
    pub const fn with_context(kind: ErrorKind, ctx: &'static str) -> Self {
        Self { kind, context: Some(ctx) }
    }
    
    /// エラー種類を取得
    pub fn kind(&self) -> &ErrorKind {
        &self.kind
    }
    
    /// コンテキストを取得
    pub fn context(&self) -> Option<&'static str> {
        self.context
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

impl fmt::Display for KernelError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self.kind {
            ErrorKind::Device(e) => write!(f, "Device error: {:?}", e)?,
            ErrorKind::Memory(e) => write!(f, "Memory error: {:?}", e)?,
            ErrorKind::Task(e) => write!(f, "Task error: {:?}", e)?,
            ErrorKind::InvalidArgument => write!(f, "Invalid argument")?,
            ErrorKind::ResourceUnavailable => write!(f, "Resource unavailable")?,
            ErrorKind::NotImplemented => write!(f, "Not implemented")?,
        }
        
        if let Some(ctx) = self.context {
            write!(f, " (context: {})", ctx)?;
        }
        
        Ok(())
    }
}

impl From<DeviceError> for KernelError {
    fn from(e: DeviceError) -> Self {
        KernelError::new(ErrorKind::Device(e))
    }
}

impl From<MemoryError> for KernelError {
    fn from(e: MemoryError) -> Self {
        KernelError::new(ErrorKind::Memory(e))
    }
}

impl From<TaskError> for KernelError {
    fn from(e: TaskError) -> Self {
        KernelError::new(ErrorKind::Task(e))
    }
}

impl From<ErrorKind> for KernelError {
    fn from(kind: ErrorKind) -> Self {
        KernelError::new(kind)
    }
}
