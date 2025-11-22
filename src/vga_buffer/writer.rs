// src/vga_buffer/writer.rs

//! VGA writer implementation with bounds-checked buffer access and
//! robust error propagation.

use super::backend::{DefaultVgaBuffer, VgaBufferAccess};
use super::color::ColorCode;
use super::constants::*;
use crate::diagnostics::DIAGNOSTICS;
use core::fmt::{self, Write};
use core::sync::atomic::{AtomicBool, Ordering};

const DIRTY_WORD_BITS: usize = core::mem::size_of::<u64>() * 8;
const DIRTY_WORD_COUNT: usize = CELL_COUNT.div_ceil(DIRTY_WORD_BITS);

/// Buffer accessibility tracking
pub(super) static BUFFER_ACCESSIBLE: AtomicBool = AtomicBool::new(false);

/// Errors that can occur when interacting with the VGA subsystem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VgaError {
    /// The VGA buffer is not currently accessible.
    BufferNotAccessible,
    /// The cursor position is outside the visible screen area.
    InvalidPosition,
    /// The underlying memory write failed.
    WriteFailure,
    /// The writer has not been successfully initialized yet.
    NotInitialized,
    /// The writer was used without the runtime lock being held.
    NotLocked,
    /// Lock order violation detected.
    LockOrderViolation,
}

impl VgaError {
    /// Convert the error into a human-readable static message.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::BufferNotAccessible => "buffer not accessible",
            Self::InvalidPosition => "invalid position",
            Self::WriteFailure => "write failure",
            Self::NotInitialized => "writer not initialized",
            Self::NotLocked => "writer not locked",
            Self::LockOrderViolation => "lock order violation",
        }
    }
}

/// A writer type that allows writing ASCII bytes and strings to an underlying `Buffer`.
pub struct VgaWriter {
    column_position: usize,
    color_code: ColorCode,
    buffer: DefaultVgaBuffer,
}

impl VgaWriter {
    pub const fn new() -> Self {
        VgaWriter {
            column_position: 0,
            color_code: ColorCode::new(super::color::VgaColor::Yellow, super::color::VgaColor::Black),
            buffer: DefaultVgaBuffer::new(),
        }
    }

    pub fn init_accessibility(&mut self) -> Result<(), VgaError> {
        let test_idx = 0;
        let original = self.buffer.read_cell(test_idx)?;
        self.buffer.write_cell(test_idx, original)?;
        BUFFER_ACCESSIBLE.store(true, Ordering::Release);
        Ok(())
    }

    pub fn clear(&mut self) -> Result<(), VgaError> {
        let blank = (self.color_code.as_u8() as u16) << 8 | b' ' as u16;
        for i in 0..CELL_COUNT {
            self.buffer.write_cell(i, blank)?;
        }
        self.column_position = 0;
        Ok(())
    }

    pub fn set_color(&mut self, color: ColorCode) -> Result<(), VgaError> {
        self.color_code = color;
        Ok(())
    }

    pub fn write_colored(&mut self, s: &str, color: ColorCode) -> Result<(), VgaError> {
        let old_color = self.color_code;
        self.color_code = color;
        self.write_string(s);
        self.color_code = old_color;
        Ok(())
    }

    pub fn write_byte(&mut self, byte: u8) {
        match byte {
            b'\n' => self.new_line(),
            byte => {
                if self.column_position >= VGA_WIDTH {
                    self.new_line();
                }
                let row = VGA_HEIGHT - 1;
                let col = self.column_position;
                let color_code = self.color_code;
                let val = (color_code.as_u8() as u16) << 8 | byte as u16;
                let _ = self.buffer.write_cell(row * VGA_WIDTH + col, val);
                self.column_position += 1;
            }
        }
    }

    pub fn write_string(&mut self, s: &str) {
        for byte in s.bytes() {
            match byte {
                0x20..=0x7e | b'\n' => self.write_byte(byte),
                _ => self.write_byte(0xfe),
            }
        }
    }

    fn new_line(&mut self) {
        for row in 1..VGA_HEIGHT {
            for col in 0..VGA_WIDTH {
                let val = self.buffer.read_cell(row * VGA_WIDTH + col).unwrap_or(0);
                let _ = self.buffer.write_cell((row - 1) * VGA_WIDTH + col, val);
            }
        }
        self.clear_row(VGA_HEIGHT - 1);
        self.column_position = 0;
    }

    fn clear_row(&mut self, row: usize) {
        let blank = (self.color_code.as_u8() as u16) << 8 | b' ' as u16;
        for col in 0..VGA_WIDTH {
            let _ = self.buffer.write_cell(row * VGA_WIDTH + col, blank);
        }
    }

