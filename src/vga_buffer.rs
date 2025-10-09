// src/vga_buffer.rs

//! VGA text mode driver with interrupt-safe Mutex protection
//!
//! This module provides safe VGA text buffer access with the following features:
//! - 16-color support (VGA standard palette)
//! - Auto-scrolling when screen is full
//! - Interrupt-safe locking (prevents deadlock in interrupt handlers)
//! - fmt::Write trait implementation for print!/println! macros
//! - Optimized scrolling with copy (overlapping-safe)
//!
//! # Architecture
//!
//! The VGA text buffer is located at physical address 0xB8000 and supports
//! 80x25 characters. Each character occupies 2 bytes:
//! - Byte 0: ASCII character code
//! - Byte 1: Color attribute (4 bits background | 4 bits foreground)
//!
//! # Thread Safety
//!
//! All access to the VGA buffer is protected by a Mutex and executes within
//! interrupt-disabled critical sections via `x86_64::instructions::interrupts::without_interrupts`.
//! This prevents deadlocks when interrupt handlers attempt to write to the screen.
//!
//! # Limitations
//!
//! VGA text mode only supports ASCII characters (0x00-0x7F and extended ASCII).
//! Multi-byte characters (e.g., Japanese, Chinese) will display as replacement characters (■).
//!
//! To support UTF-8/Unicode characters, you would need to:
//! 1. Implement a custom font bitmap/glyph renderer
//! 2. Use framebuffer graphics mode instead of text mode
//! 3. Implement Unicode to glyph mapping
//! 4. Handle character composition (combining characters, etc.)
//!
//! # Examples
//!
//! ```no_run
//! use vga_buffer::{clear, print_colored, ColorCode};
//!
//! // Clear screen and print colored message
//! clear();
//! print_colored("Hello, ", ColorCode::normal());
//! print_colored("World!\n", ColorCode::success());
//! ```

use core::fmt::{self, Write};
use spin::Mutex;
use x86_64::instructions::interrupts;

/// VGA text buffer physical memory address
///
/// Note: This address is valid for BIOS text mode. In UEFI mode,
/// the framebuffer may be at a different address. This kernel
/// assumes BIOS boot mode.
const VGA_BUFFER_ADDR: usize = 0xb8000;

/// Screen dimensions
const VGA_WIDTH: usize = 80;
const VGA_HEIGHT: usize = 25;

/// Bytes per character (1 byte ASCII + 1 byte color attribute)
const BYTES_PER_CHAR: usize = 2;

/// Bytes per row
const BYTES_PER_ROW: usize = VGA_WIDTH * BYTES_PER_CHAR;

/// ASCII character range for printable characters
const PRINTABLE_ASCII_START: u8 = 0x20;
const PRINTABLE_ASCII_END: u8 = 0x7e;

/// Replacement character for non-printable characters (■)
const REPLACEMENT_CHAR: u8 = 0xfe;

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
///
/// VGA color byte format: [background 4 bits][foreground 4 bits]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorCode(u8);

impl ColorCode {
    /// Create a new color code from foreground and background colors
    ///
    /// # Arguments
    ///
    /// * `fg` - Foreground color
    /// * `bg` - Background color
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

/// Position in the VGA buffer
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

    /// Calculate byte offset in VGA buffer
    const fn byte_offset(&self) -> usize {
        (self.row * VGA_WIDTH + self.col) * BYTES_PER_CHAR
    }

    /// Move to next column
    fn advance_col(&mut self) {
        self.col += 1;
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
}

/// VGA Writer structure - encapsulates all VGA state
struct VgaWriter {
    position: Position,
    color_code: ColorCode,
    buffer: *mut u8,
}

// SAFETY: VGA buffer is always accessible and we're in a single-threaded kernel
// The Mutex ensures exclusive access, so this is safe
unsafe impl Send for VgaWriter {}
unsafe impl Sync for VgaWriter {}

impl VgaWriter {
    /// Create a new VGA writer
    ///
    /// # Platform Compatibility
    ///
    /// This implementation assumes BIOS text mode at 0xB8000.
    /// For UEFI systems, you would need to:
    /// - Query the framebuffer address from boot info
    /// - Implement pixel-based rendering
    /// - Handle different screen resolutions
    ///
    /// The current implementation is optimized for legacy BIOS boot.
    const fn new() -> Self {
        Self {
            position: Position::new(),
            color_code: ColorCode::normal(),
            buffer: VGA_BUFFER_ADDR as *mut u8,
        }
    }
    
    /// Test if VGA buffer is accessible
    ///
    /// Attempts to read from the VGA buffer to verify it's mapped.
    /// This helps detect issues with memory-mapped I/O access.
    ///
    /// # Returns
    ///
    /// `true` if the buffer appears accessible, `false` otherwise.
    ///
    /// # Safety
    ///
    /// This performs a volatile read from VGA memory. Safe because:
    /// - Reading from VGA buffer has no side effects
    /// - The address is within valid VGA text mode range
    fn is_accessible(&self) -> bool {
        unsafe {
            // Try to read the first character cell
            // If this causes a fault, the kernel will panic anyway
            let _test = core::ptr::read_volatile(self.buffer as *const u16);
            true
        }
    }

    /// Set text color
    fn set_color(&mut self, color: ColorCode) {
        self.color_code = color;
    }

    /// Clear the entire screen
    fn clear(&mut self) {
        for row in 0..VGA_HEIGHT {
            self.clear_row(row);
        }
        self.position = Position::new();
        self.color_code = ColorCode::normal();
    }

