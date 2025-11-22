// src/vga_buffer/constants.rs

//! Constants for VGA text buffer operations

/// VGA text buffer physical memory address (x86/x86_64 PC/AT standard)
///
/// This address (0xB8000) is specific to PC/AT-compatible systems
/// (x86, x86_64) and represents the standard legacy VGA text mode buffer.
///
/// **Platform Dependency:** This is x86/x86_64 specific.
/// Other architectures will require different display backends:
/// - Framebuffer devices (common on ARM/AArch64/RISC-V)
/// - Serial console as primary output
/// - GPU-based rendering
#[cfg(any(target_arch = "x86_64", target_arch = "x86"))]
pub const VGA_BUFFER_ADDR: usize = 0xb8000;

/// VGA buffer address stub for non-x86 architectures
///
/// On non-x86 platforms, VGA text mode is not available.
/// This constant is provided for compatibility but should not be used.
/// Alternative display methods should be implemented.
#[cfg(not(any(target_arch = "x86_64", target_arch = "x86")))]
pub const VGA_BUFFER_ADDR: usize = 0; // Stub - not available on this architecture

/// Screen dimensions
pub const VGA_WIDTH: usize = 80;
pub const VGA_HEIGHT: usize = 25;

/// Total number of character cells in the VGA buffer
#[allow(dead_code)]
pub const CELL_COUNT: usize = VGA_WIDTH * VGA_HEIGHT;

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
