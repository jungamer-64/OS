// src/framebuffer/writer.rs

//! Framebuffer writer for formatted text output

use super::font::{FONT_HEIGHT, FONT_WIDTH, draw_char};
use super::{FramebufferError, FramebufferInfo, RgbColor, colorcode_to_rgb};
use crate::vga_buffer::ColorCode;
use core::fmt;

/// Framebuffer writer for text rendering
pub struct FramebufferWriter {
    /// Current column position (in characters)
    column: usize,
    /// Current row position (in characters)
    row: usize,
    /// Maximum columns
    max_columns: usize,
    /// Maximum rows
    max_rows: usize,
    /// Current foreground color
    fg_color: RgbColor,
    /// Current background color
    bg_color: RgbColor,
    // Note: Framebuffer info will be passed as parameter in methods
}

impl FramebufferWriter {
    /// Create a new framebuffer writer
    #[must_use]
    pub fn new(width: usize, height: usize) -> Self {
        let max_columns = width / FONT_WIDTH;
        let max_rows = height / FONT_HEIGHT;

        Self {
            column: 0,
            row: 0,
            max_columns,
            max_rows,
            fg_color: RgbColor::white(),
            bg_color: RgbColor::black(),
        }
    }

    /// Set the text color from a ColorCode
    pub fn set_color(&mut self, color: ColorCode) {
        let code = color.as_u8();
        let fg = (code & 0x0F) as u8;
        let bg = ((code >> 4) & 0x0F) as u8;
        
        self.fg_color = colorcode_to_rgb(fg);
        self.bg_color = colorcode_to_rgb(bg);
    }

    /// Write a single byte to the framebuffer
    ///
    /// # Errors
    ///
    /// Returns error if framebuffer operations fail
    pub fn write_byte(&mut self, fb: &mut FramebufferInfo, byte: u8) -> Result<(), FramebufferError> {
        match byte {
            b'\n' => self.new_line(fb)?,
            byte => {
                if self.column >= self.max_columns {
                    self.new_line(fb)?;
                }

                let x = self.column * FONT_WIDTH;
                let y = self.row * FONT_HEIGHT;

                draw_char(fb, x, y, byte as char, self.fg_color, self.bg_color)?;
                self.column += 1;
            }
        }
        Ok(())
    }

    /// Write a string to the framebuffer
    ///
    /// # Errors
    ///
    /// Returns error if framebuffer operations fail
    pub fn write_string(&mut self, fb: &mut FramebufferInfo, s: &str) -> Result<(), FramebufferError> {
        for byte in s.bytes() {
            match byte {
                // Printable ASCII or newline
                0x20..=0x7e | b'\n' => self.write_byte(fb, byte)?,
                // Not part of printable ASCII range
                _ => self.write_byte(fb, 0xfe)?, // ■ character
            }
        }
        Ok(())
    }

    /// Write colored string to the framebuffer
    ///
    /// # Errors
    ///
    /// Returns error if framebuffer operations fail  
    pub fn write_colored(
        &mut self,
        fb: &mut FramebufferInfo,
        s: &str,
        color: ColorCode,
    ) -> Result<(), FramebufferError> {
        // Store current colors for restoration
        let old_fg = self.fg_color;
        let old_bg = self.bg_color;
        
        self.set_color(color);
        self.write_string(fb, s)?;
        
        // Restore old colors
        self.fg_color = old_fg;
        self.bg_color = old_bg;
        
        Ok(())
    }

    /// Move to a new line, scrolling if necessary
    fn new_line(&mut self, fb: &mut FramebufferInfo) -> Result<(), FramebufferError> {
        self.column = 0;
        
        if self.row >= self.max_rows - 1 {
            // Need to scroll
            fb.scroll_up(FONT_HEIGHT, self.bg_color)?;
        } else {
            self.row += 1;
        }
        
        Ok(())
    }

    /// Clear the screen
    ///
    /// # Errors
    ///
    /// Returns error if clear operation fails
    pub fn clear(&mut self, fb: &mut FramebufferInfo) -> Result<(), FramebufferError> {
        fb.clear(self.bg_color)?;
        self.column = 0;
        self.row = 0;
        Ok(())
    }

    /// Get current cursor position (for testing/debugging)
    #[must_use]
    pub const fn position(&self) -> (usize, usize) {
        (self.column, self.row)
    }
}

/// Best-effort conversion from RGB to VGA color code
///
/// This is a simple approximation for color restoration
#[must_use]
fn rgb_to_vga_code(color: RgbColor) -> u8 {
    // Simple threshold-based conversion
    let r = if color.r > 128 { 1 } else { 0 };
    let g = if color.g > 128 { 1 } else { 0 };
    let b = if color.b > 128 { 1 } else { 0 };
    
    // Intensity bit
    let intensity = if color.r > 200 || color.g > 200 || color.b > 200 { 8 } else { 0 };
    
    r * 4 + g * 2 + b + intensity
}

/// Wrapper for implementing fmt::Write
pub struct FramebufferWriteAdapter<'a> {
    writer: &'a mut FramebufferWriter,
    fb: &'a mut FramebufferInfo,
}

impl<'a> FramebufferWriteAdapter<'a> {
    /// Create a new adapter
    #[must_use]
    pub fn new(writer: &'a mut FramebufferWriter, fb: &'a mut FramebufferInfo) -> Self {
        Self { writer, fb }
    }
}

impl fmt::Write for FramebufferWriteAdapter<'_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.writer.write_string(self.fb, s).map_err(|_| fmt::Error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn test_writer_creation() {
        let writer = FramebufferWriter::new(640, 480);
        assert_eq!(writer.position(), (0, 0));
        assert_eq!(writer.max_columns, 640 / FONT_WIDTH);
        assert_eq!(writer.max_rows, 480 / FONT_HEIGHT);
    }

    #[test_case]
    fn test_color_conversion() {
        let white = colorcode_to_rgb(15);
        assert_eq!(white, RgbColor::white());

        let black = colorcode_to_rgb(0);
        assert_eq!(black, RgbColor::black());
    }
}
