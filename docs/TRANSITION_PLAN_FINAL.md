# 理想的な Rust カーネルアーキテクチャへの移行計画（最終版）

Unix/Linux レガシーに縛られない、Rust の安全性と型システムを最大限活かした理想的なカーネルアーキテクチャへの移行計画。

NOTE

この計画はユーザーの監査フィードバックを完全に反映した最終版です。

## 目標

既存の tiny_os プロジェクトを以下の特徴を持つ理想的な Rust カーネルに進化させる：

- **trait ベースの抽象化** - BlockDevice, Task, Scheduler などの trait でサブシステムを抽象化
- **型安全性の最大化** - デバイスレジスタやメモリ領域を型で表現、unsafe を最小限に
- **モジュール化** - サブシステムを明確に分離（CPU, メモリ, IO, デバイス）
- **非同期/並行性** - Future ベースの非同期タスク管理基盤
- **所有権ベースのリソース管理** - Rust の所有権システムでメモリやデバイスリソースを管理

## 現在の状態

### 既存の構造

```
src/
├─ arch/              ✅ アーキテクチャ抽象化（一部実装済み）
│  ├─ mod.rs         - Cpu trait, ArchCpu type
│  └─ x86_64/        - x86_64 実装
├─ memory/           ✅ メモリ抽象化（基礎のみ）
│  ├─ access.rs      - MemoryAccess trait
│  └─ safety.rs      - SafeBuffer
├─ vga_buffer/       - VGA ドライバ（レガシー設計）
├─ serial/           - Serial ドライバ（レガシー設計）
├─ framebuffer/      - Framebuffer サポート
├─ display/          - 表示抽象化
└─ init.rs           - 初期化ロジック
```

### 強み

✅ arch::Cpu trait による CPU 抽象化がすでに存在
✅ memory::MemoryAccess trait によるメモリアクセス抽象化
✅ bootloader_api 0.11 で UEFI サポート
✅ 安全性重視の設計（#![deny(unsafe_op_in_unsafe_fn)]）

### 改善点

❌ デバイスドライバが trait で抽象化されていない
❌ タスク/スケジューリング機構が未実装
❌ 非同期処理基盤が未実装
❌ 型安全な MMIO アクセスが未整備
❌ メモリ管理が最小限（ページング、アロケータ未実装）

## 実装アプローチ

### 完全書き直し方針

既存の `vga_buffer/`, `serial/`, `display/` などのモジュールは **参考として残し**、新しい `kernel/` モジュール構造に **一から実装** します。

理由：

- クリーンな設計を優先
- レガシーコードの制約を受けない
- trait ベースの統一されたアーキテクチャ
- unsafe を最小限に限定

移行戦略：

1. 新しい `kernel/` モジュールを作成
2. `main.rs` を新しいカーネルを使用するように書き換え
3. 古いモジュールは `.backup/legacy/` に移動（参考用）
4. 完全にテスト後、レガシーコードを削除

## 提案する変更

### Phase 1: カーネルコア基盤の構築

新しい `kernel/` モジュールのコア部分を構築します。

#### [NEW] `kernel/core/mod.rs`

カーネルコア抽象化モジュール：

```rust
//! カーネルコア抽象化
//! 
//! このモジュールは、カーネル全体で使用する基本的な trait、型、
//! エラーハンドリングを提供します。

pub mod traits;
pub mod types;
pub mod result;

pub use traits::{Device, CharDevice, BlockDevice, Task, Scheduler, TaskState};
pub use types::{DeviceId, TaskId, ProcessId, Priority};
pub use result::{KernelResult, KernelError, ErrorKind};
```

#### [NEW] `kernel/core/traits.rs`

カーネル全体で使用する trait 定義（Task trait は TaskState ベース）：

```rust
//! カーネルコア trait 定義

use super::types::*;
use super::result::*;

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
    
    /// タスクを追加
    fn add_task(&mut self, task: Box<dyn Task>) -> KernelResult<TaskId>;
    
    /// タスクを削除
    fn remove_task(&mut self, id: TaskId) -> KernelResult<()>;
    
    /// タスク数を取得
    fn task_count(&self) -> usize;
    
    /// タスクの状態を変更
    fn set_task_state(&mut self, id: TaskId, state: TaskState) -> KernelResult<()>;
}
```

#### [NEW] `kernel/core/types.rs`

カーネル共通型定義：

```rust
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
```

