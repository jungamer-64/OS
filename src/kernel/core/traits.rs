// src/kernel/core/traits.rs
//! カーネルコア trait 定義

use super::types::*;
use super::result::*;
// use alloc::boxed::Box; // Phase 4 で有効化

/// デバイス抽象化の基本 trait
/// 
/// すべてのデバイスドライバはこの trait を実装します。
pub trait Device {
    /// デバイス名を取得
    fn name(&self) -> &str;
    
    /// デバイスを初期化
    fn init(&mut self) -> KernelResult<()>;
    
    /// デバイスをリセット
    fn reset(&mut self) -> KernelResult<()>;
    
    /// デバイスが利用可能か確認
    fn is_available(&self) -> bool {
        true
    }
}

/// キャラクタデバイス trait（シリアル、VGA など）
/// 
/// バイト単位で読み書きするデバイス用。
pub trait CharDevice: Device {
    /// 1バイト読み取り（ノンブロッキング）
    fn read_byte(&self) -> KernelResult<Option<u8>>;
    
    /// 1バイト書き込み
    fn write_byte(&mut self, byte: u8) -> KernelResult<()>;
    
    /// バッファを書き込み
    fn write_bytes(&mut self, buf: &[u8]) -> KernelResult<usize> {
        for &byte in buf.iter() {
            self.write_byte(byte)?;
        }
        Ok(buf.len())
    }
}

/// ブロックデバイス trait（ストレージなど）
/// 
/// 固定サイズブロック単位で読み書きするデバイス用。
pub trait BlockDevice: Device {
    /// ブロックサイズを取得（バイト単位）
    fn block_size(&self) -> usize;
    
    /// ブロックを読み取り
    fn read_block(&self, block: u64, buf: &mut [u8]) -> KernelResult<usize>;
    
    /// ブロックを書き込み
    fn write_block(&mut self, block: u64, buf: &[u8]) -> KernelResult<usize>;
    
    /// デバイスの総ブロック数
    fn total_blocks(&self) -> u64 {
        0 // デフォルト実装
    }
}

/// タスク抽象化
/// 
/// スケジューラで管理される実行単位。
/// タスクの実行状態は外部（Scheduler）が管理します。
pub trait Task {
    /// タスク ID を取得
    fn id(&self) -> TaskId;
    
    /// 優先度を取得
    fn priority(&self) -> Priority;
    
    /// タスク名を取得
    fn name(&self) -> &str {
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

/// スケジューラ trait
/// 
/// タスクのスケジューリングとコンテキストスイッチを管理。
pub trait Scheduler {
    /// 次に実行するタスクを選択
    fn schedule(&mut self) -> Option<TaskId>;
    
    /// 指定されたタスクにスイッチ
    fn switch_to(&mut self, id: TaskId) -> KernelResult<()>;
    
    // /// タスクを追加 (Phase 4 で有効化)
    // fn add_task(&mut self, task: Box<dyn Task>) -> KernelResult<TaskId>;
    
    /// タスクを削除
    fn remove_task(&mut self, id: TaskId) -> KernelResult<()>;
    
    /// タスク数を取得
    fn task_count(&self) -> usize;
    
    /// タスクの状態を変更
    fn set_task_state(&mut self, id: TaskId, state: TaskState) -> KernelResult<()>;
}
