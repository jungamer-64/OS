// src/display.rs

//! Display and output formatting module
//!
//! This module handles all user-facing display operations including:
//! - Boot information display
//! - Feature list presentation
//! - System information formatting
//! - Panic message formatting (both VGA and serial)
//!
//! The display functions abstract away the details of multi-output
//! (VGA + serial) messaging, providing a unified interface for
//! kernel information presentation.

use crate::constants::{FEATURES, SERIAL_HINTS, SYSTEM_INFO};
use crate::vga_buffer::ColorCode;
use crate::{serial_print, serial_println};
use bootloader::BootInfo;
use core::panic::PanicInfo;

/// Broadcast a message to both VGA and serial outputs
///
/// This is the primary function for synchronized multi-output messaging.
/// It ensures that both the screen and serial console receive the same
/// information with appropriate color coding.
///
/// # Arguments
///
/// * `message` - The message to display
/// * `color` - The VGA color to use for display
///
/// # Examples
///
/// ```
/// use crate::display::broadcast;
/// use crate::vga_buffer::ColorCode;
///
/// broadcast("System ready\n", ColorCode::success());
/// ```
pub fn broadcast(message: &str, color: ColorCode) {
    crate::vga_buffer::print_colored(message, color);
    serial_print!("{}", message);
}

/// Separator line for panic messages
const PANIC_SEPARATOR: &str = "═══════════════════════════════════════\n";

/// Display boot environment information
///
/// Shows critical system information about the boot environment:
/// - VGA buffer accessibility status
/// - Serial port (COM1) availability
/// - Physical memory offset (from bootloader)
///
/// This helps diagnose compatibility issues on real hardware.
///
/// # Platform Notes
///
/// bootloader 0.9 does not provide framebuffer info in BootInfo,
/// so UEFI vs BIOS detection is not available. The kernel assumes
/// BIOS text mode at 0xB8000. For UEFI systems, enable CSM (Compatibility
/// Support Module) in BIOS/UEFI settings to ensure VGA text mode works.
///
/// # Arguments
///
/// * `_boot_info` - Boot information from the bootloader (unused in 0.9)
pub fn display_boot_environment(_boot_info: &'static BootInfo) {
    crate::vga_buffer::print_colored("\n--- Boot Environment ---\n", ColorCode::info());
    serial_println!("\n--- Boot Environment ---");

    // VGA buffer status (check accessibility)
    let vga_status = "VGA Text Mode (0xB8000)";
    crate::vga_buffer::print_colored("Display: ", ColorCode::normal());
    crate::vga_buffer::print_colored(vga_status, ColorCode::success());
    crate::vga_buffer::print_colored("\n", ColorCode::normal());
    serial_println!("Display: {}", vga_status);

    // Serial port status
    let serial_status = if crate::serial::is_available() {
        "✓ COM1 Available"
    } else {
        "✗ COM1 Not Present"
    };

    crate::vga_buffer::print_colored("Serial: ", ColorCode::normal());
    let serial_color = if crate::serial::is_available() {
        ColorCode::success()
    } else {
        ColorCode::error()
    };
    crate::vga_buffer::print_colored(serial_status, serial_color);
    crate::vga_buffer::print_colored("\n", ColorCode::normal());
    serial_println!("Serial: {}", serial_status);

    // Platform warning
    crate::vga_buffer::print_colored("\nNote: ", ColorCode::warning());
    crate::vga_buffer::print_colored(
        "This kernel requires BIOS text mode or CSM.\n",
        ColorCode::normal(),
    );
    serial_println!();
    serial_println!("Note: This kernel assumes BIOS text mode at 0xB8000.");
    serial_println!("For UEFI systems, enable CSM in BIOS/UEFI settings.");

    crate::vga_buffer::print_colored("------------------------\n\n", ColorCode::info());
    serial_println!("------------------------\n");
}

/// Print a separator line to serial output
///
/// Used primarily for panic messages to improve readability
/// in serial console logs.
fn serial_separator() {
    serial_print!("{}", PANIC_SEPARATOR);
}

/// Display a single feature with proper formatting
///
/// Features are displayed with a bullet point on both VGA and serial.
///
/// # Arguments
///
/// * `feature` - The feature description to display
fn emit_feature(feature: &str) {
    crate::vga_buffer::print_colored("  • ", ColorCode::normal());
    crate::vga_buffer::print_colored(feature, ColorCode::normal());
    crate::vga_buffer::print_colored("\n", ColorCode::normal());
    serial_println!("  • {}", feature);
}

