// src/vga_buffer.rs

//! VGA text mode driver with interrupt-safe Mutex protection
//!
//! This module provides safe VGA text buffer access with the following features:
//! - 16-color support (VGA standard palette)
//! - Auto-scrolling when screen is full
//! - Interrupt-safe locking (prevents deadlock in interrupt handlers)
//! - fmt::Write trait implementation for print!/println! macros
//! - Optimized scrolling with validated memory operations
//! - Boundary checking and buffer validation
//!
//! # Safety and Robustness
//!
//! All buffer accesses are validated to prevent:
//! - Buffer overflows
//! - Out-of-bounds writes
//! - Invalid memory access
//! - Race conditions via Mutex protection
//! - Deadlocks via interrupt-disabled critical sections

use core::fmt::{self, Write};
use core::sync::atomic::{AtomicBool, Ordering};
use spin::Mutex;
use x86_64::instructions::interrupts;

/// VGA text buffer physical memory address
const VGA_BUFFER_ADDR: usize = 0xb8000;

/// Screen dimensions
const VGA_WIDTH: usize = 80;
const VGA_HEIGHT: usize = 25;

/// Bytes per character (1 byte ASCII + 1 byte color attribute)
const BYTES_PER_CHAR: usize = 2;

/// Bytes per row
const BYTES_PER_ROW: usize = VGA_WIDTH * BYTES_PER_CHAR;

/// Total buffer size in bytes
const BUFFER_SIZE: usize = VGA_HEIGHT * BYTES_PER_ROW;

/// ASCII character range for printable characters
const PRINTABLE_ASCII_START: u8 = 0x20;
const PRINTABLE_ASCII_END: u8 = 0x7e;

/// Replacement character for non-printable characters (■)
const REPLACEMENT_CHAR: u8 = 0xfe;

/// Buffer accessibility tracking
static BUFFER_ACCESSIBLE: AtomicBool = AtomicBool::new(false);

/// VGA color codes (4-bit color palette)
#[allow(dead_code)]
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VgaColor {
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

/// Color code combining foreground and background colors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorCode(u8);

impl ColorCode {
    /// Create a new color code from foreground and background colors
    pub const fn new(fg: VgaColor, bg: VgaColor) -> Self {
        Self((bg as u8) << 4 | (fg as u8))
    }

    /// Get the raw byte value
    pub const fn as_u8(self) -> u8 {
        self.0
    }

    /// Default color scheme (light gray on black)
    pub const fn normal() -> Self {
        Self::new(VgaColor::LightGray, VgaColor::Black)
    }

    /// Info color scheme (light cyan on black)
    pub const fn info() -> Self {
        Self::new(VgaColor::LightCyan, VgaColor::Black)
    }

    /// Success color scheme (light green on black)
    pub const fn success() -> Self {
        Self::new(VgaColor::LightGreen, VgaColor::Black)
    }

    /// Warning color scheme (yellow on black)
    pub const fn warning() -> Self {
        Self::new(VgaColor::Yellow, VgaColor::Black)
    }

    /// Error color scheme (light red on black)
    pub const fn error() -> Self {
        Self::new(VgaColor::LightRed, VgaColor::Black)
    }

    /// Panic color scheme (white on red)
    pub const fn panic() -> Self {
        Self::new(VgaColor::White, VgaColor::Red)
    }
}

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
struct VgaWriter {
    position: Position,
    color_code: ColorCode,
    buffer: *mut u8,
}

// SAFETY: We ensure exclusive access via Mutex and interrupt disabling
unsafe impl Send for VgaWriter {}
unsafe impl Sync for VgaWriter {}

impl VgaWriter {
    /// Create a new VGA writer
    const fn new() -> Self {
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
    fn init_accessibility(&mut self) {
        let accessible = self.test_accessibility();
        BUFFER_ACCESSIBLE.store(accessible, Ordering::Release);
    }

    /// Set text color
    fn set_color(&mut self, color: ColorCode) {
        self.color_code = color;
    }

    /// Clear the entire screen with bounds checking
    fn clear(&mut self) {
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
    fn write_colored(&mut self, s: &str, color: ColorCode) {
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

/// Global VGA writer protected by Mutex
///
/// # Locking Order
///
/// CRITICAL: To prevent deadlocks, follow this locking order:
/// 1. SERIAL_PORTS (in serial.rs)
/// 2. VGA_WRITER (this mutex)
///
/// Always acquire serial lock before VGA lock if both are needed.
static VGA_WRITER: Mutex<VgaWriter> = Mutex::new(VgaWriter::new());

/// Execute a function with the VGA writer, protected from interrupts
///
/// This helper ensures that interrupt handlers cannot cause deadlocks
/// when trying to access the VGA writer.
///
/// # Deadlock Prevention
///
/// Using `without_interrupts` ensures:
/// - No interrupt can try to acquire VGA_WRITER while we hold it
/// - No nested lock attempts from the same execution context
/// - Safe concurrent access from multiple code paths
fn with_writer<F, R>(f: F) -> R
where
    F: FnOnce(&mut VgaWriter) -> R,
{
    interrupts::without_interrupts(|| f(&mut VGA_WRITER.lock()))
}

/// Global print! macro
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ({
        $crate::vga_buffer::_print(format_args!($($arg)*))
    });
}

/// Global println! macro
#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($fmt:expr) => ($crate::print!(concat!($fmt, "\n")));
    ($fmt:expr, $($arg:tt)*) => ($crate::print!(concat!($fmt, "\n"), $($arg)*));
}

/// Print function called by macros
#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    with_writer(|writer| {
        let _ = writer.write_fmt(args);
    });
}

/// Initialize VGA buffer and test accessibility
///
/// Should be called once during kernel initialization.
/// Tests buffer accessibility and caches the result.
pub fn init() {
    with_writer(|writer| {
        writer.init_accessibility();
    });
}

/// Check if VGA buffer is accessible
pub fn is_accessible() -> bool {
    BUFFER_ACCESSIBLE.load(Ordering::Acquire)
}

/// Clear the screen
pub fn clear() {
    with_writer(|writer| {
        writer.clear();
    });
}

/// Set the text color
pub fn set_color(color: ColorCode) {
    with_writer(|writer| writer.set_color(color));
}

/// Print colored text
pub fn print_colored(s: &str, color: ColorCode) {
    with_writer(|writer| writer.write_colored(s, color));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_code_encoding() {
        let color = ColorCode::new(VgaColor::White, VgaColor::Red);
        assert_eq!(color.as_u8(), 0x4F);
    }

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
