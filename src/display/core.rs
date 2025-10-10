// src/display/core.rs

//! Display core functionality
//!
//! Provides the fundamental output abstraction layer that allows
//! writing to multiple outputs (VGA, serial) through a unified interface.
//!
//! # Design
//!
//! The `Output` trait abstracts over different output targets, allowing
//! functions to be output-agnostic. This enables:
//! - Testing with mock outputs
//! - Flexible output routing
//! - Consistent formatting across outputs

use crate::vga_buffer::ColorCode;
use crate::{serial_print, serial_println};
use core::fmt::{self, Write};

/// Text output target abstraction
///
/// Implementors of this trait can receive formatted text output
/// with color information.
pub trait Output {
    /// Write text with a specific color
    ///
    /// # Arguments
    ///
    /// * `text` - The text to write
    /// * `color` - The color code to use
    fn write(&mut self, text: &str, color: ColorCode);
}

/// Hardware-backed dual output (VGA + serial)
///
/// Writes to both VGA buffer and serial port simultaneously.
/// This ensures output is visible both on screen and in logs.
pub struct HardwareOutput;

impl Output for HardwareOutput {
    fn write(&mut self, text: &str, color: ColorCode) {
        // Write to VGA if accessible
        if crate::vga_buffer::is_accessible() {
            if let Err(err) = crate::vga_buffer::print_colored(text, color) {
                if crate::serial::is_available() {
                    serial_println!("[WARN] VGA broadcast failed: {}", err.as_str());
                }
            }
        }

        // Write to serial if available
        if crate::serial::is_available() {
            serial_print!("{}", text);
        }
    }
}

/// Create a hardware output instance
#[allow(clippy::redundant_pub_crate)]
pub(crate) const fn hardware_output() -> HardwareOutput {
    HardwareOutput
}

struct OutputWriter<'a, O> {
    out: &'a mut O,
    color: ColorCode,
}

impl<O: Output> Write for OutputWriter<'_, O> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.out.write(s, self.color);
        Ok(())
    }
}

/// Broadcast a message to hardware outputs
///
/// # Arguments
///
/// * `message` - The message to broadcast
/// * `color` - The color to use
///
/// # Examples
///
/// ```
/// broadcast("Hello, world!\n", ColorCode::normal());
/// ```
#[allow(dead_code)]
pub fn broadcast(message: &str, color: ColorCode) {
    let mut out = hardware_output();
    broadcast_with(&mut out, message, color);
}

/// Broadcast a message to a specific output
///
/// # Arguments
///
/// * `out` - The output target
/// * `message` - The message to broadcast
/// * `color` - The color to use
pub fn broadcast_with<O: Output>(out: &mut O, message: &str, color: ColorCode) {
    broadcast_args_with(out, format_args!("{message}"), color);
}

/// Broadcast formatted arguments to hardware outputs
///
/// # Arguments
///
/// * `args` - Format arguments
/// * `color` - The color to use
#[allow(dead_code)]
pub fn broadcast_args(args: fmt::Arguments, color: ColorCode) {
    let mut out = hardware_output();
    broadcast_args_with(&mut out, args, color);
}

/// Broadcast formatted arguments to a specific output
///
/// This is the core formatting function that all other broadcast
/// functions eventually call.
///
/// # Arguments
///
/// * `out` - The output target
/// * `args` - Format arguments
/// * `color` - The color to use
///
/// # Implementation Notes
///
/// Streams formatted data directly to the provided output without requiring
/// intermediate heap buffers. Formatting errors are ignored intentionally,
/// matching the infallible write semantics of the concrete outputs.
pub fn broadcast_args_with<O: Output>(out: &mut O, args: fmt::Arguments, color: ColorCode) {
    let mut writer = OutputWriter { out, color };
    let _ = fmt::write(&mut writer, args);
}

// NOTE: Unit tests removed as they require std library features (Vec, String)
// that are not available in this no_std environment.
// Integration tests should be used instead for testing this functionality.