#### [NEW] `kernel/core/result.rs`

カーネル共通の Result/Error 型（コンテキスト情報付き）：

```rust
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
```

### Phase 1.5: ポートI/O抽象化

IMPORTANT

Phase 2 の前提条件: デバイスドライバが依存するポートI/O抽象化を先に実装します。

#### [NEW] `arch/x86_64/port.rs`

型安全なポートI/O抽象化：

```rust
//! x86_64 ポートI/O抽象化
//!
//! 型安全な I/O ポートアクセスを提供します。
//! unsafe 操作を最小限の範囲に閉じ込めます。

use core::marker::PhantomData;

/// 読み書き可能な I/O ポート
#[derive(Debug)]
pub struct Port<T> {
    port: u16,
    _phantom: PhantomData<T>,
}

impl<T> Port<T> {
    /// 新しいポートを作成（const 関数）
    pub const fn new(port: u16) -> Self {
        Self {
            port,
            _phantom: PhantomData,
        }
    }
}

/// 8ビットポート実装
impl Port<u8> {
    /// ポートから1バイト読み取り
    ///
    /// # Safety
    /// 
    /// 呼び出し元は、指定されたポート番号が有効であり、
    /// 読み取り操作が安全であることを保証する必要があります。
    pub unsafe fn read(&self) -> u8 {
        let value: u8;
        core::arch::asm!(
            "in al, dx",
            in("dx") self.port,
            out("al") value,
            options(nomem, nostack, preserves_flags)
        );
        value
    }
    
    /// ポートに1バイト書き込み
    ///
    /// # Safety
    /// 
    /// 呼び出し元は、指定されたポート番号が有効であり、
    /// 書き込み操作が安全であることを保証する必要があります。
    pub unsafe fn write(&mut self, value: u8) {
        core::arch::asm!(
            "out dx, al",
            in("dx") self.port,
            in("al") value,
            options(nomem, nostack, preserves_flags)
        );
    }
}

/// 16ビットポート実装
impl Port<u16> {
    /// ポートから2バイト読み取り
    pub unsafe fn read(&self) -> u16 {
        let value: u16;
        core::arch::asm!(
            "in ax, dx",
            in("dx") self.port,
            out("ax") value,
            options(nomem, nostack, preserves_flags)
        );
        value
    }
    
    /// ポートに2バイト書き込み
    pub unsafe fn write(&mut self, value: u16) {
        core::arch::asm!(
            "out dx, ax",
            in("dx") self.port,
            in("ax") value,
            options(nomem, nostack, preserves_flags)
        );
    }
}

/// 32ビットポート実装
impl Port<u32> {
    /// ポートから4バイト読み取り
    pub unsafe fn read(&self) -> u32 {
        let value: u32;
        core::arch::asm!(
            "in eax, dx",
            in("dx") self.port,
            out("eax") value,
            options(nomem, nostack, preserves_flags)
        );
        value
    }
    
    /// ポートに4バイト書き込み
    pub unsafe fn write(&mut self, value: u32) {
        core::arch::asm!(
            "out dx, eax",
            in("dx") self.port,
            in("eax") value,
            options(nomem, nostack, preserves_flags)
        );
    }
}

/// 読み取り専用 I/O ポート
#[derive(Debug)]
pub struct PortReadOnly<T> {
    port: u16,
    _phantom: PhantomData<T>,
}

impl<T> PortReadOnly<T> {
    /// 新しい読み取り専用ポートを作成
    pub const fn new(port: u16) -> Self {
        Self {
            port,
            _phantom: PhantomData,
        }
    }
}

/// 8ビット読み取り専用ポート
impl PortReadOnly<u8> {
    /// ポートから1バイト読み取り
    pub unsafe fn read(&self) -> u8 {
        let value: u8;
        core::arch::asm!(
            "in al, dx",
            in("dx") self.port,
            out("al") value,
            options(nomem, nostack, preserves_flags)
        );
        value
    }
}

/// 16ビット読み取り専用ポート
impl PortReadOnly<u16> {
    /// ポートから2バイト読み取り
    pub unsafe fn read(&self) -> u16 {
        let value: u16;
        core::arch::asm!(
            "in ax, dx",
            in("dx") self.port,
            out("ax") value,
            options(nomem, nostack, preserves_flags)
        );
        value
    }
}

/// 32ビット読み取り専用ポート
impl PortReadOnly<u32> {
    /// ポートから4バイト読み取り
    pub unsafe fn read(&self) -> u32 {
        let value: u32;
        core::arch::asm!(
            "in eax, dx",
            in("dx") self.port,
            out("eax") value,
            options(nomem, nostack, preserves_flags)
        );
        value
    }
}

/// 書き込み専用 I/O ポート
#[derive(Debug)]
pub struct PortWriteOnly<T> {
    port: u16,
    _phantom: PhantomData<T>,
}

impl<T> PortWriteOnly<T> {
    /// 新しい書き込み専用ポートを作成
    pub const fn new(port: u16) -> Self {
        Self {
            port,
            _phantom: PhantomData,
        }
    }
}

/// 8ビット書き込み専用ポート
impl PortWriteOnly<u8> {
    /// ポートに1バイト書き込み
    pub unsafe fn write(&mut self, value: u8) {
        core::arch::asm!(
            "out dx, al",
            in("dx") self.port,
            in("al") value,
            options(nomem, nostack, preserves_flags)
        );
    }
}

/// 16ビット書き込み専用ポート
impl PortWriteOnly<u16> {
    /// ポートに2バイト書き込み
    pub unsafe fn write(&mut self, value: u16) {
        core::arch::asm!(
            "out dx, ax",
            in("dx") self.port,
            in("ax") value,
            options(nomem, nostack, preserves_flags)
        );
    }
}

/// 32ビット書き込み専用ポート
impl PortWriteOnly<u32> {
    /// ポートに4バイト書き込み
    pub unsafe fn write(&mut self, value: u32) {
        core::arch::asm!(
            "out dx, eax",
            in("dx") self.port,
            in("eax") value,
            options(nomem, nostack, preserves_flags)
        );
    }
}
```

