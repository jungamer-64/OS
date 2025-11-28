// kernel/src/kernel/core/traits.rs
//! カーネルコア trait 定義

use super::types::{TaskId, Priority};
use super::result::KernelResult;
use alloc::boxed::Box;

/// デバイス抽象化の基本 trait
/// 
/// すべてのデバイスドライバはこの trait を実装します。
pub trait Device {
    /// デバイス名を取得
    fn name(&self) -> &'static str;
    
    /// デバイスを初期化
    ///
    /// # Errors
    ///
    /// デバイスの初期化に失敗した場合、エラーを返します。
    fn init(&mut self) -> KernelResult<()>;
    
    /// デバイスをリセット
    ///
    /// # Errors
    ///
    /// デバイスのリセットに失敗した場合、エラーを返します。
    fn reset(&mut self) -> KernelResult<()>;
    
    /// デバイスが利用可能か確認
    #[inline]
    fn is_available(&self) -> bool {
        true
    }
}

/// キャラクタデバイス trait（シリアル、VGA など）
/// 
/// バイト単位で読み書きするデバイス用。
pub trait CharDevice: Device {
    /// 1バイト読み取り（ノンブロッキング）
    ///
    /// # Errors
    ///
    /// 読み取りに失敗した場合、エラーを返します。
    fn read_byte(&self) -> KernelResult<Option<u8>>;
    
    /// 1バイト書き込み
    ///
    /// # Errors
    ///
    /// 書き込みに失敗した場合、エラーを返します。
    fn write_byte(&mut self, byte: u8) -> KernelResult<()>;
    
    /// バッファを書き込み（最適化版）
    ///
    /// # Errors
    ///
    /// 書き込みに失敗した場合、エラーを返します。
    #[inline]
    fn write_bytes(&mut self, buf: &[u8]) -> KernelResult<usize> {
        let mut written = 0;
        for &byte in buf {
            self.write_byte(byte)?;
            written += 1;
        }
        Ok(written)
    }
}

/// ブロックデバイス trait（ストレージなど）
/// 
/// 固定サイズブロック単位で読み書きするデバイス用。
pub trait BlockDevice: Device {
    /// ブロックサイズを取得（バイト単位）
    fn block_size(&self) -> usize;
    
    /// ブロックを読み取り
    ///
    /// # Errors
    ///
    /// 読み取りに失敗した場合、エラーを返します。
    fn read_block(&self, block: u64, buf: &mut [u8]) -> KernelResult<usize>;
    
    /// ブロックを書き込み
    ///
    /// # Errors
    ///
    /// 書き込みに失敗した場合、エラーを返します。
    fn write_block(&mut self, block: u64, buf: &[u8]) -> KernelResult<usize>;
    
    /// デバイスの総ブロック数
    #[inline]
    fn total_blocks(&self) -> u64 {
        0 // デフォルト実装
    }
}

/// タスク抽象化
/// 
/// スケジューラで管理される実行単位。
/// タスクの実行状態は外部（Scheduler）が管理します。
pub trait Task: Send {
    /// タスク ID を取得
    fn id(&self) -> TaskId;
    
    /// 優先度を取得
    fn priority(&self) -> Priority;
    
    /// タスク名を取得
    #[inline]
    fn name(&self) -> &'static str {
        "unnamed"
    }
    
    /// 現在の実行状態を取得
    fn state(&self) -> TaskState;
}

/// タスク実行状態
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    /// 実行可能（スケジュール待ち）
    Ready,
    /// 現在実行中
    Running,
    /// ブロック中（I/O待ちなど）
    Blocked,
    /// 終了済み
    Terminated,
}

impl TaskState {
    /// 状態遷移が有効かどうかをチェック
    #[inline]
    #[must_use]
    #[allow(clippy::match_same_arms)]
    pub const fn can_transition_to(self, next: Self) -> bool {
        match (self, next) {
            // Ready -> Running, Blocked, Terminated
            (Self::Ready, Self::Running | Self::Blocked | Self::Terminated) => true,
            // Running -> Ready, Blocked, Terminated
            (Self::Running, Self::Ready | Self::Blocked | Self::Terminated) => true,
            // Blocked -> Ready, Terminated
            (Self::Blocked, Self::Ready | Self::Terminated) => true,
            // Terminated -> (no transitions allowed)
            (Self::Terminated, _) => false,
            // Same state (always allowed)
            (a, b) if a as u8 == b as u8 => true,
            // Other transitions are invalid
            _ => false,
        }
    }
    
    /// 実行可能な状態かどうかをチェック
    #[inline]
    #[must_use]
    pub const fn is_runnable(self) -> bool {
        matches!(self, Self::Ready | Self::Running)
    }
    
    /// ブロック状態かどうかをチェック
    #[inline]
    #[must_use]
    pub const fn is_blocked(self) -> bool {
        matches!(self, Self::Blocked)
    }
    
    /// 終了状態かどうかをチェック
    #[inline]
    #[must_use]
    pub const fn is_terminated(self) -> bool {
        matches!(self, Self::Terminated)
    }
}

/// スケジューラ trait
/// 
/// タスクのスケジューリングとコンテキストスイッチを管理。
pub trait Scheduler {
    /// 次に実行するタスクを選択
    fn schedule(&mut self) -> Option<TaskId>;
    
    /// 指定されたタスクにスイッチ
    ///
    /// # Errors
    ///
    /// タスクへの切り替えに失敗した場合、エラーを返します。
    fn switch_to(&mut self, id: TaskId) -> KernelResult<()>;
    
    /// タスクを追加
    ///
    /// # Errors
    ///
    /// タスクの追加に失敗した場合、エラーを返します。
    fn add_task(&mut self, task: Box<dyn Task>) -> KernelResult<TaskId>;
    
    /// タスクを削除
    ///
    /// # Errors
    ///
    /// タスクの削除に失敗した場合、エラーを返します。
    fn remove_task(&mut self, id: TaskId) -> KernelResult<()>;
    
    /// タスク数を取得
    fn task_count(&self) -> usize;
    
    /// タスクの状態を変更
    ///
    /// # Errors
    ///
    /// タスクの状態変更に失敗した場合、エラーを返します。
    fn set_task_state(&mut self, id: TaskId, state: TaskState) -> KernelResult<()>;
}
