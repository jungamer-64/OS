// src/kernel/driver/serial.rs
//! Serial ポートドライバ (UART 16550)
//!
//! `CharDevice` trait に基づいた型安全な実装。

use crate::kernel::core::{Device, CharDevice, KernelResult};
use crate::kernel::core::result::DeviceError;
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
        // SAFETY: line_statusポート(0x3FD)からの読み取りは、UART 16550の標準レジスタ操作であり、
        // ビット5は送信ホールディングレジスタが空かどうかを示す標準的なステータスビット。
        unsafe { self.line_status.read() & 0x20 != 0 }
    }
}

impl Device for SerialPort {
    fn name(&self) -> &'static str {
        "COM1"
    }
    
    fn init(&mut self) -> KernelResult<()> {
        // SAFETY: UART 16550の初期化は標準的なI/Oポート操作のシーケンス。
        // 各ポートアドレス(0x3F8-0x3FC)はUART 16550仕様で定義されており、
        // これらのレジスタへの書き込みは安全。
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
        // SAFETY: line_statusポートからの読み取りでデータ受信準備完了を確認し、
        // dataポートからの読み取りを行う。これはUART 16550の標準的な読み取り手順。
        unsafe {
            if self.line_status.read() & 0x01 != 0 {
                Ok(Some(self.data.read()))
            } else {
                Ok(None)
            }
        }
    }
    
    fn write_byte(&mut self, byte: u8) -> KernelResult<()> {
        // 送信バッファが空になるまで待機（タイムアウト付き）
        const TIMEOUT: usize = 100_000;
        for _ in 0..TIMEOUT {
            if self.is_tx_empty() {
                // SAFETY: 送信バッファが空であることを確認済み。dataポートへの書き込みは
                // UART 16550の標準的な送信手順。
                unsafe {
                    self.data.write(byte);
                }
                return Ok(());
            }
            core::hint::spin_loop();
        }
        // タイムアウト: シリアルポートが応答しない
        Err(DeviceError::Timeout.into())
    }
}

use core::fmt;
impl fmt::Write for SerialPort {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for byte in s.bytes() {
            self.write_byte(byte).map_err(|_| fmt::Error)?;
        }
        Ok(())
    }
}

/// グローバル Serial ポート (const 初期化可能)
pub static SERIAL1: Mutex<SerialPort> = Mutex::new(SerialPort::com1());
