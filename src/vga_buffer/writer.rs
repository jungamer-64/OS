// src/vga_buffer/writer.rs

//! VGA writer implementation with bounds-checked buffer access

use super::color::ColorCode;
use super::constants::*;
use crate::diagnostics::DIAGNOSTICS;
use core::fmt::{self, Write};
use core::sync::atomic::{AtomicBool, Ordering};

/// Total number of character cells in the VGA buffer.
const CELL_COUNT: usize = VGA_WIDTH * VGA_HEIGHT;

/// Buffer accessibility tracking
pub(super) static BUFFER_ACCESSIBLE: AtomicBool = AtomicBool::new(false);

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
    ///
    /// Returns `None` if the position is out of bounds.
    fn cell_index(&self) -> Option<usize> {
        if self.row >= VGA_HEIGHT || self.col >= VGA_WIDTH {
            return None;
        }
        Some(self.row * VGA_WIDTH + self.col)
    }

    /// Move to next column, returns false if at row end
    fn advance_col(&mut self) -> bool {
        self.col += 1;
        self.col < VGA_WIDTH
    }

    /// Move to new line
    fn new_line(&mut self) {
        self.col = 0;
        self.row += 1;
    }

    /// Check if at end of row
    fn is_at_row_end(&self) -> bool {
        self.col >= VGA_WIDTH
    }

    /// Check if at bottom of screen
    fn is_at_screen_bottom(&self) -> bool {
        self.row >= VGA_HEIGHT
    }

    /// Validate position is within bounds
    fn is_valid(&self) -> bool {
        self.row < VGA_HEIGHT && self.col < VGA_WIDTH
    }
}

/// Errors that can occur when writing to the VGA buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum VgaError {
    /// The VGA buffer is not currently accessible.
    BufferNotAccessible,
    /// The cursor position is outside the visible screen area.
    InvalidPosition,
    /// The underlying memory write failed.
    WriteFailure,
}

/// Thin wrapper around the VGA text buffer that centralizes all raw pointer
/// interaction. Exposes safe helpers that validate indices before touching
/// memory so higher-level code can remain mostly safe.
#[derive(Clone, Copy)]
struct ScreenBuffer {
    ptr: *mut u16,
}

impl ScreenBuffer {
    /// Create a new screen buffer pointing at the well-known VGA text address.
    const fn new() -> Self {
        Self {
            ptr: VGA_BUFFER_ADDR as *mut u16,
        }
    }

    /// Number of accessible cells.
    #[inline(always)]
    fn len(&self) -> usize {
        CELL_COUNT
    }

    /// Write a value to a cell if it is within bounds.
    #[inline(always)]
    fn write(&self, index: usize, value: u16) -> bool {
        debug_assert!(index < self.len(), "VGA cell index {} out of bounds", index);
        if index >= self.len() {
            return false;
        }

        unsafe {
            // SAFETY: `index` is validated against the buffer length above and the
            // pointer is fixed to the VGA text buffer address. The Mutex guarding
            // this writer ensures exclusive access to the buffer memory.
            core::ptr::write_volatile(self.ptr.add(index), value);

            // Ensure the write is observed before subsequent operations.
            core::sync::atomic::compiler_fence(Ordering::SeqCst);
        }

        true
    }

    /// Read a value from a cell if it is within bounds.
    #[inline(always)]
    fn read(&self, index: usize) -> Option<u16> {
        if index >= self.len() {
            return None;
        }

        Some(unsafe {
            // SAFETY: `index` is in range as checked above, and the pointer targets
            // the VGA text memory. Volatile read is permitted for hardware memory.
            core::ptr::read_volatile(self.ptr.add(index))
        })
    }

