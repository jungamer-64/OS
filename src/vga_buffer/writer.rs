// src/vga_buffer/writer.rs

//! VGA writer implementation with bounds-checked buffer access

use super::color::ColorCode;
use super::constants::*;
use core::fmt::{self, Write};
use core::sync::atomic::{AtomicBool, Ordering};

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

    /// Calculate byte offset in VGA buffer with bounds checking
    ///
    /// Returns `None` if the position is out of bounds.
    fn byte_offset(&self) -> Option<usize> {
        if self.row >= VGA_HEIGHT || self.col >= VGA_WIDTH {
            return None;
        }
        Some((self.row * VGA_WIDTH + self.col) * BYTES_PER_CHAR)
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

/// VGA Writer structure with bounds-checked buffer access
pub struct VgaWriter {
    position: Position,
    color_code: ColorCode,
    buffer: *mut u8,
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
            buffer: VGA_BUFFER_ADDR as *mut u8,
        }
    }

    /// Test if VGA buffer is accessible
    ///
    /// Attempts multiple validation tests to verify buffer accessibility:
    /// 1. Read test from first cell
    /// 2. Write/read test to verify write capability
    /// 3. Restoration of original value
    fn test_accessibility(&self) -> bool {
        unsafe {
            // Test 1: Try reading first character cell
            let original = core::ptr::read_volatile(self.buffer as *const u16);

            // Test 2: Write test pattern and read back
            let test_pattern: u16 = 0x0F20; // White space
            core::ptr::write_volatile(self.buffer as *mut u16, test_pattern);
            let readback = core::ptr::read_volatile(self.buffer as *const u16);

            // Test 3: Restore original value
            core::ptr::write_volatile(self.buffer as *mut u16, original);

            // Verify write/read worked
            readback == test_pattern
        }
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

        for col in 0..VGA_WIDTH {
            let offset = (row * VGA_WIDTH + col) * BYTES_PER_CHAR;
            // Additional bounds check
            if offset + 1 < BUFFER_SIZE {
                self.write_encoded_char_at_offset(offset, blank);
            }
        }
    }

    /// Encode a character with color into a 16-bit value
    const fn encode_char(ch: u8, color: ColorCode) -> u16 {
        (color.as_u8() as u16) << 8 | ch as u16
    }

    /// Write an encoded character to the buffer at a specific offset
    ///
    /// # Safety
    ///
    /// Caller must ensure offset is within buffer bounds.
    /// This is a private method only called after bounds validation.
    fn write_encoded_char_at_offset(&mut self, offset: usize, encoded: u16) {
        // Debug assertion for development builds
        debug_assert!(
            offset + 1 < BUFFER_SIZE,
            "VGA buffer write out of bounds: offset={}, size={}",
            offset,
            BUFFER_SIZE
        );

        if offset + 1 >= BUFFER_SIZE {
            // Bounds check failed - this should never happen
            // but we protect against it in release builds
            return;
        }

        unsafe {
            core::ptr::write_volatile(self.buffer.add(offset) as *mut u16, encoded);
        }
    }

    /// Write an encoded character at the current position
    fn write_encoded_char(&mut self, encoded: u16) {
        if let Some(offset) = self.position.byte_offset() {
            self.write_encoded_char_at_offset(offset, encoded);
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

        // Validate buffer bounds before copying
        let src_offset = BYTES_PER_ROW;
        let dst_offset = 0;
        let copy_size = BYTES_PER_ROW * (VGA_HEIGHT - 1);

        // Bounds check
        if src_offset + copy_size > BUFFER_SIZE {
            return;
        }

        unsafe {
            // SAFETY:
            // - src and dst are within the same valid buffer
            // - copy_size is validated to fit within buffer
            // - ptr::copy handles overlapping memory correctly
            core::ptr::copy(
                self.buffer.add(src_offset),
                self.buffer.add(dst_offset),
                copy_size,
            );
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

    /// Write a single byte to the screen
    fn write_byte(&mut self, byte: u8) {
        if !self.is_accessible() {
            return;
        }

        match byte {
            b'\n' => self.new_line(),
            _ => {
                if self.position.is_at_row_end() {
                    self.new_line();
                }

                // Validate position before writing
                if !self.position.is_valid() {
                    return;
                }

                let encoded = Self::encode_char(byte, self.color_code);
                self.write_encoded_char(encoded);

                self.position.advance_col();
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
        assert_eq!(pos.byte_offset(), Some((1 * 80 + 2) * 2));

        let invalid_pos = Position {
            row: VGA_HEIGHT,
            col: 0,
        };
        assert_eq!(invalid_pos.byte_offset(), None);
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
