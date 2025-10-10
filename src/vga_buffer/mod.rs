// src/vga_buffer/mod.rs

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

mod color;
mod constants;
mod writer;

pub use color::ColorCode;
#[allow(unused_imports)]
pub use color::VgaColor;
use core::fmt;
use core::sync::atomic::Ordering;
use spin::Mutex;
use writer::{VgaWriter, BUFFER_ACCESSIBLE};
use x86_64::instructions::interrupts;

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
        use core::fmt::Write;
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
