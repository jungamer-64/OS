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

/// VGA 4ビット色（型安全なラッパー）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct Color4Bit(u8);

impl Color4Bit {
    pub const BLACK: Self = Self(0);
    pub const BLUE: Self = Self(1);
    pub const GREEN: Self = Self(2);
    pub const CYAN: Self = Self(3);
    pub const RED: Self = Self(4);
    pub const MAGENTA: Self = Self(5);
    pub const BROWN: Self = Self(6);
    pub const LIGHT_GRAY: Self = Self(7);
    pub const DARK_GRAY: Self = Self(8);
    pub const LIGHT_BLUE: Self = Self(9);
    pub const LIGHT_GREEN: Self = Self(10);
    pub const LIGHT_CYAN: Self = Self(11);
    pub const LIGHT_RED: Self = Self(12);
    pub const PINK: Self = Self(13);
    pub const YELLOW: Self = Self(14);
    pub const WHITE: Self = Self(15);
    
    /// 新しい4ビット色を作成（境界チェック付き）
    #[inline]
    #[must_use]
    pub const fn new(value: u8) -> Option<Self> {
        if value <= 15 {
            Some(Self(value))
        } else {
            None
        }
    }
    
    /// 内部値を取得
    #[inline]
    #[must_use]
    pub const fn value(self) -> u8 {
        self.0
    }
}

/// VGA カラーコード（前景色と背景色の組み合わせ）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct VgaColor(u8);

impl VgaColor {
    /// 新しいVGAカラーコードを作成
    #[inline]
    pub const fn new(fg: Color4Bit, bg: Color4Bit) -> Self {
        VgaColor((bg.0 << 4) | fg.0)
    }
    
    /// デフォルト色（白字に黒背景）
    pub const DEFAULT: Self = VgaColor::new(Color4Bit::WHITE, Color4Bit::BLACK);
    
    /// 前景色を取得
    #[inline]
    pub const fn foreground(self) -> Color4Bit {
        Color4Bit(self.0 & 0x0F)
    }
    
    /// 背景色を取得
    #[inline]
    pub const fn background(self) -> Color4Bit {
        Color4Bit((self.0 >> 4) & 0x0F)
    }
    
    /// 内部値を取得
    #[inline]
    #[allow(dead_code)]
    const fn value(self) -> u8 {
        self.0
    }
}

/// VGA 文字（文字コードとカラーコードの組み合わせ）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
struct VgaChar {
    ascii: u8,
    color: VgaColor,
}

impl VgaChar {
    /// 新しいVGA文字を作成
    #[inline]
    const fn new(ascii: u8, color: VgaColor) -> Self {
        VgaChar { ascii, color }
    }
    
    /// 空白文字を作成
    #[inline]
    const fn blank(color: VgaColor) -> Self {
        VgaChar::new(b' ', color)
    }
}

/// VGA 位置（境界チェック付き）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VgaPosition {
    col: usize,
    row: usize,
}

impl VgaPosition {
    /// 新しい位置を作成（境界チェック付き）
    #[inline]
    pub const fn new(col: usize, row: usize) -> Option<Self> {
        if col < VGA_WIDTH && row < VGA_HEIGHT {
            Some(VgaPosition { col, row })
        } else {
            None
        }
    }
    
    /// 原点(0, 0)を返す
    #[inline]
    pub const fn origin() -> Self {
        VgaPosition { col: 0, row: 0 }
    }
    
    /// 列番号を取得
    #[inline]
    pub const fn col(self) -> usize {
        self.col
    }
    
    /// 行番号を取得
    #[inline]
    pub const fn row(self) -> usize {
        self.row
    }
    
    /// 次の列に移動（境界チェック付き）
    #[inline]
    fn next_col(self) -> Option<Self> {
        if self.col + 1 < VGA_WIDTH {
            Some(VgaPosition { col: self.col + 1, row: self.row })
        } else {
            None
        }
    }
    
    /// 次の行の先頭に移動（境界チェック付き）
    #[inline]
    fn next_row(self) -> Option<Self> {
        if self.row + 1 < VGA_HEIGHT {
            Some(VgaPosition { col: 0, row: self.row + 1 })
        } else {
            None
        }
    }
}