#### [MODIFY] `arch/x86_64/mod.rs`

ポートモジュールを公開：

```rust
// arch/x86_64/mod.rs に追加
pub mod port;
pub use port::{Port, PortReadOnly, PortWriteOnly};
```

### Phase 2: 新しいデバイスドライバの実装

kernel/driver/ ディレクトリに trait ベースのデバイスドライバを一から実装します。

WARNING

VGA グローバル変数の初期化：VgaTextMode::new() は const fn にできないため、Once を使用した遅延初期化が必須です。

#### [NEW] `kernel/driver/mod.rs`

```rust
//! デバイスドライバモジュール

pub mod serial;
pub mod vga;
pub mod keyboard;

pub use serial::{SerialPort, SERIAL1};
pub use vga::{VgaTextMode, init_vga, vga};
pub use keyboard::PS2Keyboard;
```

#### [NEW] `kernel/driver/serial.rs`

型安全な Serial ポートドライバ：

```rust
//! Serial ポートドライバ (UART 16550)
//!
//! CharDevice trait に基づいた型安全な実装。

use crate::kernel::core::{Device, CharDevice, KernelResult, DeviceError};
use crate::arch::x86_64::port::{Port, PortReadOnly};
use spin::Mutex;

/// Serial ポート (COM1)
pub struct SerialPort {
    data: Port<u8>,
    int_enable: Port<u8>,
    fifo_ctrl: Port<u8>,
    line_ctrl: Port<u8>,
    modem_ctrl: Port<u8>,
    line_status: PortReadOnly<u8>,
}

impl SerialPort {
    /// COM1 を作成 (0x3F8)
    pub const fn com1() -> Self {
        Self {
            data: Port::new(0x3F8),
            int_enable: Port::new(0x3F8 + 1),
            fifo_ctrl: Port::new(0x3F8 + 2),
            line_ctrl: Port::new(0x3F8 + 3),
            modem_ctrl: Port::new(0x3F8 + 4),
            line_status: PortReadOnly::new(0x3F8 + 5),
        }
    }
    
    /// 送信バッファが空か確認
    fn is_tx_empty(&self) -> bool {
        unsafe { self.line_status.read() & 0x20 != 0 }
    }
}

impl Device for SerialPort {
    fn name(&self) -> &str {
        "COM1"
    }
    
    fn init(&mut self) -> KernelResult<()> {
        unsafe {
            // 割り込み無効化
            self.int_enable.write(0x00);
            // 9600 baud, 8N1 設定
            self.line_ctrl.write(0x80);
            self.data.write(0x03);
            self.int_enable.write(0x00);
            self.line_ctrl.write(0x03);
            // FIFO 有効化
            self.fifo_ctrl.write(0xC7);
            // DTR/RTS 設定
            self.modem_ctrl.write(0x0B);
        }
        Ok(())
    }
    
    fn reset(&mut self) -> KernelResult<()> {
        self.init()
    }
}

impl CharDevice for SerialPort {
    fn read_byte(&self) -> KernelResult<Option<u8>> {
        unsafe {
            if self.line_status.read() & 0x01 != 0 {
                Ok(Some(self.data.read()))
            } else {
                Ok(None)
            }
        }
    }
    
    fn write_byte(&mut self, byte: u8) -> KernelResult<()> {
        // 送信バッファが空になるまで待機
        while !self.is_tx_empty() {
            core::hint::spin_loop();
        }
        unsafe {
            self.data.write(byte);
        }
        Ok(())
    }
}

/// グローバル Serial ポート (const 初期化可能)
pub static SERIAL1: Mutex<SerialPort> = Mutex::new(SerialPort::com1());
```