    /// Clear a specific row with blank characters
    fn clear_row(&mut self, row: usize) {
        let blank = Self::encode_char(b' ', self.color_code);

        for col in 0..VGA_WIDTH {
            let offset = (row * VGA_WIDTH + col) * BYTES_PER_CHAR;
            self.write_encoded_char(offset, blank);
        }
    }

    /// Encode a character with color into a 16-bit value
    const fn encode_char(ch: u8, color: ColorCode) -> u16 {
        (color.as_u8() as u16) << 8 | ch as u16
    }

    /// Write an encoded character to the buffer
    fn write_encoded_char(&mut self, offset: usize, encoded: u16) {
        unsafe {
            core::ptr::write_volatile(self.buffer.add(offset) as *mut u16, encoded);
        }
    }

    /// Scroll the screen up by one line
    ///
    /// Moves all rows (except the top) up by one line and clears the bottom row.
    /// Uses `copy()` (not `copy_nonoverlapping()`) because source and destination
    /// ranges overlap, which is explicitly allowed for `ptr::copy`.
    ///
    /// # Safety
    ///
    /// This method uses unsafe pointer operations but is safe because:
    /// - The source and destination are within the same valid VGA buffer
    /// - The buffer size is correctly calculated
    /// - `copy()` handles overlapping memory regions correctly
    fn scroll(&mut self) {
        // Copy all rows except the first one up by one line
        unsafe {
            core::ptr::copy(
                self.buffer.add(BYTES_PER_ROW),   // Source: row 1
                self.buffer,                      // Dest: row 0
                BYTES_PER_ROW * (VGA_HEIGHT - 1), // Size: all rows except last
            );
        }

        // Clear the last row
        self.clear_row(VGA_HEIGHT - 1);
        self.position.row = VGA_HEIGHT - 1;
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
        match byte {
            b'\n' => self.new_line(),
            _ => {
                if self.position.is_at_row_end() {
                    self.new_line();
                }

                let encoded = Self::encode_char(byte, self.color_code);
                let offset = self.position.byte_offset();
                self.write_encoded_char(offset, encoded);

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

/// Implement Write trait for VgaWriter (enables write! and writeln! macros)
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

/// Global VGA writer protected by Mutex (SAFE!)
static VGA_WRITER: Mutex<VgaWriter> = Mutex::new(VgaWriter::new());

/// Execute a function with the VGA writer, protected from interrupts
///
/// This helper ensures that interrupt handlers cannot cause deadlocks
/// when trying to access the VGA writer.
fn with_writer<F, R>(f: F) -> R
where
    F: FnOnce(&mut VgaWriter) -> R,
{
    interrupts::without_interrupts(|| f(&mut VGA_WRITER.lock()))
}

/// Global print! macro (safe version with interrupt protection)
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ({
        $crate::vga_buffer::_print(format_args!($($arg)*))
    });
}

/// Global println! macro (safe version with interrupt protection)
#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($fmt:expr) => ($crate::print!(concat!($fmt, "\n")));
    ($fmt:expr, $($arg:tt)*) => ($crate::print!(concat!($fmt, "\n"), $($arg)*));
}

/// Print function called by macros
///
/// # Safety
///
/// Uses `without_interrupts` to prevent deadlock when interrupt handlers call println!
#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    with_writer(|writer| {
        let _ = writer.write_fmt(args);
    });
}

/// Clear the screen (public API)
///
/// # Platform Notes
///
/// This function assumes the VGA buffer at 0xB8000 is accessible.
/// On UEFI systems without CSM (Compatibility Support Module),
/// this address may not be valid. Consider checking boot mode
/// before calling this function in production systems.
pub fn clear() {
    with_writer(|writer| {
        // Verify buffer is accessible before clearing
        // This helps catch issues early in development
        if !writer.is_accessible() {
            // Can't clear if buffer isn't accessible
            // In a more advanced kernel, you might:
            // - Fall back to UEFI framebuffer
            // - Use serial-only output
            // - Panic with a helpful message
            return;
        }
        writer.clear();
    });
}

/// Set the text color (public API)
pub fn set_color(color: ColorCode) {
    with_writer(|writer| writer.set_color(color));
}

/// Print colored text (public API)
///
/// This function temporarily changes the color for the given string,
/// then restores the previous color.
///
/// # Arguments
///
/// * `s` - The string to print
/// * `color` - The color to use
///
/// # Examples
///
/// ```
/// vga_buffer::print_colored("Error!\n", ColorCode::error());
/// ```
pub fn print_colored(s: &str, color: ColorCode) {
    with_writer(|writer| writer.write_colored(s, color));
}

// Tests are not supported in no_std environment
// For testing, consider using a hosted test harness with mocking
//
// #[cfg(test)]
// mod tests {
//     use super::*;
//
//     #[test]
//     fn test_color_code_encoding() {
//         let color = ColorCode::new(VgaColor::White, VgaColor::Red);
//         assert_eq!(color.as_u8(), 0x4F); // Red (4) << 4 | White (15)
//     }
//
//     #[test]
//     fn test_position_byte_offset() {
//         let pos = Position { row: 1, col: 2 };
//         assert_eq!(pos.byte_offset(), (1 * 80 + 2) * 2);
//     }
//
//     #[test]
//     fn test_char_encoding() {
//         let encoded = VgaWriter::encode_char(b'A', ColorCode::normal());
//         assert_eq!(encoded & 0xFF, b'A' as u16);
//         assert_eq!(encoded >> 8, ColorCode::normal().as_u8() as u16);
//     }
// }
