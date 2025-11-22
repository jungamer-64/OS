// src/kernel/driver/vga.rs
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
    /// 新しい VGA ドライバを作成
    /// 
    /// # Safety
    /// 
    /// VGA_BUFFER_ADDR が有効なVGAテキストバッファを指していることを前提とします。
    /// この関数はカーネル初期化時に一度だけ呼び出されるべきです。
    pub fn new() -> Self {
        // VGAバッファアドレスの基本的な妥当性チェック
        // 実際のハードウェアでは 0xB8000 が標準的なVGAテキストバッファアドレス
        assert!(VGA_BUFFER_ADDR != 0, "VGA buffer address cannot be null");
        assert!(VGA_BUFFER_ADDR >= 0x1000, "VGA buffer address too low");
        assert!(
            VGA_BUFFER_ADDR % core::mem::align_of::<Buffer>() == 0,
            "VGA buffer address must be properly aligned"
        );
        
        Self {
            col: 0,
            row: 0,
            color: ColorCode::new(Color::White, Color::Black),
            // Safety: 上記のアサーションでアドレスの基本的な妥当性を確認済み
            // VGA_BUFFER_ADDR は定数として定義されており、カーネル初期化時に
            // 適切なメモリマップが設定されていることが前提
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
    fn name(&self) -> &'static str {
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