    /// Copy a range of cells within the buffer with bounds validation.
    #[inline(always)]
    fn copy(&self, src: usize, dst: usize, count: usize) -> bool {
        if count == 0 {
            return true;
        }

        let len = self.len();
        if src >= len || dst >= len {
            return false;
        }

        let src_end = match src.checked_add(count) {
            Some(end) if end <= len => end,
            _ => return false,
        };

        let dst_end = match dst.checked_add(count) {
            Some(end) if end <= len => end,
            _ => return false,
        };

        debug_assert!(src_end <= len);
        debug_assert!(dst_end <= len);

        unsafe {
            // SAFETY: Source and destination ranges are within the same VGA buffer
            // and validated not to exceed the buffer length. `ptr::copy` is safe for
            // overlapping regions, which matches VGA scroll semantics.
            core::ptr::copy(self.ptr.add(src), self.ptr.add(dst), count);
        }

        true
    }

    /// Fill a row with the provided encoded character value.
    fn fill_row(&self, row: usize, encoded: u16) {
        if row >= VGA_HEIGHT {
            return;
        }

        let start = row * VGA_WIDTH;
        let end = start + VGA_WIDTH;
        for idx in start..end {
            if !self.write(idx, encoded) {
                break;
            }
        }
    }
}

/// VGA Writer structure with bounds-checked buffer access
pub struct VgaWriter {
    position: Position,
    color_code: ColorCode,
    buffer: ScreenBuffer,
}

// SAFETY: We ensure exclusive access via Mutex and interrupt disabling
unsafe impl Send for VgaWriter {}
unsafe impl Sync for VgaWriter {}

impl VgaWriter {
    /// Create a new VGA writer
    pub const fn new() -> Self {
        Self {
            position: Position::new(),
            color_code: ColorCode::normal(),
            buffer: ScreenBuffer::new(),
        }
    }

    /// Test if VGA buffer is accessible
    ///
    /// Attempts multiple validation tests to verify buffer accessibility:
    /// 1. Read test from first cell
    /// 2. Write/read test to verify write capability
    /// 3. Restoration of original value
    fn test_accessibility(&self) -> bool {
        let Some(original) = self.buffer.read(0) else {
            return false;
        };

        let test_pattern: u16 = 0x0F20; // White space
        if !self.buffer.write(0, test_pattern) {
            return false;
        }

        let readback = self.buffer.read(0);

        // Restore original value regardless of the outcome.
        let _ = self.buffer.write(0, original);

        matches!(readback, Some(value) if value == test_pattern)
    }

    /// Verify buffer is accessible (cached result)
    fn is_accessible(&self) -> bool {
        BUFFER_ACCESSIBLE.load(Ordering::Acquire)
    }

    /// Initialize and test buffer accessibility
    pub fn init_accessibility(&mut self) {
        let accessible = self.test_accessibility();
        BUFFER_ACCESSIBLE.store(accessible, Ordering::Release);
    }

    /// Set text color
    pub fn set_color(&mut self, color: ColorCode) {
        self.color_code = color;
    }

    /// Clear the entire screen with bounds checking
    pub fn clear(&mut self) {
        if !self.is_accessible() {
            return;
        }

        for row in 0..VGA_HEIGHT {
            self.clear_row(row);
        }

        self.position = Position::new();
        self.color_code = ColorCode::normal();
    }

    /// Clear a specific row with blank characters
    fn clear_row(&mut self, row: usize) {
        // Bounds check
        if row >= VGA_HEIGHT {
            return;
        }

        let blank = Self::encode_char(b' ', self.color_code);

        self.buffer.fill_row(row, blank);
    }

    /// Encode a character with color into a 16-bit value
    const fn encode_char(ch: u8, color: ColorCode) -> u16 {
        (color.as_u8() as u16) << 8 | ch as u16
    }

    /// Write an encoded character at the current position
    #[allow(dead_code)]
    fn write_encoded_char(&mut self, encoded: u16) {
        if let Some(index) = self.position.cell_index() {
            let _ = self.buffer.write(index, encoded);
        }
    }

    /// Scroll the screen up by one line with validated memory operations
    ///
    /// # Safety
    ///
    /// Uses `ptr::copy` which is safe for overlapping regions.
    /// All offsets are validated before copying.
    fn scroll(&mut self) {
        if !self.is_accessible() {
            return;
        }

        // Record scroll operation for diagnostics
        DIAGNOSTICS.record_vga_scroll();

        // Validate buffer bounds before copying
        let src_index = VGA_WIDTH;
        let dst_index = 0;
        let copy_cells = VGA_WIDTH * (VGA_HEIGHT - 1);

        if !self.buffer.copy(src_index, dst_index, copy_cells) {
            return;
        }

        // Clear the last row
        self.clear_row(VGA_HEIGHT - 1);

        // Update position
        self.position.row = VGA_HEIGHT - 1;
        self.position.col = 0;
    }

