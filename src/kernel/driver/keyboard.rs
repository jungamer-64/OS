// src/kernel/driver/keyboard.rs
//! PS/2 キーボードドライバ
//!
//! CharDevice trait に基づいた型安全な実装。

use crate::kernel::core::{Device, CharDevice, KernelResult};
use crate::arch::x86_64::port::{PortReadOnly, PortWriteOnly};

/// PS/2 キーボード
pub struct PS2Keyboard {
    data: PortReadOnly<u8>,
    status: PortReadOnly<u8>,
    command: PortWriteOnly<u8>,
}

impl PS2Keyboard {
    /// 新しいキーボードドライバを作成
    pub const fn new() -> Self {
        Self {
            data: PortReadOnly::new(0x60),
            status: PortReadOnly::new(0x64),
            command: PortWriteOnly::new(0x64),
        }
    }
    
    /// ステータスレジスタを読み取り
    fn read_status(&self) -> u8 {
        unsafe { self.status.read() }
    }
}

impl Device for PS2Keyboard {
    fn name(&self) -> &str {
        "PS/2 Keyboard"
    }
    
    fn init(&mut self) -> KernelResult<()> {
        // 初期化ロジック（必要なら）
        // コントローラのリセットなどはここで行う
        Ok(())
    }
    
    fn reset(&mut self) -> KernelResult<()> {
        self.init()
    }
}

impl CharDevice for PS2Keyboard {
    fn read_byte(&self) -> KernelResult<Option<u8>> {
        let status = self.read_status();
        // 出力バッファフルビット (bit 0) を確認
        if status & 0x01 != 0 {
            let scancode = unsafe { self.data.read() };
            Ok(Some(scancode))
        } else {
            Ok(None)
        }
    }
    
    fn write_byte(&mut self, _byte: u8) -> KernelResult<()> {
        // キーボードへの書き込みは通常コマンド送信だが、
        // CharDevice としてはサポートしない（またはLED制御などに使う）
        Ok(())
    }
}
