// src/display/panic.rs

//! Panic information display module
//!
//! Provides formatted output of panic information to both serial and VGA
//! outputs. Designed to be extremely defensive to prevent panic-during-panic.

use super::backend::{default_display_backend, DisplayHardware};
use crate::diagnostics::DIAGNOSTICS;
use crate::vga_buffer::ColorCode;
use crate::{serial_print, serial_println};
use core::cmp;
use core::fmt::{self, Write};
use core::panic::PanicInfo;

/// Separator line for panic messages
const PANIC_SEPARATOR: &str = "========================================\n";
const PANIC_SHORT_SEP: &str = "----------------------------------------\n";

/// Maximum length for truncated messages (prevent excessive output)
const MAX_MESSAGE_LENGTH: usize = 500;

#[inline]
fn print_display<O: DisplayHardware>(out: &mut O, text: &str, color: ColorCode) {
    if let Err(err) = out.write_colored(text, color) {
        if crate::serial::is_available() {
            serial_println!("[WARN] Display panic output failed: {}", err);
        }
    }
}

struct TruncatingBuffer {
    buf: [u8; MAX_MESSAGE_LENGTH],
    len: usize,
    truncated: bool,
}

impl TruncatingBuffer {
    const fn new() -> Self {
        Self {
            buf: [0; MAX_MESSAGE_LENGTH],
            len: 0,
            truncated: false,
        }
    }

    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buf[..self.len]).unwrap_or("<fmt error>")
    }

    const fn was_truncated(&self) -> bool {
        self.truncated
    }
}

impl Write for TruncatingBuffer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        if self.truncated {
            return Ok(());
        }

        let remaining = self.buf.len().saturating_sub(self.len);
        if remaining == 0 {
            self.truncated = true;
            return Ok(());
        }

        let mut copy_len = cmp::min(remaining, s.len());
        while copy_len > 0 && !s.is_char_boundary(copy_len) {
            copy_len -= 1;
        }

        if copy_len == 0 {
            self.truncated = true;
            return Ok(());
        }

        self.buf[self.len..self.len + copy_len].copy_from_slice(&s.as_bytes()[..copy_len]);
        self.len += copy_len;
        if copy_len < s.len() {
            self.truncated = true;
        }

        Ok(())
    }
}

#[cfg(all(test, feature = "std-tests"))]
fn truncate_borrowed_message(message: &str) -> (&str, bool) {
    if message.len() <= MAX_MESSAGE_LENGTH {
        return (message, false);
    }

    let mut end = MAX_MESSAGE_LENGTH;
    while end > 0 && !message.is_char_boundary(end) {
        end -= 1;
    }

    if end == 0 {
        return ("", true);
    }

    (&message[..end], true)
}

fn log_truncation_notice() {
    serial_println!("  [message truncated to {} chars]", MAX_MESSAGE_LENGTH);
}

/// Format panic message into a buffer with truncation support.
///
/// Returns the formatted message and whether truncation occurred.
fn extract_panic_message(info: &PanicInfo) -> (TruncatingBuffer, bool) {
    let mut buffer = TruncatingBuffer::new();

    if let Some(msg_str) = info.message().as_str() {
        let _ = buffer.write_str(msg_str);
    } else {
        let _ = fmt::write(&mut buffer, format_args!("{}", info.message()));
    }

    let truncated = buffer.was_truncated();
    (buffer, truncated)
}

/// Display panic information to serial output
///
/// Outputs comprehensive debug information including:
/// - Panic message (truncated if too long)
/// - Source location (file, line, column)
/// - Formatted separator lines for readability
///
/// # Safety
///
/// This function is extremely defensive:
/// - Checks serial availability before any output
/// - Never panics (can't panic in panic handler)
/// - Never allocates memory
/// - Never performs blocking operations
///
/// # Arguments
///
/// * `info` - Panic information from the panic handler
pub fn display_panic_info_serial(info: &PanicInfo) {
    // Defensive check: don't try to use serial if not available
    if !crate::serial::is_available() {
        return;
    }

    // Start with visual separator
    serial_println!("");
    serial_separator();

    // Main panic header
    serial_println!("       !!! KERNEL PANIC !!!");

    serial_separator();

    // Extract and display message
    display_panic_message_serial(info);

    // Extract and display location
    display_panic_location_serial(info);

    serial_separator();

    // Final status message
    serial_println!("System halted. CPU entering hlt loop.");
    serial_println!("No further execution will occur.");

    serial_separator();
}

