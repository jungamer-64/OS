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


