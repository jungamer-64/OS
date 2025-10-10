// src/vga_buffer/constants.rs

//! Constants for VGA text buffer operations

/// VGA text buffer physical memory address
pub const VGA_BUFFER_ADDR: usize = 0xb8000;

/// Screen dimensions
pub const VGA_WIDTH: usize = 80;
pub const VGA_HEIGHT: usize = 25;

/// Bytes per character (1 byte ASCII + 1 byte color attribute)
#[allow(dead_code)]
pub const BYTES_PER_CHAR: usize = 2;

/// Bytes per row (80 characters * 2 bytes each)
#[allow(dead_code)]
pub const BYTES_PER_ROW: usize = VGA_WIDTH * BYTES_PER_CHAR;

/// Total buffer size in bytes
#[allow(dead_code)]
pub const BUFFER_SIZE: usize = VGA_HEIGHT * BYTES_PER_ROW;

/// ASCII character range for printable characters
pub const PRINTABLE_ASCII_START: u8 = 0x20;
pub const PRINTABLE_ASCII_END: u8 = 0x7e;

/// Replacement character for non-printable characters (■)
pub const REPLACEMENT_CHAR: u8 = 0xfe;