    /// Move to a new line, scrolling if necessary
    fn new_line(&mut self) {
        self.position.new_line();

        if self.position.is_at_screen_bottom() {
            self.scroll();
        }
    }

    /// Write a single byte to the screen, ignoring any failure.
    fn write_byte(&mut self, byte: u8) {
        let _ = self.write_byte_internal(byte);
    }

    /// Write a single byte with detailed error reporting.
    #[allow(dead_code)]
    pub fn write_byte_checked(&mut self, byte: u8) -> Result<(), VgaError> {
        self.write_byte_internal(byte)
    }

    fn write_byte_internal(&mut self, byte: u8) -> Result<(), VgaError> {
        if !self.is_accessible() {
            DIAGNOSTICS.record_vga_write(false);
            return Err(VgaError::BufferNotAccessible);
        }

        match byte {
            b'\n' => {
                self.new_line();
                DIAGNOSTICS.record_vga_write(true);
                Ok(())
            }
            _ => {
                if self.position.is_at_row_end() {
                    self.new_line();
                }

                if !self.position.is_valid() {
                    DIAGNOSTICS.record_vga_write(false);
                    return Err(VgaError::InvalidPosition);
                }

                let Some(index) = self.position.cell_index() else {
                    DIAGNOSTICS.record_vga_write(false);
                    return Err(VgaError::InvalidPosition);
                };

                let encoded = Self::encode_char(byte, self.color_code);
                if self.buffer.write(index, encoded) {
                    let _ = self.position.advance_col();
                    DIAGNOSTICS.record_vga_write(true);
                    Ok(())
                } else {
                    DIAGNOSTICS.record_vga_write(false);
                    Err(VgaError::WriteFailure)
                }
            }
        }
    }

    /// Write a string with temporary color
    pub fn write_colored(&mut self, s: &str, color: ColorCode) {
        let old_color = self.color_code;
        self.set_color(color);
        let _ = self.write_str(s);
        self.set_color(old_color);
    }

    /// Check if a byte is printable ASCII or newline
    fn is_printable(byte: u8) -> bool {
        (PRINTABLE_ASCII_START..=PRINTABLE_ASCII_END).contains(&byte) || byte == b'\n'
    }
}

impl Write for VgaWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        if !self.is_accessible() {
            return Ok(());
        }

        for byte in s.bytes() {
            let display_byte = if Self::is_printable(byte) {
                byte
            } else {
                REPLACEMENT_CHAR
            };
            self.write_byte(display_byte);
        }
        Ok(())
    }
}

/// Double-buffered VGA writer to minimize display tearing.
#[allow(dead_code)]
pub struct DoubleBufferedWriter {
    front: ScreenBuffer,
    back: [u16; CELL_COUNT],
    dirty: [bool; CELL_COUNT],
}

impl DoubleBufferedWriter {
    /// Create a new double-buffered writer with a clean back buffer.
    #[allow(dead_code)]
    pub const fn new() -> Self {
        Self {
            front: ScreenBuffer::new(),
            back: [0; CELL_COUNT],
            dirty: [false; CELL_COUNT],
        }
    }

    /// Stage a cell write in the back buffer.
    #[allow(dead_code)]
    pub fn write_cell(&mut self, index: usize, value: u16) -> bool {
        if index >= CELL_COUNT {
            return false;
        }

        self.back[index] = value;
        self.dirty[index] = true;
        true
    }

    /// Flush dirty cells to the front buffer.
    #[allow(dead_code)]
    pub fn flush(&mut self) {
        for i in 0..CELL_COUNT {
            if self.dirty[i] {
                let _ = self.front.write(i, self.back[i]);
                self.dirty[i] = false;
            }
        }
    }
}

#[cfg(test)]
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