/// Display panic message to serial
///
/// Extracts the panic message and displays it, with truncation
/// if the message is excessively long.
fn display_panic_message_serial(info: &PanicInfo) {
    if !crate::serial::is_available() {
        return;
    }

    serial_println!();
    serial_println!("[PANIC MESSAGE]");
    serial_short_separator();

    // Display message with defensive formatting
    serial_print!("  ");

    let (buffer, truncated) = extract_panic_message(info);
    serial_println!("{}", buffer.as_str());
    if truncated {
        log_truncation_notice();
    }

    serial_println!();
}

/// Display panic location to serial
///
/// Shows the source file, line number, and column number where
/// the panic occurred.
fn display_panic_location_serial(info: &PanicInfo) {
    if !crate::serial::is_available() {
        return;
    }

    serial_println!("[PANIC LOCATION]");
    serial_short_separator();

    if let Some(location) = info.location() {
        DIAGNOSTICS.record_panic_location(location.line(), location.column());
        serial_println!("  File:   {}", location.file());
        serial_println!("  Line:   {}", location.line());
        serial_println!("  Column: {}", location.column());
    } else {
        serial_println!("  Location information not available");
    }

    serial_println!();
}

/// Display panic information to VGA output
///
/// Provides a user-friendly summary on the VGA display.
/// Less detailed than serial output to avoid overwhelming the user.
///
/// # Safety
///
/// - Checks VGA accessibility before any output
/// - Uses only safe VGA buffer operations
/// - Never panics
///
/// # Arguments
///
/// * `info` - Panic information from the panic handler
pub fn display_panic_info_vga(info: &PanicInfo) {
    let mut display = default_display_backend();

    // Defensive check: don't try to use the display if not accessible
    if !display.is_available() {
        return;
    }

    // Clear screen header for visibility
    print_display(&mut display, "\n", ColorCode::normal());

    // Main panic header with high-visibility colors
    print_display(&mut display, "!!! KERNEL PANIC !!!\n", ColorCode::panic());

    print_display(&mut display, "\n", ColorCode::normal());

    // Display location info if available
    if let Some(location) = info.location() {
        DIAGNOSTICS.record_panic_location(location.line(), location.column());
        display_location_vga(&mut display, location);
    } else {
        print_display(&mut display, "Location: Unknown\n", ColorCode::error());
    }

    print_display(&mut display, "\n", ColorCode::normal());

    // Display brief message summary
    display_message_summary_vga(&mut display, info);

    // Instructions for user
    display_user_instructions_vga(&mut display);
}

/// Truncate file path for display if too long.
///
/// Returns the last N characters of the path, which typically
/// contains the most relevant information (filename and parent dirs).
const MAX_FILE_PATH_LENGTH: usize = 50;

fn truncate_file_path(file: &str) -> &str {
    if file.len() > MAX_FILE_PATH_LENGTH {
        &file[file.len() - MAX_FILE_PATH_LENGTH..]
    } else {
        file
    }
}

/// Display location information on VGA
fn display_location_vga<O: DisplayHardware>(out: &mut O, location: &core::panic::Location) {
    // File name
    print_display(out, "File: ", ColorCode::error());

    let display_file = truncate_file_path(location.file());
    print_display(out, display_file, ColorCode::normal());
    print_display(out, "\n", ColorCode::normal());

    // Line and column numbers
    // In no_std without alloc, we can't easily format numbers
    // Serial output will show the actual numbers
    print_display(out, "Line: <see serial>\n", ColorCode::error());
    print_display(out, "Column: <see serial>\n", ColorCode::error());
}

