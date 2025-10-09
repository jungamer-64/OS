// src/vga_buffer.rs

//! VGA text mode driver with interrupt-safe Mutex protection
//!
//! This module provides safe VGA text buffer access with the following features:
//! - 16-color support (VGA standard palette)
//! - Auto-scrolling when screen is full
//! - Interrupt-safe locking (prevents deadlock in interrupt handlers)
//! - fmt::Write trait implementation for print!/println! macros
//! - Optimized scrolling with copy_nonoverlapping
//!
//! Note: VGA text mode only supports ASCII characters (0x00-0x7F and extended ASCII).
//! Multi-byte characters (e.g., Japanese, Chinese) will display as garbage.
//!
//! To support UTF-8/Unicode characters, you would need to:
//! 1. Implement a custom font bitmap/glyph renderer
//! 2. Use framebuffer graphics mode instead of text mode
//! 3. Implement Unicode to glyph mapping
//! 4. Handle character composition (combining characters, etc.)

use core::fmt::{self, Write};
use spin::Mutex;
use x86_64::instructions::interrupts;

/// VGA text buffer constants
const VGA_BUFFER: usize = 0xb8000;
const VGA_WIDTH: usize = 80;
const VGA_HEIGHT: usize = 25;

/// VGA color codes
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

/// Create VGA color byte from foreground and background colors
const fn vga_color_code(fg: VgaColor, bg: VgaColor) -> u8 {
    (bg as u8) << 4 | (fg as u8)
}

/// Default color schemes (all exported for public use)
pub const COLOR_NORMAL: u8 = vga_color_code(VgaColor::LightGray, VgaColor::Black);
pub const COLOR_INFO: u8 = vga_color_code(VgaColor::LightCyan, VgaColor::Black);
pub const COLOR_SUCCESS: u8 = vga_color_code(VgaColor::LightGreen, VgaColor::Black);
pub const COLOR_WARNING: u8 = vga_color_code(VgaColor::Yellow, VgaColor::Black);
pub const COLOR_ERROR: u8 = vga_color_code(VgaColor::LightRed, VgaColor::Black);
pub const COLOR_PANIC: u8 = vga_color_code(VgaColor::White, VgaColor::Red);

/// VGA Writer structure - encapsulates all VGA state
struct VgaWriter {
    row: usize,
    col: usize,
    color_code: u8,
    buffer: *mut u8,
}

// SAFETY: VGA buffer is always accessible and we're in a single-threaded kernel
// The Mutex ensures exclusive access, so this is safe
unsafe impl Send for VgaWriter {}
unsafe impl Sync for VgaWriter {}

/// Global VGA writer protected by Mutex (SAFE!)
/// Using spin::Mutex directly instead of lazy_static for better compatibility
static VGA_WRITER: Mutex<VgaWriter> = Mutex::new(VgaWriter {
    row: 0,
    col: 0,
    color_code: COLOR_NORMAL,
    buffer: VGA_BUFFER as *mut u8,
});

impl VgaWriter {
    /// Set text color
    fn set_color(&mut self, color: u8) {
        self.color_code = color;
    }

    /// Clear the entire screen
    fn clear(&mut self) {
        for row in 0..VGA_HEIGHT {
            self.clear_row(row);
        }
        self.row = 0;
        self.col = 0;
        self.color_code = COLOR_NORMAL;
    }

    /// Clear a specific row
    fn clear_row(&mut self, row: usize) {
        let blank_lo = b' ';
        let blank_hi = self.color_code;
        for col in 0..VGA_WIDTH {
            let pos = (row * VGA_WIDTH + col) * 2;
            unsafe {
                core::ptr::write_volatile(self.buffer.offset(pos as isize), blank_lo);
                core::ptr::write_volatile(self.buffer.offset((pos + 1) as isize), blank_hi);
            }
        }
    }

    /// Scroll the screen up by one line (optimized with copy, overlap-safe)
    fn scroll(&mut self) {
        // Copy all rows up by one line (much faster than byte-by-byte)
        // Using copy() instead of copy_nonoverlapping() because ranges overlap
        for row in 1..VGA_HEIGHT {
            let src = row * VGA_WIDTH * 2;
            let dst = (row - 1) * VGA_WIDTH * 2;
            unsafe {
                core::ptr::copy(
                    self.buffer.add(src),
                    self.buffer.add(dst),
                    VGA_WIDTH * 2, // 1 row in bytes
                );
            }
        }
        self.clear_row(VGA_HEIGHT - 1);
        self.row = VGA_HEIGHT - 1;
    }

    /// Move to a new line
    fn new_line(&mut self) {
        self.col = 0;
        self.row += 1;
        if self.row >= VGA_HEIGHT {
            self.scroll();
        }
    }

    /// Write a single byte to the screen
    fn write_byte(&mut self, byte: u8) {
        match byte {
            b'\n' => self.new_line(),
            byte => {
                if self.col >= VGA_WIDTH {
                    self.new_line();
                }
                let pos = (self.row * VGA_WIDTH + self.col) * 2;
                unsafe {
                    core::ptr::write_volatile(self.buffer.offset(pos as isize), byte);
                    core::ptr::write_volatile(
                        self.buffer.offset((pos + 1) as isize),
                        self.color_code,
                    );
                }
                self.col += 1;
            }
        }
    }

    /// Write a string with temporary color
    fn write_colored(&mut self, s: &str, color: u8) {
        let old_color = self.color_code;
        self.set_color(color);
        self.write_str(s).unwrap();
        self.set_color(old_color);
    }
}

/// Implement Write trait for VgaWriter (enables write! and writeln! macros)
impl Write for VgaWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for byte in s.bytes() {
            // Only display printable ASCII or newline
            if (0x20..=0x7e).contains(&byte) || byte == b'\n' {
                self.write_byte(byte);
            } else {
                // Replace non-printable with ■ (0xfe)
                self.write_byte(0xfe);
            }
        }
        Ok(())
    }
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
/// SAFETY: Uses without_interrupts to prevent deadlock when interrupt handlers call println!
#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    interrupts::without_interrupts(|| {
        let _ = VGA_WRITER.lock().write_fmt(args);
    });
}

/// Helper function to clear the screen (public API)
/// SAFETY: Uses without_interrupts to prevent deadlock
pub fn clear() {
    interrupts::without_interrupts(|| {
        VGA_WRITER.lock().clear();
    });
}

/// Helper function to set color (public API)
/// SAFETY: Uses without_interrupts to prevent deadlock
pub fn set_color(color: u8) {
    interrupts::without_interrupts(|| {
        VGA_WRITER.lock().set_color(color);
    });
}

/// Helper function for colored output (public API)
/// SAFETY: Uses without_interrupts to prevent deadlock
pub fn print_colored(s: &str, color: u8) {
    interrupts::without_interrupts(|| {
        VGA_WRITER.lock().write_colored(s, color);
    });
}