#### [NEW] `kernel/driver/vga.rs`

型安全な VGA テキストモードドライバ（Once パターンで遅延初期化）：

```rust
//! VGA テキストモードドライバ
//!
//! CharDevice trait に基づいた型安全な実装。
//! グローバル変数は Once を使用して遅延初期化します。

use crate::kernel::core::{Device, CharDevice, KernelResult, DeviceError};
use core::fmt;
use spin::{Mutex, Once};

const VGA_BUFFER_ADDR: usize = 0xb8000;
const VGA_WIDTH: usize = 80;
const VGA_HEIGHT: usize = 25;

/// VGA 色
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Color {
    Black = 0,
    Blue = 1,
    Green = 2,
    Cyan = 3,
    Red = 4,
    Magenta = 5,
    Brown = 6,
    LightGray = 7,
    DarkGray = 8,
    LightBlue = 9,
    LightGreen = 10,
    LightCyan = 11,
    LightRed = 12,
    Pink = 13,
    Yellow = 14,
    White = 15,
}

/// VGA カラーコード
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
struct ColorCode(u8);

impl ColorCode {
    const fn new(fg: Color, bg: Color) -> Self {
        ColorCode((bg as u8) << 4 | (fg as u8))
    }
}

/// VGA 文字
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
struct ScreenChar {
    ascii: u8,
    color: ColorCode,
}

/// VGA バッファ
#[repr(transparent)]
struct Buffer {
    chars: [[ScreenChar; VGA_WIDTH]; VGA_HEIGHT],
}

/// VGA テキストモードドライバ
pub struct VgaTextMode {
    col: usize,
    row: usize,
    color: ColorCode,
    buffer: &'static mut Buffer,
}

impl VgaTextMode {
    /// 新しい VGA ドライバを作成（const fn ではない）
    pub fn new() -> Self {
        Self {
            col: 0,
            row: 0,
            color: ColorCode::new(Color::White, Color::Black),
            buffer: unsafe { &mut *(VGA_BUFFER_ADDR as *mut Buffer) },
        }
    }
    
    /// 画面をクリア
    pub fn clear_screen(&mut self) {
        let blank = ScreenChar {
            ascii: b' ',
            color: self.color,
        };
        for row in 0..VGA_HEIGHT {
            for col in 0..VGA_WIDTH {
                self.buffer.chars[row][col] = blank;
            }
        }
        self.col = 0;
        self.row = 0;
    }
    
    /// 改行
    fn newline(&mut self) {
        self.col = 0;
        self.row += 1;
        if self.row >= VGA_HEIGHT {
            // スクロール
            for row in 1..VGA_HEIGHT {
                for col in 0..VGA_WIDTH {
                    self.buffer.chars[row - 1][col] = self.buffer.chars[row][col];
                }
            }
            self.row = VGA_HEIGHT - 1;
            let blank = ScreenChar {
                ascii: b' ',
                color: self.color,
            };
            for col in 0..VGA_WIDTH {
                self.buffer.chars[self.row][col] = blank;
            }
        }
    }
}

impl Device for VgaTextMode {
    fn name(&self) -> &str {
        "VGA Text Mode"
    }
    
    fn init(&mut self) -> KernelResult<()> {
        self.clear_screen();
        Ok(())
    }
    
    fn reset(&mut self) -> KernelResult<()> {
        self.clear_screen();
        Ok(())
    }
}

impl CharDevice for VgaTextMode {
    fn read_byte(&self) -> KernelResult<Option<u8>> {
        // VGA は書き込み専用
        Err(DeviceError::NotFound.into())
    }
    
    fn write_byte(&mut self, byte: u8) -> KernelResult<()> {
        match byte {
            b'\n' => self.newline(),
            byte => {
                if self.col >= VGA_WIDTH {
                    self.newline();
                }
                self.buffer.chars[self.row][self.col] = ScreenChar {
                    ascii: byte,
                    color: self.color,
                };
                self.col += 1;
            }
        }
        Ok(())
    }
}

impl fmt::Write for VgaTextMode {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for byte in s.bytes() {
            self.write_byte(byte).map_err(|_| fmt::Error)?;
        }
        Ok(())
    }
}

/// グローバル VGA ドライバ（遅延初期化）
/// 
/// VgaTextMode::new() は const fn でないため、Once を使用します。
pub static VGA: Once<Mutex<VgaTextMode>> = Once::new();

/// VGA ドライバを初期化
/// 
/// カーネル起動時に一度だけ呼び出す必要があります。
pub fn init_vga() -> KernelResult<()> {
    VGA.call_once(|| {
        let mut vga = VgaTextMode::new();
        vga.init().expect("VGA initialization failed");
        Mutex::new(vga)
    });
    Ok(())
}

/// VGA ドライバにアクセス
/// 
/// init_vga() が呼ばれていない場合は panic します。
pub fn vga() -> &'static Mutex<VgaTextMode> {
    VGA.get().expect("VGA not initialized. Call init_vga() first.")
}
```

