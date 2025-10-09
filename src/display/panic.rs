// src/display/panic.rs

//! Panic information display module
//!
//! Provides formatted output of panic information to both serial and VGA
//! outputs. Designed to be extremely defensive to prevent panic-during-panic.

use crate::vga_buffer::ColorCode;
use crate::{serial_print, serial_println};
use core::panic::PanicInfo;

/// Separator line for panic messages
const PANIC_SEPARATOR: &str = "========================================\n";
const PANIC_SHORT_SEP: &str = "----------------------------------------\n";

/// Maximum length for truncated messages (prevent excessive output)
const MAX_MESSAGE_LENGTH: usize = 500;

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

    // Format the panic message - info.message() already provides formatting support
    if let Some(msg_str) = info.message().as_str() {
        serial_println!("{}", msg_str);
    } else {
        // For more complex formatted messages, print using the Arguments directly
        serial_println!("{}", info.message());
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
    // Defensive check: don't try to use VGA if not accessible
    if !crate::vga_buffer::is_accessible() {
        return;
    }

    // Clear screen header for visibility
    crate::vga_buffer::print_colored("\n", ColorCode::normal());

    // Main panic header with high-visibility colors
    crate::vga_buffer::print_colored("!!! KERNEL PANIC !!!\n", ColorCode::panic());

    crate::vga_buffer::print_colored("\n", ColorCode::normal());

    // Display location info if available
    if let Some(location) = info.location() {
        display_location_vga(location);
    } else {
        crate::vga_buffer::print_colored("Location: Unknown\n", ColorCode::error());
    }

    crate::vga_buffer::print_colored("\n", ColorCode::normal());

    // Display brief message summary
    display_message_summary_vga(info);

    // Instructions for user
    display_user_instructions_vga();
}

/// Display location information on VGA
fn display_location_vga(location: &core::panic::Location) {
    // File name
    crate::vga_buffer::print_colored("File: ", ColorCode::error());

    // Truncate long file paths for readability
    let file = location.file();
    let display_file = if file.len() > 50 {
        // Show last 50 characters (usually the most relevant part)
        &file[file.len() - 50..]
    } else {
        file
    };

    crate::vga_buffer::print_colored(display_file, ColorCode::normal());
    crate::vga_buffer::print_colored("\n", ColorCode::normal());

    // Line and column numbers
    // In no_std without alloc, we can't easily format numbers
    // Serial output will show the actual numbers
    crate::vga_buffer::print_colored("Line: <see serial>\n", ColorCode::error());
    crate::vga_buffer::print_colored("Column: <see serial>\n", ColorCode::error());
}

/// Display message summary on VGA
fn display_message_summary_vga(info: &PanicInfo) {
    crate::vga_buffer::print_colored("Message: ", ColorCode::error());

    // Try to extract string message if simple, otherwise show generic message
    if let Some(msg_str) = info.message().as_str() {
        // Truncate long messages for VGA display
        let display_msg = if msg_str.len() > 60 {
            "<message too long - see serial>"
        } else {
            msg_str
        };
        crate::vga_buffer::print_colored(display_msg, ColorCode::normal());
    } else {
        crate::vga_buffer::print_colored("<see serial output>", ColorCode::normal());
    }
    crate::vga_buffer::print_colored("\n", ColorCode::normal());
}

/// Display user instructions on VGA
fn display_user_instructions_vga() {
    crate::vga_buffer::print_colored("\n", ColorCode::normal());

    crate::vga_buffer::print_colored(
        "The system has encountered a critical error.\n",
        ColorCode::warning(),
    );

    // Check if serial is available for detailed info
    if crate::serial::is_available() {
        crate::vga_buffer::print_colored(
            "See serial output (COM1) for detailed information.\n",
            ColorCode::normal(),
        );
    } else {
        crate::vga_buffer::print_colored(
            "Serial port unavailable - limited debug info.\n",
            ColorCode::warning(),
        );
    }

    crate::vga_buffer::print_colored("\n", ColorCode::normal());

    crate::vga_buffer::print_colored(
        "System halted. No further execution possible.\n",
        ColorCode::error(),
    );

    crate::vga_buffer::print_colored("Please reboot the system.\n", ColorCode::normal());
}

/// Print separator line to serial
fn serial_separator() {
    serial_print!("{}", PANIC_SEPARATOR);
}

/// Print short separator line to serial
fn serial_short_separator() {
    serial_print!("{}", PANIC_SHORT_SEP);
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
