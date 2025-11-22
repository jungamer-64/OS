// src/kernel/driver/keyboard.rs
//! PS/2 キーボードドライバ
//!
//! CharDevice trait に基づいた型安全な実装。

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

use spin::Mutex;
use alloc::collections::VecDeque;
use core::task::{Waker, Poll, Context};
use core::pin::Pin;
use core::future::Future;
// use crossbeam_queue::ArrayQueue; // Not available, using VecDeque with Mutex

/// スキャンコードキュー
pub struct ScancodeQueue {
    queue: VecDeque<u8>,
    waker: Option<Waker>,
}

impl ScancodeQueue {
    pub const fn new() -> Self {
        Self {
            queue: VecDeque::new(),
            waker: None,
        }
    }

    /// スキャンコードを追加し、待機中のタスクを起こす
    pub fn add_scancode(&mut self, scancode: u8) {
        self.queue.push_back(scancode);
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }

    /// 次のスキャンコードを取得（非同期）
    pub fn next_scancode(&mut self) -> Option<u8> {
        self.queue.pop_front()
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
pub struct ScancodeStream {
    _private: (),
}

impl ScancodeStream {
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


