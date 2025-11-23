//! UEFI Framebuffer ドライバ
//!
//! ``bootloader_api`` から渡されるフレームバッファ情報を使用して描画を行います。
//! 簡易的なフォントレンダリング機能を持ちます。

use crate::kernel::core::{Device, CharDevice, KernelResult, DeviceError};
use bootloader_api::info::{FrameBufferInfo, PixelFormat};
use core::fmt;
use spin::{Mutex, Once};

/// The 8x16 font binary data, loaded at compile time.
static FONT: &[u8] = include_bytes!("../../../../assets/font/basic_8x16.bin");


/// Represents an RGB color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    /// Red component (0-255)
    pub r: u8,
    /// Green component (0-255)
    pub g: u8,
    /// Blue component (0-255)
    pub b: u8,
}

impl Color {
    /// Basic white color.
    pub const WHITE: Color = Color { r: 255, g: 255, b: 255 };
    /// Basic black color.
    pub const BLACK: Color = Color { r: 0, g: 0, b: 0 };
}

/// グローバルフレームバッファドライバ
pub static FRAMEBUFFER: Once<Mutex<Framebuffer>> = Once::new();

/// フレームバッファを初期化
pub fn init_framebuffer(info: FrameBufferInfo, buffer: &'static mut [u8]) {
    FRAMEBUFFER.call_once(|| {
        let fb = Framebuffer::new(info, buffer);
        Mutex::new(fb)
    });
}

/// フレームバッファにアクセス
/// 
/// # Panics
/// 
/// フレームバッファが初期化されていない場合にパニックします。
/// カーネル起動時に `init_framebuffer` を呼び出してください。
pub fn framebuffer() -> &'static Mutex<Framebuffer> {
    FRAMEBUFFER.get().expect(
        "Framebuffer not initialized. Call init_framebuffer() during kernel initialization."
    )
}

/// フレームバッファドライバ
pub struct Framebuffer {
    info: FrameBufferInfo,
    buffer: &'static mut [u8],
    x_pos: usize,
    y_pos: usize,
}

impl Framebuffer {
    pub fn new(info: FrameBufferInfo, buffer: &'static mut [u8]) -> Self {
        let mut fb = Self {
            info,
            buffer,
            x_pos: 0,
            y_pos: 0,
        };
        fb.clear();
        fb
    }

    /// Clears the framebuffer with black color and resets cursor position.
    pub fn clear(&mut self) {
        self.x_pos = 0;
        self.y_pos = 0;
        self.buffer.fill(0);
    }

    /// Returns the framebuffer information.
    pub fn info(&self) -> FrameBufferInfo {
        self.info
    }

    fn newline(&mut self) {
        self.x_pos = 0;
        self.y_pos += 16; // Font height
        
        if self.y_pos + 16 > self.info.height {
            // スクロール処理
            self.scroll_up(16);
            self.y_pos = self.info.height.saturating_sub(16);
        }
    }

    /// 画面を上にスクロール
    fn scroll_up(&mut self, lines: usize) {
        let bytes_per_line = self.info.stride * self.info.bytes_per_pixel * lines;
        let total_bytes = self.info.height * self.info.stride * self.info.bytes_per_pixel;
        
        if bytes_per_line >= total_bytes {
            self.buffer.fill(0);
            return;
        }
        
        unsafe {
            core::ptr::copy(
                self.buffer.as_ptr().add(bytes_per_line),
                self.buffer.as_mut_ptr(),
                total_bytes - bytes_per_line,
            );
        }
        
        self.buffer[total_bytes - bytes_per_line..].fill(0);
    }

    fn write_pixel(&mut self, x: usize, y: usize, color: Color) {
        if x >= self.info.width || y >= self.info.height {
            return;
        }

        let pixel_offset = y * self.info.stride + x;
        let bytes_per_pixel = self.info.bytes_per_pixel;
        let byte_offset = pixel_offset * bytes_per_pixel;
        
        if byte_offset + bytes_per_pixel > self.buffer.len() {
            return;
        }

        let dest = &mut self.buffer[byte_offset..byte_offset + bytes_per_pixel];
        
        match self.info.pixel_format {
            PixelFormat::Rgb => {
                if bytes_per_pixel >= 4 {
                    dest[0] = color.r;
                    dest[1] = color.g;
                    dest[2] = color.b;
                    dest[3] = 255; // Alpha channel
                } else if bytes_per_pixel >= 3 {
                    dest[0] = color.r;
                    dest[1] = color.g;
                    dest[2] = color.b;
                }
            }
            PixelFormat::Bgr => {
                if bytes_per_pixel >= 4 {
                    dest[0] = color.b;
                    dest[1] = color.g;
                    dest[2] = color.r;
                    dest[3] = 255; // Alpha channel
                } else if bytes_per_pixel >= 3 {
                    dest[0] = color.b;
                    dest[1] = color.g;
                    dest[2] = color.r;
                }
            }
            PixelFormat::U8 => {
                if bytes_per_pixel >= 1 {
                    // Simple grayscale conversion
                    dest[0] = ((color.r as u16 + color.g as u16 + color.b as u16) / 3) as u8;
                }
            }
            _ => {
                // Default to BGR for unknown formats on UEFI, as it's most common.
                if bytes_per_pixel >= 4 {
                    dest[0] = color.b;
                    dest[1] = color.g;
                    dest[2] = color.r;
                    dest[3] = 255; // Alpha channel
                } else if bytes_per_pixel >= 3 {
                    dest[0] = color.b;
                    dest[1] = color.g;
                    dest[2] = color.r;
                }
            }
        }
    }