#### [NEW] `kernel/driver/keyboard.rs`

簡易的な PS/2 キーボードドライバ：

```rust
//! PS/2 キーボードドライバ
//!
//! Device trait に基づいた実装。

use crate::kernel::core::{Device, KernelResult};
use crate::arch::x86_64::port::PortReadOnly;

/// PS/2 キーボード
pub struct PS2Keyboard {
    data_port: PortReadOnly<u8>,
}

impl PS2Keyboard {
    pub const fn new() -> Self {
        Self {
            data_port: PortReadOnly::new(0x60),
        }
    }
    
    /// スキャンコードを読み取り
    pub fn read_scancode(&self) -> Option<u8> {
        unsafe {
            Some(self.data_port.read())
        }
    }
}

impl Device for PS2Keyboard {
    fn name(&self) -> &str {
        "PS/2 Keyboard"
    }
    
    fn init(&mut self) -> KernelResult<()> {
        // PS/2 キーボードは通常 BIOS/UEFI で初期化済み
        Ok(())
    }
    
    fn reset(&mut self) -> KernelResult<()> {
        Ok(())
    }
}
```

### Phase 3: 型安全な MMIO アクセス

デバイスレジスタを型で表現し、unsafe を隠蔽します。

NOTE

改善点：アドレス検証付きコンストラクタを追加し、BitField を複数の整数型に対応させました。

#### [NEW] `kernel/mmio.rs`