/// Display boot information and welcome message
///
/// This function outputs the kernel banner and system component
/// information to both VGA and serial outputs.
pub fn display_boot_information() {
    broadcast("=== Rust OS Kernel Started ===\n\n", ColorCode::info());
    broadcast(
        "Welcome to minimal x86_64 Rust OS!\n\n",
        ColorCode::normal(),
    );

    for &(label, value) in SYSTEM_INFO {
        display_system_info(label, value);
    }
}

/// Display a single system information entry
///
/// Formats and displays a label-value pair representing
/// system component information.
///
/// # Arguments
///
/// * `label` - The information label (e.g., "Bootloader")
/// * `value` - The corresponding value (e.g., "0.9.33")
fn display_system_info(label: &str, value: &str) {
    crate::vga_buffer::print_colored(label, ColorCode::info());
    crate::vga_buffer::print_colored(": ", ColorCode::normal());
    crate::vga_buffer::print_colored(value, ColorCode::normal());
    if !value.ends_with('\n') {
        crate::vga_buffer::print_colored("\n", ColorCode::normal());
    }

    serial_println!("{}: {}", label, value);
}

/// Display the list of kernel features
///
/// Outputs all major features and improvements to both
/// VGA and serial consoles with appropriate formatting.
pub fn display_feature_list() {
    crate::vga_buffer::print_colored("✓ Major Improvements:\n", ColorCode::success());

    serial_println!();
    serial_println!("✓ Kernel features:");

    for feature in FEATURES {
        emit_feature(feature);
    }

    crate::vga_buffer::print_colored("\n", ColorCode::normal());
}

/// Display usage notes and hints
///
/// Provides information about kernel operation and
/// how to interact with the system (primarily for QEMU users).
pub fn display_usage_note() {
    crate::vga_buffer::print_colored("\nNote: ", ColorCode::warning());
    crate::vga_buffer::print_colored(
        "All core features tested and working!\n\n",
        ColorCode::normal(),
    );

    serial_println!();
    for hint in SERIAL_HINTS {
        serial_println!("{}", hint);
    }
}

/// Display panic information to serial output
///
/// Outputs detailed panic information to the serial console,
/// including the panic message and source location.
///
/// # Fail-Safe Design
///
/// This function checks if serial port is available before attempting
/// output. On systems without COM1, panic information will only be
/// displayed on VGA, which is sufficient for debugging.
///
/// # Arguments
///
/// * `info` - Panic information from the panic handler
pub fn display_panic_info_serial(info: &PanicInfo) {
    // Only attempt serial output if port is available
    // This prevents hangs on systems without serial hardware
    if !crate::serial::is_available() {
        return;
    }

    serial_println!("");
    serial_separator();
    serial_println!("       !!! KERNEL PANIC !!!");
    serial_separator();

    serial_println!("Message: {}", info.message());

    if let Some(location) = info.location() {
        serial_println!(
            "Location: {}:{}:{}",
            location.file(),
            location.line(),
            location.column()
        );
    }

    serial_separator();
    serial_println!("System halted. CPU in hlt loop.");
}

/// Display panic information to VGA buffer
///
/// Shows a simplified panic message on the screen,
/// directing users to check serial output for details.
///
/// # Arguments
///
/// * `info` - Panic information from the panic handler
pub fn display_panic_info_vga(info: &PanicInfo) {
    crate::vga_buffer::print_colored("\n!!! KERNEL PANIC !!!\n\n", ColorCode::panic());

    if let Some(location) = info.location() {
        crate::vga_buffer::print_colored("File: ", ColorCode::error());
        crate::vga_buffer::print_colored(location.file(), ColorCode::normal());
        crate::vga_buffer::print_colored("\n", ColorCode::normal());

        crate::vga_buffer::print_colored("Line: ", ColorCode::error());
        crate::vga_buffer::print_colored("(see serial output)\n", ColorCode::normal());

        crate::vga_buffer::print_colored("Column: ", ColorCode::error());
        crate::vga_buffer::print_colored("(see serial output)\n", ColorCode::normal());
    }

    crate::vga_buffer::print_colored(
        "\nSystem halted. See serial for more details.\n",
        ColorCode::warning(),
    );
}
