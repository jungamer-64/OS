// kernel/src/kernel/driver/keyboard.rs
//! PS/2 キーボードドライバ
//!
//! CharDevice trait に基づいた型安全な実装。
//! 非同期対応のスキャンコードストリームを提供。

use crate::kernel::core::{Device, KernelResult};
use crate::arch::x86_64::port::PortReadOnly;

/// PS/2 キーボード
pub struct PS2Keyboard {
    data_port: PortReadOnly<u8>,
}

impl Default for PS2Keyboard {
    fn default() -> Self {
        Self::new()
    }
}

impl PS2Keyboard {
    /// Creates a new PS/2 keyboard driver instance.
    pub const fn new() -> Self {
        Self {
            data_port: PortReadOnly::new(0x60),
        }
    }
    
    /// スキャンコードを読み取り
    pub fn read_scancode(&self) -> Option<u8> {
        // SAFETY: 0x60はPS/2キーボードのデータポート。
        // このポートからの読み取りは標準的なPC/AT互換機の操作。
        unsafe {
            Some(self.data_port.read())
        }
    }
}

impl Device for PS2Keyboard {
    fn name(&self) -> &'static str {
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

use spin::Mutex;
use alloc::collections::VecDeque;
use core::task::{Waker, Poll, Context};
use core::pin::Pin;
use core::future::Future;

/// スキャンコードキューの容量
const SCANCODE_QUEUE_CAPACITY: usize = 128;

/// スキャンコードキュー
pub struct ScancodeQueue {
    queue: VecDeque<u8>,
    waker: Option<Waker>,
}

impl Default for ScancodeQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl ScancodeQueue {
    /// Creates a new scancode queue.
    pub const fn new() -> Self {
        Self {
            queue: VecDeque::new(),
            waker: None,
        }
    }

    /// スキャンコードを追加し、待機中のタスクを起こす
    pub fn add_scancode(&mut self, scancode: u8) {
        // キャパシティを超える場合は古いものを捨てる
        if self.queue.len() >= SCANCODE_QUEUE_CAPACITY {
            self.queue.pop_front();
        }
        self.queue.push_back(scancode);
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }

    /// 次のスキャンコードを取得（同期）
    pub fn next_scancode(&mut self) -> Option<u8> {
        self.queue.pop_front()
    }
    
    /// スキャンコードが利用可能か確認
    pub fn has_scancode(&self) -> bool {
        !self.queue.is_empty()
    }
    
    /// キュー内のスキャンコード数
    pub fn len(&self) -> usize {
        self.queue.len()
    }
    
    /// キューが空か確認
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
    
    /// Waker を登録
    pub fn register_waker(&mut self, waker: &Waker) {
        self.waker = Some(waker.clone());
    }
}

/// グローバルキーボードインスタンス
pub static KEYBOARD: Mutex<PS2Keyboard> = Mutex::new(PS2Keyboard::new());

/// グローバルスキャンコードキュー
pub static SCANCODE_QUEUE: Mutex<ScancodeQueue> = Mutex::new(ScancodeQueue::new());

/// 次のスキャンコードを待つ Future
/// 
/// 1つのスキャンコードを非同期で待機します。
pub struct ScancodeStream {
    _private: (),
}

impl Default for ScancodeStream {
    fn default() -> Self {
        Self::new()
    }
}

impl ScancodeStream {
    /// Creates a new scancode stream for async keyboard input.
    pub fn new() -> Self {
        Self { _private: () }
    }
}

impl Future for ScancodeStream {
    type Output = u8;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut queue = SCANCODE_QUEUE.lock();
        
        if let Some(scancode) = queue.next_scancode() {
            Poll::Ready(scancode)
        } else {
            queue.register_waker(cx.waker());
            Poll::Pending
        }
    }
}

/// キーボード入力の非同期イテレータ
/// 
/// スキャンコードを連続的に非同期で取得します。
pub struct AsyncKeyboard {
    _private: (),
}

impl Default for AsyncKeyboard {
    fn default() -> Self {
        Self::new()
    }
}

