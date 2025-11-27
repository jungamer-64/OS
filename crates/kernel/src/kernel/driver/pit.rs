//! Programmable Interval Timer (PIT)
//!
//! システムタイマーの設定を行います。

use crate::kernel::core::{Device, KernelResult};
use crate::arch::x86_64::port::{Port, PortWriteOnly};
use spin::Mutex;

/// PIT のベース周波数 (Hz)
const PIT_FREQUENCY: u32 = 1_193_182;

/// チャンネル 0 データポート
const CHANNEL0_DATA: u16 = 0x40;
/// コマンドポート
const COMMAND_PORT: u16 = 0x43;

/// Programmable Interval Timer
pub struct ProgrammableIntervalTimer {
    channel0: Port<u8>,
    command: PortWriteOnly<u8>,
}

impl Default for ProgrammableIntervalTimer {
    fn default() -> Self {
        Self::new()
    }
}

impl ProgrammableIntervalTimer {
    /// 新しい PIT ドライバを作成
    pub const fn new() -> Self {
        Self {
            channel0: Port::new(CHANNEL0_DATA),
            command: PortWriteOnly::new(COMMAND_PORT),
        }
    }

    /// 周波数を設定
    pub fn set_frequency(&mut self, freq: u32) -> KernelResult<()> {
        let divisor = PIT_FREQUENCY / freq;
        
        // 実際の周波数が高すぎる場合はエラーにすべきだが、
        // ここでは単純に u16 に収まるようにする
        let divisor = if divisor > 65535 { 65535 } else { divisor as u16 };

        // SAFETY: PITのコマンドポート(0x43)とチャネル0データポート(0x40)への書き込みは
        // PC/AT互換機の標準タイマー設定手順。モード3（矩形波）での設定。
        unsafe {
            // モード設定: Channel 0, Access lo/hi, Mode 3 (Square Wave), Binary
            // 00 11 011 0 = 0x36
            self.command.write(0x36);
            
            // Divisor を送信 (Low byte, then High byte)
            self.channel0.write((divisor & 0xFF) as u8);
            self.channel0.write((divisor >> 8) as u8);
        }
        
        Ok(())
    }
}

impl Device for ProgrammableIntervalTimer {
    fn name(&self) -> &'static str {
        "Intel 8253/8254 PIT"
    }

    fn init(&mut self) -> KernelResult<()> {
        // デフォルトで 100Hz に設定
        self.set_frequency(100)
    }

    fn reset(&mut self) -> KernelResult<()> {
        self.init()
    }
}

/// グローバル PIT インスタンス
pub static PIT: Mutex<ProgrammableIntervalTimer> = Mutex::new(ProgrammableIntervalTimer::new());