/// Display message summary on VGA
fn display_message_summary_vga<O: DisplayHardware>(out: &mut O, info: &PanicInfo) {
    print_display(out, "Message: ", ColorCode::error());

    // Try to extract string message if simple, otherwise show generic message
    if let Some(msg_str) = info.message().as_str() {
        // Truncate long messages for VGA display
        let display_msg = if msg_str.len() > 60 {
            "<message too long - see serial>"
        } else {
            msg_str
        };
        print_display(out, display_msg, ColorCode::normal());
    } else {
        print_display(out, "<see serial output>", ColorCode::normal());
    }
    print_display(out, "\n", ColorCode::normal());
}

/// Display user instructions on VGA
fn display_user_instructions_vga<O: DisplayHardware>(out: &mut O) {
    print_display(out, "\n", ColorCode::normal());

    print_display(
        out,
        "The system has encountered a critical error.\n",
        ColorCode::warning(),
    );

    // Check if serial is available for detailed info
    if crate::serial::is_available() {
        print_display(
            out,
            "See serial output (COM1) for detailed information.\n",
            ColorCode::normal(),
        );
    } else {
        print_display(
            out,
            "Serial port unavailable - limited debug info.\n",
            ColorCode::warning(),
        );
    }

    print_display(out, "\n", ColorCode::normal());

    print_display(
        out,
        "System halted. No further execution possible.\n",
        ColorCode::error(),
    );

    print_display(out, "Please reboot the system.\n", ColorCode::normal());
}

/// Print separator line to serial
fn serial_separator() {
    serial_print!("{}", PANIC_SEPARATOR);
}

/// Print short separator line to serial
fn serial_short_separator() {
    serial_print!("{}", PANIC_SHORT_SEP);
}

#[cfg(all(test, feature = "std-tests"))]
mod tests {
    use super::*;
    use core::iter;
    use std::string::String;

    #[test]
    fn test_max_message_length_reasonable() {
        assert!(MAX_MESSAGE_LENGTH >= 100);
        assert!(MAX_MESSAGE_LENGTH <= 1000);
    }

    #[test]
    fn test_separator_lengths() {
        assert_eq!(PANIC_SEPARATOR.len(), 41); // 40 '=' + newline
        assert_eq!(PANIC_SHORT_SEP.len(), 41); // 40 '-' + newline
    }

    #[test]
    fn truncate_borrowed_message_respects_utf8_boundaries() {
        let long_msg: String = iter::repeat("é")
            .take((MAX_MESSAGE_LENGTH / 2) + 50)
            .collect();
        let (snippet, truncated) = truncate_borrowed_message(&long_msg);
        assert!(truncated, "long UTF-8 message should be truncated");
        assert!(snippet.len() <= MAX_MESSAGE_LENGTH);
        assert!(snippet.is_char_boundary(snippet.len()));
    }

    #[test]
    fn truncating_buffer_limits_output() {
        let mut buffer = TruncatingBuffer::new();
        let long_input: String = iter::repeat('A').take(MAX_MESSAGE_LENGTH + 128).collect();
        let _ = buffer.write_str(&long_input);

        assert!(buffer.was_truncated());
        assert!(buffer.as_str().len() <= MAX_MESSAGE_LENGTH);
    }

    #[test]
    fn truncating_buffer_accepts_multiple_writes() {
        let mut buffer = TruncatingBuffer::new();
        let part: String = iter::repeat('B').take(MAX_MESSAGE_LENGTH / 2).collect();
        let _ = buffer.write_str(&part);
        let _ = buffer.write_str(&part);
        let _ = buffer.write_str("extra");

        assert!(buffer.was_truncated());
        assert!(buffer.as_str().len() <= MAX_MESSAGE_LENGTH);
    }
}