```rust
//! 型安全な MMIO（Memory-Mapped I/O）抽象化
//!
//! デバイスレジスタを型で表現し、unsafe を最小限に閉じ込めます。

use core::marker::PhantomData;
use core::ptr;
use crate::kernel::core::{KernelResult, MemoryError};

/// 型安全な MMIO レジスタ
#[repr(transparent)]
pub struct MmioReg<T> {
    addr: usize,
    _phantom: PhantomData<T>,
}

impl<T: Copy> MmioReg<T> {
    /// 新しい MMIO レジスタを作成（アドレス検証なし）
    /// 
    /// # Safety
    /// 
    /// addr は有効な MMIO アドレスである必要があります。
    /// また、適切にアライメントされている必要があります。
    pub const unsafe fn new_unchecked(addr: usize) -> Self {
        Self {
            addr,
            _phantom: PhantomData,
        }
    }
    
    /// 検証付きで MMIO レジスタを作成
    /// 
    /// このメソッドは以下をチェックします：
    /// - ヌルポインタでない
    /// - 適切にアライメントされている
    /// - 最小 MMIO アドレス（0x1000）以上
    pub fn new_checked(addr: usize) -> KernelResult<Self> {
        // ヌルポインタチェック
        if addr == 0 {
            return Err(MemoryError::InvalidAddress.into());
        }
        
        // アライメントチェック
        if addr % core::mem::align_of::<T>() != 0 {
            return Err(MemoryError::MisalignedAccess.into());
        }
        
        // 最小 MMIO アドレス（通常 0x1000 以上）
        if addr < 0x1000 {
            return Err(MemoryError::InvalidAddress.into());
        }
        
        Ok(Self {
            addr,
            _phantom: PhantomData,
        })
    }
    
    /// レジスタから読み取り
    /// 
    /// # Safety
    /// 
    /// このレジスタのアドレスが有効であることを保証する必要があります。
    pub unsafe fn read(&self) -> T {
        ptr::read_volatile(self.addr as *const T)
    }
    
    /// レジスタに書き込み
    /// 
    /// # Safety
    /// 
    /// このレジスタのアドレスが有効であることを保証する必要があります。
    pub unsafe fn write(&mut self, value: T) {
        ptr::write_volatile(self.addr as *mut T, value)
    }
}

/// ビットフィールド操作用のヘルパー trait
/// 
/// 複数の整数型に対応したジェネリック実装。
pub trait BitField: Sized + Copy {
    fn set_bit(&mut self, bit: u32);
    fn clear_bit(&mut self, bit: u32);
    fn is_set(&self, bit: u32) -> bool;
}

/// BitField trait を複数の整数型に一括実装
macro_rules! impl_bitfield {
    ($($t:ty),*) => {
        $(
            impl BitField for $t {
                fn set_bit(&mut self, bit: u32) {
                    *self |= 1 << bit;
                }
                
                fn clear_bit(&mut self, bit: u32) {
                    *self &= !(1 << bit);
                }
                
                fn is_set(&self, bit: u32) -> bool {
                    (*self & (1 << bit)) != 0
                }
            }
        )*
    };
}

// u8, u16, u32, u64, usize に BitField を実装
impl_bitfield!(u8, u16, u32, u64, usize);
```

デバイスドライバでの使用例

```rust
use crate::kernel::mmio::{MmioReg, BitField};

struct UartControl {
    control: MmioReg<u32>,
    data: MmioReg<u8>,
}

const UART_TX_ENABLE_BIT: u32 = 0;

impl UartControl {
    fn new() -> KernelResult<Self> {
        Ok(Self {
            control: MmioReg::new_checked(0x10000000)?,
            data: MmioReg::new_checked(0x10000004)?,
        })
    }
    
    fn enable_tx(&mut self) {
        unsafe {
            let mut val = self.control.read();
            val.set_bit(UART_TX_ENABLE_BIT);
            self.control.write(val);
        }
    }
}
```

### Phase 4: メモリ管理の拡張

WARNING

重要な修正：PageMapping にライフタイム 'pt を追加し、ページテーブルへの参照を保持します。 これにより、グローバル関数への依存をなくし、並行性の問題を回避します。

#### [NEW] `kernel/mm/`

新しいメモリ管理サブシステム：

- `paging.rs` - 型安全なページテーブル管理
- `allocator.rs` - カスタムヒープアロケータ
- `frame.rs` - 物理フレーム管理

#### [NEW] `kernel/mm/paging.rs`