    /// Draws a filled rectangle at the specified position.
    pub fn draw_rect(&mut self, x: usize, y: usize, width: usize, height: usize, color: Color) {
        for i in 0..width {
            for j in 0..height {
                self.write_pixel(x + i, y + j, color);
            }
        }
    }

    /// Draws a line between two points using Bresenham's algorithm.
    pub fn draw_line(&mut self, x1: usize, y1: usize, x2: usize, y2: usize, color: Color) {
        let mut x = x1 as isize;
        let mut y = y1 as isize;
        let dx = (x2 as isize - x1 as isize).abs();
        let dy = -(y2 as isize - y1 as isize).abs();
        let sx = if x1 < x2 { 1 } else { -1 };
        let sy = if y1 < y2 { 1 } else { -1 };
        let mut err = dx + dy;

        loop {
            self.write_pixel(x as usize, y as usize, color);
            if x == x2 as isize && y == y2 as isize { break; }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    /// Draws a circle using the Midpoint circle algorithm.
    pub fn draw_circle(&mut self, center_x: usize, center_y: usize, radius: usize, color: Color) {
        let mut x = radius as isize;
        let mut y = 0;
        let mut err = 0;

        while x >= y {
            self.write_pixel((center_x as isize + x) as usize, (center_y as isize + y) as usize, color);
            self.write_pixel((center_x as isize + y) as usize, (center_y as isize + x) as usize, color);
            self.write_pixel((center_x as isize - y) as usize, (center_y as isize + x) as usize, color);
            self.write_pixel((center_x as isize - x) as usize, (center_y as isize + y) as usize, color);
            self.write_pixel((center_x as isize - x) as usize, (center_y as isize - y) as usize, color);
            self.write_pixel((center_x as isize - y) as usize, (center_y as isize - x) as usize, color);
            self.write_pixel((center_x as isize + y) as usize, (center_y as isize - x) as usize, color);
            self.write_pixel((center_x as isize + x) as usize, (center_y as isize - y) as usize, color);

            if err <= 0 {
                y += 1;
                err += 2 * y + 1;
            }
            if err > 0 {
                x -= 1;
                err -= 2 * x + 1;
            }
        }
    }

    fn write_char(&mut self, c: char) {
        match c {
            '\n' => self.newline(),
            ' '..='~' => {
                if self.x_pos + 8 >= self.info.width {
                    self.newline();
                }
                
                if let Some(bitmap) = get_char_bitmap(c) {
                    for (y, row_byte) in bitmap.iter().enumerate() {
                        for x in 0..8 {
                            if (row_byte >> (7 - x)) & 1 != 0 {
                                self.write_pixel(self.x_pos + x, self.y_pos + y, Color::WHITE);
                            } else {
                                self.write_pixel(self.x_pos + x, self.y_pos + y, Color::BLACK);
                            }
                        }
                    }
                }
                self.x_pos += 8;
            }
            _ => { // Non-printable characters
                if self.x_pos + 8 >= self.info.width {
                    self.newline();
                }
                if let Some(bitmap) = get_char_bitmap('?') {
                    for (y, row_byte) in bitmap.iter().enumerate() {
                        for x in 0..8 {
                            if (row_byte >> (7 - x)) & 1 != 0 {
                                self.write_pixel(self.x_pos + x, self.y_pos + y, Color::WHITE);
                            } else {
                                self.write_pixel(self.x_pos + x, self.y_pos + y, Color::BLACK);
                            }
                        }
                    }
                }
                self.x_pos += 8;
            }
        }
    }
}

impl fmt::Write for Framebuffer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars() {
            self.write_char(c);
        }
        Ok(())
    }
}

impl Device for Framebuffer {
    fn name(&self) -> &'static str {
        "UEFI Framebuffer"
    }
    
    fn init(&mut self) -> KernelResult<()> {
        self.clear();
        Ok(())
    }
    
    fn reset(&mut self) -> KernelResult<()> {
        self.clear();
        Ok(())
    }
}

impl CharDevice for Framebuffer {
    fn read_byte(&self) -> KernelResult<Option<u8>> {
        // フレームバッファは書き込み専用
        Err(DeviceError::NotFound.into())
    }
    
    fn write_byte(&mut self, byte: u8) -> KernelResult<()> {
        self.write_char(byte as char);
        Ok(())
    }
}

/// Returns the 16-byte bitmap for a given printable ASCII character.
fn get_char_bitmap(c: char) -> Option<&'static [u8; 16]> {
    const FONT_CHAR_HEIGHT: usize = 16;
    
    let c_u8 = c as u8;
    
    // The font file starts at ASCII 32 (Space) and ends at ASCII 126 (~).
    if !(32..=126).contains(&c_u8) {
        return None;
    }
    
    let char_index = (c_u8 - 32) as usize;
    let start = char_index * FONT_CHAR_HEIGHT;
    let end = start + FONT_CHAR_HEIGHT;
    
    if end <= FONT.len() {
        let slice = &FONT[start..end];
        // Safety: スライスのサイズは常にFONT_CHAR_HEIGHT (16)バイトであり、
        // これは[u8; FONT_CHAR_HEIGHT]に正確に変換可能
        Some(slice.try_into().expect(
            "Font slice size mismatch. This is a bug in font data."
        ))
    } else {
        None
    }
}