impl AsyncKeyboard {
    /// Creates a new async keyboard iterator.
    pub fn new() -> Self {
        Self { _private: () }
    }
    
    /// 次のスキャンコードを非同期で取得
    pub async fn next_scancode(&self) -> u8 {
        ScancodeStream::new().await
    }
    
    /// 指定回数のスキャンコードを待って収集
    pub async fn read_n(&self, n: usize) -> alloc::vec::Vec<u8> {
        let mut result = alloc::vec::Vec::with_capacity(n);
        for _ in 0..n {
            result.push(self.next_scancode().await);
        }
        result
    }
}

/// 非同期でスキャンコードを待つヘルパー関数
pub async fn wait_for_scancode() -> u8 {
    ScancodeStream::new().await
}

/// 非同期でN個のスキャンコードを待つ
pub async fn wait_for_scancodes(n: usize) -> alloc::vec::Vec<u8> {
    AsyncKeyboard::new().read_n(n).await
}

// ============================================================================
// キーコード変換 (将来用)
// ============================================================================

/// スキャンコードセット1 (US キーボード)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum KeyCode {
    // Numbers
    Key1 = 0x02,
    Key2 = 0x03,
    Key3 = 0x04,
    Key4 = 0x05,
    Key5 = 0x06,
    Key6 = 0x07,
    Key7 = 0x08,
    Key8 = 0x09,
    Key9 = 0x0A,
    Key0 = 0x0B,
    
    // Letters (top row)
    Q = 0x10,
    W = 0x11,
    E = 0x12,
    R = 0x13,
    T = 0x14,
    Y = 0x15,
    U = 0x16,
    I = 0x17,
    O = 0x18,
    P = 0x19,
    
    // Letters (middle row)
    A = 0x1E,
    S = 0x1F,
    D = 0x20,
    F = 0x21,
    G = 0x22,
    H = 0x23,
    J = 0x24,
    K = 0x25,
    L = 0x26,
    
    // Letters (bottom row)
    Z = 0x2C,
    X = 0x2D,
    C = 0x2E,
    V = 0x2F,
    B = 0x30,
    N = 0x31,
    M = 0x32,
    
    // Special keys
    Escape = 0x01,
    Backspace = 0x0E,
    Tab = 0x0F,
    Enter = 0x1C,
    LeftCtrl = 0x1D,
    LeftShift = 0x2A,
    RightShift = 0x36,
    LeftAlt = 0x38,
    Space = 0x39,
    CapsLock = 0x3A,
    
    // Function keys
    F1 = 0x3B,
    F2 = 0x3C,
    F3 = 0x3D,
    F4 = 0x3E,
    F5 = 0x3F,
    F6 = 0x40,
    F7 = 0x41,
    F8 = 0x42,
    F9 = 0x43,
    F10 = 0x44,
    F11 = 0x57,
    F12 = 0x58,
}

impl KeyCode {
    /// スキャンコードからキーコードに変換
    pub fn from_scancode(scancode: u8) -> Option<Self> {
        match scancode {
            0x02 => Some(Self::Key1),
            0x03 => Some(Self::Key2),
            0x04 => Some(Self::Key3),
            0x05 => Some(Self::Key4),
            0x06 => Some(Self::Key5),
            0x07 => Some(Self::Key6),
            0x08 => Some(Self::Key7),
            0x09 => Some(Self::Key8),
            0x0A => Some(Self::Key9),
            0x0B => Some(Self::Key0),
            
            0x10 => Some(Self::Q),
            0x11 => Some(Self::W),
            0x12 => Some(Self::E),
            0x13 => Some(Self::R),
            0x14 => Some(Self::T),
            0x15 => Some(Self::Y),
            0x16 => Some(Self::U),
            0x17 => Some(Self::I),
            0x18 => Some(Self::O),
            0x19 => Some(Self::P),
            
            0x1E => Some(Self::A),
            0x1F => Some(Self::S),
            0x20 => Some(Self::D),
            0x21 => Some(Self::F),
            0x22 => Some(Self::G),
            0x23 => Some(Self::H),
            0x24 => Some(Self::J),
            0x25 => Some(Self::K),
            0x26 => Some(Self::L),
            
            0x2C => Some(Self::Z),
            0x2D => Some(Self::X),
            0x2E => Some(Self::C),
            0x2F => Some(Self::V),
            0x30 => Some(Self::B),
            0x31 => Some(Self::N),
            0x32 => Some(Self::M),
            
            0x01 => Some(Self::Escape),
            0x0E => Some(Self::Backspace),
            0x0F => Some(Self::Tab),
            0x1C => Some(Self::Enter),
            0x1D => Some(Self::LeftCtrl),
            0x2A => Some(Self::LeftShift),
            0x36 => Some(Self::RightShift),
            0x38 => Some(Self::LeftAlt),
            0x39 => Some(Self::Space),
            0x3A => Some(Self::CapsLock),
            
            0x3B => Some(Self::F1),
            0x3C => Some(Self::F2),
            0x3D => Some(Self::F3),
            0x3E => Some(Self::F4),
            0x3F => Some(Self::F5),
            0x40 => Some(Self::F6),
            0x41 => Some(Self::F7),
            0x42 => Some(Self::F8),
            0x43 => Some(Self::F9),
            0x44 => Some(Self::F10),
            0x57 => Some(Self::F11),
            0x58 => Some(Self::F12),
            
            _ => None,
        }
    }
    
