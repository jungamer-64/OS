//! コンテキストスイッチ
//!
//! タスクの実行コンテキストを保存・復元します。

/// 保存されたレジスタ
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Context {
    r15: u64,
    r14: u64,
    r13: u64,
    r12: u64,
    rbx: u64,
    rbp: u64,
    rip: u64,
}

impl Context {
    /// 新しい空のコンテキストを作成
    pub const fn empty() -> Self {
        Self {
            r15: 0,
            r14: 0,
            r13: 0,
            r12: 0,
            rbx: 0,
            rbp: 0,
            rip: 0,
        }
    }
    
    /// 新しいタスク用のコンテキストを作成
    pub fn new(entry_point: u64, stack_top: u64) -> Self {
        Self {
            r15: 0,
            r14: 0,
            r13: 0,
            r12: 0,
            rbx: 0,
            rbp: stack_top, // スタックポインタの初期値として使用（実際には switch で rsp が切り替わる）
            rip: entry_point,
        }
    }
}

// コンテキストスイッチの実装はアーキテクチャ依存のアセンブリで行う必要があります。
// ここでは概念的な定義のみ行います。
// 実際の実装には naked function や global_asm! が必要です。