```rust
//! ページング管理
//!
//! ライフタイムベースのページマッピングで安全性を保証。

use core::marker::PhantomData;
use crate::kernel::core::{KernelResult, MemoryError};

/// 仮想アドレス
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct VirtAddr(pub usize);

/// 物理アドレス
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct PhysAddr(pub usize);

/// ページテーブルフラグ
#[derive(Debug, Clone, Copy)]
pub struct PageTableFlags {
    bits: u64,
}

impl PageTableFlags {
    pub const PRESENT: Self = Self { bits: 1 << 0 };
    pub const WRITABLE: Self = Self { bits: 1 << 1 };
    pub const USER: Self = Self { bits: 1 << 2 };
    
    pub const fn empty() -> Self {
        Self { bits: 0 }
    }
    
    pub const fn new(bits: u64) -> Self {
        Self { bits }
    }
}

/// ページテーブルエントリを型で表現
#[repr(transparent)]
struct PageTableEntry(u64);

impl PageTableEntry {
    const PRESENT: u64 = 1 << 0;
    const WRITABLE: u64 = 1 << 1;
    const USER: u64 = 1 << 2;
    
    fn set_present(&mut self, present: bool) {
        if present {
            self.0 |= Self::PRESENT;
        } else {
            self.0 &= !Self::PRESENT;
        }
    }
    
    fn is_present(&self) -> bool {
        (self.0 & Self::PRESENT) != 0
    }
}

/// ページテーブル（簡易版）
pub struct PageTable {
    // 実際の実装は省略
}

impl PageTable {
    /// ページをマップ
    /// 
    /// # Safety
    /// 
    /// virt と phys は有効なアドレスである必要があります。
    pub unsafe fn map_page(
        &mut self,
        virt: VirtAddr,
        phys: PhysAddr,
        flags: PageTableFlags,
    ) -> KernelResult<()> {
        // 実装は省略
        Ok(())
    }
    
    /// ページをアンマップ
    /// 
    /// # Safety
    /// 
    /// virt は現在マップされているアドレスである必要があります。
    pub unsafe fn unmap_page(&mut self, virt: VirtAddr) -> KernelResult<()> {
        // 実装は省略
        Ok(())
    }
}

/// ページテーブルへの参照を保持するページマッピング
/// 
/// ライフタイム `'pt` により、ページテーブルの所有権を管理します。
/// Drop 時に自動的にアンマップされます。
pub struct PageMapping<'pt> {
    virt: VirtAddr,
    phys: PhysAddr,
    page_table: &'pt mut PageTable,
    _phantom: PhantomData<&'pt mut PageTable>,
}

impl<'pt> PageMapping<'pt> {
    /// 新しいページマッピングを作成
    /// 
    /// # Safety
    /// 
    /// virt と phys は有効なアドレスである必要があります。
    pub unsafe fn new(
        page_table: &'pt mut PageTable,
        virt: VirtAddr,
        phys: PhysAddr,
        flags: PageTableFlags,
    ) -> KernelResult<Self> {
        page_table.map_page(virt, phys, flags)?;
        Ok(Self {
            virt,
            phys,
            page_table,
            _phantom: PhantomData,
        })
    }
    
    /// 仮想アドレスを取得
    pub fn virt_addr(&self) -> VirtAddr {
        self.virt
    }
    
    /// 物理アドレスを取得
    pub fn phys_addr(&self) -> PhysAddr {
        self.phys
    }
}

impl Drop for PageMapping<'_> {
    fn drop(&mut self) {
        // SAFETY: このマッピングを作成したので、アンマップも安全
        unsafe {
            self.page_table.unmap_page(self.virt)
                .expect("Failed to unmap page during drop");
        }
    }
}
```

### Phase 5: タスク/スケジューリング基盤

#### [NEW] `kernel/task/`

- `mod.rs` - タスク管理
- `scheduler.rs` - スケジューラ実装
- `context.rs` - コンテキストスイッチ

#### [NEW] `kernel/task/mod.rs`

```rust
//! タスク管理

use crate::kernel::core::{Task, TaskId, TaskState, Priority};

/// 簡易タスク実装（デモ用）
pub struct SimpleTask {
    id: TaskId,
    priority: Priority,
    state: TaskState,
    name: &'static str,
}

impl SimpleTask {
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
    
    fn name(&self) -> &str {
        self.name
    }
    
    fn state(&self) -> TaskState {
        self.state
    }
}
```

### Phase 6: 非同期処理基盤（将来拡張）

#### [NEW] `kernel/async/`

- `executor.rs` - Future executor
- `waker.rs` - Waker 実装
- `timer.rs` - 非同期タイマー

```rust
//! 非同期処理基盤（将来実装）
//!
//! 現時点では基本構造のみ定義。