    /// キーコードをASCII文字に変換 (Shift なし)
    pub fn to_ascii(&self, shift: bool) -> Option<char> {
        match self {
            Self::Key1 => Some(if shift { '!' } else { '1' }),
            Self::Key2 => Some(if shift { '@' } else { '2' }),
            Self::Key3 => Some(if shift { '#' } else { '3' }),
            Self::Key4 => Some(if shift { '$' } else { '4' }),
            Self::Key5 => Some(if shift { '%' } else { '5' }),
            Self::Key6 => Some(if shift { '^' } else { '6' }),
            Self::Key7 => Some(if shift { '&' } else { '7' }),
            Self::Key8 => Some(if shift { '*' } else { '8' }),
            Self::Key9 => Some(if shift { '(' } else { '9' }),
            Self::Key0 => Some(if shift { ')' } else { '0' }),
            
            Self::Q => Some(if shift { 'Q' } else { 'q' }),
            Self::W => Some(if shift { 'W' } else { 'w' }),
            Self::E => Some(if shift { 'E' } else { 'e' }),
            Self::R => Some(if shift { 'R' } else { 'r' }),
            Self::T => Some(if shift { 'T' } else { 't' }),
            Self::Y => Some(if shift { 'Y' } else { 'y' }),
            Self::U => Some(if shift { 'U' } else { 'u' }),
            Self::I => Some(if shift { 'I' } else { 'i' }),
            Self::O => Some(if shift { 'O' } else { 'o' }),
            Self::P => Some(if shift { 'P' } else { 'p' }),
            
            Self::A => Some(if shift { 'A' } else { 'a' }),
            Self::S => Some(if shift { 'S' } else { 's' }),
            Self::D => Some(if shift { 'D' } else { 'd' }),
            Self::F => Some(if shift { 'F' } else { 'f' }),
            Self::G => Some(if shift { 'G' } else { 'g' }),
            Self::H => Some(if shift { 'H' } else { 'h' }),
            Self::J => Some(if shift { 'J' } else { 'j' }),
            Self::K => Some(if shift { 'K' } else { 'k' }),
            Self::L => Some(if shift { 'L' } else { 'l' }),
            
            Self::Z => Some(if shift { 'Z' } else { 'z' }),
            Self::X => Some(if shift { 'X' } else { 'x' }),
            Self::C => Some(if shift { 'C' } else { 'c' }),
            Self::V => Some(if shift { 'V' } else { 'v' }),
            Self::B => Some(if shift { 'B' } else { 'b' }),
            Self::N => Some(if shift { 'N' } else { 'n' }),
            Self::M => Some(if shift { 'M' } else { 'm' }),
            
            Self::Space => Some(' '),
            Self::Tab => Some('\t'),
            Self::Enter => Some('\n'),
            
            _ => None,
        }
    }
}