/// VGA バッファ
#[repr(transparent)]
struct Buffer {
    chars: [[VgaChar; VGA_WIDTH]; VGA_HEIGHT],
}

/// VGA テキストモードドライバ
pub struct VgaTextMode {
    position: VgaPosition,
    color: VgaColor,
    buffer: &'static mut Buffer,
}

impl Default for VgaTextMode {
    fn default() -> Self {
        Self::new()
    }
}

impl VgaTextMode {
    /// 新しい VGA ドライバを作成
    /// 
    /// # Safety
    /// 
    /// `VGA_BUFFER_ADDR` が有効なVGAテキストバッファを指していることを前提とします。
    /// この関数はカーネル初期化時に一度だけ呼び出されるべきです。
    /// 
    /// # Panics
    /// 
    /// VGAバッファアドレスが無効な場合にパニックします。
    #[allow(clippy::assertions_on_constants)]
    #[must_use]
    pub fn new() -> Self {
        // VGAバッファアドレスの基本的な妥当性チェック
        // 実際のハードウェアでは 0xB8000 が標準的なVGAテキストバッファアドレス
        const { assert!(VGA_BUFFER_ADDR != 0, "VGA buffer address cannot be null") };
        const { assert!(VGA_BUFFER_ADDR >= 0x1000, "VGA buffer address too low") };
        assert!(
            VGA_BUFFER_ADDR.is_multiple_of(core::mem::align_of::<Buffer>()),
            "VGA buffer address must be properly aligned"
        );
        
        Self {
            position: VgaPosition::origin(),
            color: VgaColor::DEFAULT,
            // Safety: 上記のアサーションでアドレスの基本的な妥当性を確認済み
            // VGA_BUFFER_ADDR は定数として定義されており、カーネル初期化時に
            // 適切なメモリマップが設定されていることが前提
            buffer: unsafe { &mut *(VGA_BUFFER_ADDR as *mut Buffer) },
        }
    }
    
    /// 画面をクリア
    pub fn clear_screen(&mut self) {
        let blank = VgaChar::blank(self.color);
        for row in 0..VGA_HEIGHT {
            for col in 0..VGA_WIDTH {
                self.buffer.chars[row][col] = blank;
            }
        }
        self.position = VgaPosition::origin();
    }
    
    /// 改行
    fn newline(&mut self) {
        if let Some(next_pos) = self.position.next_row() {
            self.position = next_pos;
        } else {
            // スクロール
            for row in 1..VGA_HEIGHT {
                for col in 0..VGA_WIDTH {
                    self.buffer.chars[row - 1][col] = self.buffer.chars[row][col];
                }
            }
            let blank = VgaChar::blank(self.color);
            for col in 0..VGA_WIDTH {
                self.buffer.chars[VGA_HEIGHT - 1][col] = blank;
            }
            self.position = VgaPosition::new(0, VGA_HEIGHT - 1)
                .expect("VGA position must be valid");
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
                let pos = self.position;
                self.buffer.chars[pos.row()][pos.col()] = VgaChar::new(byte, self.color);
                
                if let Some(next_pos) = pos.next_col() {
                    self.position = next_pos;
                } else {
                    self.newline();
                }
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
/// `VgaTextMode::new()` は const fn でないため、Once を使用します。
pub static VGA: Once<Mutex<VgaTextMode>> = Once::new();

/// VGA ドライバを初期化
/// 
/// カーネル起動時に一度だけ呼び出す必要があります。
/// 
/// # Errors
/// 
/// VGAデバイスの初期化に失敗した場合に`Err`を返します。
/// 
/// # Panics
/// 
/// VGAデバイスの初期化に失敗した場合にパニックします。
pub fn init_vga() -> KernelResult<()> {
    VGA.call_once(|| {
        let mut vga = VgaTextMode::new();
        vga.init().expect(
            "VGA initialization failed. Check VGA hardware compatibility."
        );
        Mutex::new(vga)
    });
    Ok(())
}

/// VGA ドライバにアクセス
/// 
/// # Panics
/// 
/// `init_vga()` が呼ばれていない場合にパニックします。
/// カーネル起動時に必ず `init_vga()` を呼び出してください。
pub fn vga() -> &'static Mutex<VgaTextMode> {
    VGA.get().expect(
        "VGA not initialized. Call init_vga() during kernel initialization."
    )
}