// TODO: Future executor の実装
// TODO: Waker の実装
// TODO: 非同期 I/O の実装
```

### Phase 7: 新しいモジュール構造

最終的なディレクトリ構造：

```
src/
├─ kernel/
│  ├─ core/           # コア抽象化
│  │  ├─ traits.rs
│  │  ├─ types.rs
│  │  └─ result.rs
│  ├─ mm/             # メモリ管理
│  │  ├─ paging.rs
│  │  ├─ allocator.rs
│  │  └─ frame.rs
│  ├─ task/           # タスク管理
│  │  ├─ mod.rs
│  │  ├─ scheduler.rs
│  │  └─ context.rs
│  ├─ driver/         # デバイスドライバ
│  │  ├─ mod.rs
│  │  ├─ serial.rs
│  │  ├─ vga.rs
│  │  └─ keyboard.rs
│  ├─ mmio.rs         # MMIO 抽象化
│  └─ async/          # 非同期処理（将来）
├─ arch/              # アーキテクチャ依存
│  └─ x86_64/
│      ├─ mod.rs
│      └─ port.rs     # ポート I/O
├─ display/           # 表示抽象化（既存・移行予定）
└─ main.rs
```

## 設計方針（ユーザー承認済み）

NOTE

以下の設計方針でユーザー承認を得ました：

✅ **1. 完全書き直し**
新しい `kernel/` モジュールに一から実装。既存コードはレガシーとして保持し、後で削除。

✅ **2. trait ベースの統一アーキテクチャ**
すべてのデバイスドライバを Device, CharDevice, BlockDevice trait で抽象化。

✅ **3. メモリアロケータ**
最初は linked_list_allocator で実装。後で独自アロケータに置き換え可能な設計。

✅ **4. 非同期処理**
Phase 6 として実装。まずは同期的なタスク管理から開始。

✅ **5. Task trait の再設計**
TaskState ベースの設計により、プリエンプティブマルチタスクに対応可能。

✅ **6. エラーコンテキスト情報**
KernelError にコンテキスト文字列を保持し、デバッグを容易に。

✅ **7. グローバル変数の遅延初期化**
Once パターンで VGA ドライバを遅延初期化。

✅ **8. ライフタイムベースのリソース管理**
PageMapping<'pt> でページテーブルへの参照を管理。

## 検証計画

### 自動テスト

各フェーズ後に以下を実行：

```bash
# ビルドテスト
cargo build --target x86_64-blog_os.json
# 単体テスト（std-tests feature）
cargo test --features std-tests
# 統合テスト
cargo test --test '*'
```

### 手動検証

QEMU で起動確認：

```bash
make run
```

確認項目：
✅ VGA 出力が正常
✅ Serial ポートが動作
✅ Keyboard 入力が反応
✅ Panic ハンドラが機能

### ベンチマーク

パフォーマンス劣化がないことを確認：

- ブート時間
- デバイス I/O レイテンシ

## リスクと対策

- **リスク 1: ビルド失敗**
  - 対策: 各フェーズで小さな変更を積み重ね、頻繁にビルドテスト
- **リスク 2: パフォーマンス劣化**
  - 対策: trait の動的ディスパッチを避け、ジェネリクスで静的ディスパッチ
- **リスク 3: unsafe の増加**
  - 対策: unsafe ブロックを最小限に、必ず SAFETY コメントを追加
- **リスク 4: 複雑性の増大**
  - 対策: 各モジュールを独立させ、依存関係を最小化

## タイムライン（最終版）

| フェーズ | 作業内容 | 推定時間 |
| --- | --- | --- |
| Phase 1 | コア抽象化（traits, types, result） | 30分 |
| Phase 1.5 | ポートI/O抽象化（arch/x86_64/port.rs） | 20分 |
| Phase 2 | デバイスドライバ実装（VGA 遅延初期化含む） | 1.5時間 |
| Phase 3 | MMIO 抽象化（検証付きコンストラクタ、ジェネリック BitField） | 1時間 |
| Phase 4 | メモリ管理拡張（ライフタイム付き PageMapping） | 2.5時間 |
| Phase 5 | タスク/スケジューリング | 1.5時間 |
| Phase 6 | 非同期処理（オプション） | 2時間 |
| Phase 7 | テスト・ドキュメント作成 | 1時間 |
| **合計** | | **7〜8.5時間**（Phase 6 除く: 5.5〜6.5時間） |

## 次のステップ

✅ この実装計画（最終版）をレビュー
⏳ ユーザー承認待ち
Phase 1 の実装開始

## 監査フィードバック対応まとめ

以下の8つの改善点をすべて反映しました：
✅ Phase 1.5 追加: arch/x86_64/port.rs の完全な実装
✅ VGA 遅延初期化: Once パターンで init_vga() / `vga()` 関数
✅ PageMapping ライフタイム: <'pt> でページテーブル参照を管理
✅ result.rs 完成: From<ErrorKind> 実装を追加
✅ BitField ジェネリック: マクロで u8/u16/u32/u64/usize に対応
✅ MMIO 検証: new_checked() でアドレス検証
✅ Task trait 再設計: TaskState ベースでコンテキストスイッチ対応
✅ タイムライン調整: 7〜8.5時間に更新