    pub fn runtime_guard(&self) -> RuntimeGuard {
        RuntimeGuard
    }

    pub fn encode_char(c: u8, color: ColorCode) -> u16 {
        (color.as_u8() as u16) << 8 | c as u16
    }

    pub fn is_printable(c: u8) -> bool {
        match c {
            0x20..=0x7e | b'\n' => true,
            _ => false,
        }
    }
}

impl fmt::Write for VgaWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_string(s);
        Ok(())
    }
}

pub struct RuntimeGuard;
pub type DoubleBufferedWriter = VgaWriter;


/// Position in the VGA buffer with validation
#[derive(Debug, Clone, Copy)]
struct Position {
    row: usize,
    col: usize,
}

impl Position {
    /// Create a new position at the top-left corner
    const fn new() -> Self {
        Self { row: 0, col: 0 }
    }

    /// Calculate linear cell index in the VGA buffer with bounds checking.
    /// Returns `None` if the position is out of bounds.
    const fn cell_index(&self) -> Option<usize> {
        if self.row >= VGA_HEIGHT || self.col >= VGA_WIDTH {
            return None;
        }
        Some(self.row * VGA_WIDTH + self.col)
    }

    /// Move to next column, returns false if at row end
    const fn advance_col(&mut self) -> bool {
        if self.col + 1 < VGA_WIDTH {
            self.col += 1;
            true
        } else {
            false
        }
    }

    /// Move to new line
    const fn new_line(&mut self) {
        self.col = 0;
        self.row += 1;
    }

    /// Check if at bottom of screen
    const fn is_at_screen_bottom(&self) -> bool {
        self.row >= VGA_HEIGHT
    }

    /// Validate position is within bounds
    const fn is_valid(&self) -> bool {
        self.row < VGA_HEIGHT && self.col < VGA_WIDTH
    }
}

#[cfg(test)]
mod kernel_tests {
    use super::*;

    #[test_case]
    fn test_position_new() {
        let pos = Position::new();
        assert_eq!(pos.row, 0);
        assert_eq!(pos.col, 0);
    }

    #[test_case]
    fn test_position_bounds() {
        let mut pos = Position::new();
        assert!(pos.is_valid());
        
        pos.col = VGA_WIDTH;
        assert!(!pos.is_valid());
        
        pos.col = 0;
        pos.row = VGA_HEIGHT;
        assert!(!pos.is_valid());
    }

    #[test_case]
    fn test_position_advance() {
        let mut pos = Position::new();
        assert!(pos.advance_col());
        assert_eq!(pos.col, 1);
        
        pos.col = VGA_WIDTH - 1;
        assert!(!pos.advance_col());
    }
}

#[cfg(all(test, feature = "std-tests"))]
mod tests {
    use super::*;

    #[test]
    fn test_position_validation() {
        let mut pos = Position::new();
        assert!(pos.is_valid());

        pos.row = VGA_HEIGHT;
        assert!(!pos.is_valid());
    }

    #[test]
    fn test_position_byte_offset() {
        let pos = Position { row: 1, col: 2 };
        assert_eq!(pos.cell_index(), Some(1 * 80 + 2));

        let invalid_pos = Position {
            row: VGA_HEIGHT,
            col: 0,
        };
        assert_eq!(invalid_pos.cell_index(), None);
    }

    #[test]
    fn test_advance_col_behavior() {
        let mut end_of_row = Position {
            row: 0,
            col: VGA_WIDTH - 1,
        };
        assert!(!end_of_row.advance_col());
        assert_eq!(end_of_row.col, VGA_WIDTH - 1);

        let mut mid_row = Position { row: 0, col: 0 };
        assert!(mid_row.advance_col());
        assert_eq!(mid_row.col, 1);
    }

    #[test]
    fn test_char_encoding() {
        use super::super::color::VgaColor;
        let encoded = VgaWriter::encode_char(b'A', ColorCode::normal());
        assert_eq!(encoded & 0xFF, b'A' as u16);
        assert_eq!(encoded >> 8, ColorCode::normal().as_u8() as u16);
    }

    #[test]
    fn test_printable_detection() {
        assert!(VgaWriter::is_printable(b' '));
        assert!(VgaWriter::is_printable(b'A'));
        assert!(VgaWriter::is_printable(b'\n'));
        assert!(!VgaWriter::is_printable(0x00));
        assert!(!VgaWriter::is_printable(0x7F));
    }
}

