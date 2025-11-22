// src/display/boot.rs

//! Boot information display module
//!
//! Handles display of boot-time information including:
//! - Boot environment details
//! - System information
//! - Feature list
//! - Usage instructions

use super::backend::{default_display_backend, DisplayHardware};
use super::output::{broadcast_args_with, broadcast_with, hardware_output, Output};
use crate::constants::{FEATURES, SERIAL_HINTS, SYSTEM_INFO};
use crate::vga_buffer::ColorCode;
use bootloader_api::BootInfo;

#[cfg(debug_assertions)]
fn usage_note_trace(label: &str) {
    if crate::serial::is_available() {
        crate::serial_println!("[TRACE] usage note: {}", label);
    }
}

#[cfg(not(debug_assertions))]
fn usage_note_trace(_label: &str) {}

#[cfg(debug_assertions)]
fn status_trace(label: &str) {
    if crate::serial::is_available() {
        crate::serial_println!("[TRACE] status block: {}", label);
    }
}

#[cfg(not(debug_assertions))]
fn status_trace(_label: &str) {}

/// Display boot environment information using hardware outputs
///
/// # Arguments
///
/// * `boot_info` - Boot information from the bootloader
pub fn display_boot_environment(boot_info: &BootInfo) {
    let mut out = hardware_output();
    display_boot_environment_with(&mut out, boot_info);
}

/// Display boot environment information to a specific output
///
/// Shows information about the boot environment including:
/// - Display mode (VGA text mode)
/// - Serial port availability
/// - Boot mode requirements (BIOS/CSM)
///
/// # Arguments
///
/// * `out` - The output target
/// * `boot_info` - Boot information from the bootloader
pub fn display_boot_environment_with<O: Output>(out: &mut O, _boot_info: &BootInfo) {
    broadcast_args_with(
        out,
        format_args!("\n--- Boot Environment ---\n"),
        ColorCode::info(),
    );

    // Display mode information
    broadcast_args_with(
        out,
        format_args!("Display: VGA Text Mode (0xB8000)\n"),
        ColorCode::success(),
    );

    // Serial port status with detailed checking
    display_serial_status(out);

    // Boot mode information
    display_boot_mode_info(out);

    broadcast_args_with(
        out,
        format_args!("------------------------\n\n"),
        ColorCode::info(),
    );
}

/// Display serial port status
///
/// Shows whether the serial port is available and provides
/// appropriate status messages.
fn display_serial_status<O: Output>(out: &mut O) {
    let (status_msg, status_color) = if crate::serial::is_available() {
        ("[OK] COM1 available and initialized", ColorCode::success())
    } else if crate::serial::is_initialized() {
        (
            "[WARN] COM1 initialized but not responding",
            ColorCode::warning(),
        )
    } else {
        ("[INFO] COM1 not present", ColorCode::normal())
    };

    broadcast_args_with(out, format_args!("Serial: {status_msg}\n"), status_color);
}

/// Display boot mode information and requirements
///
/// Informs the user about BIOS text mode requirements and
/// provides tips for UEFI systems.
fn display_boot_mode_info<O: Output>(out: &mut O) {
    broadcast_args_with(
        out,
        format_args!("Note: BIOS text mode or CSM is required.\n"),
        ColorCode::warning(),
    );

    broadcast_args_with(
        out,
        format_args!("Note: VGA memory is assumed at 0xB8000.\n"),
        ColorCode::normal(),
    );

    broadcast_args_with(
        out,
        format_args!("Tip: Enable CSM in firmware on UEFI systems.\n"),
        ColorCode::normal(),
    );
}

/// Display boot information using hardware outputs
pub fn display_boot_information() {
    let mut out = hardware_output();
    display_boot_information_with(&mut out);
}

/// Display boot information to a specific output
///
/// Shows the kernel banner and system information.
///
/// # Arguments
///
/// * `out` - The output target
pub fn display_boot_information_with<O: Output>(out: &mut O) {
    // Banner
    broadcast_args_with(
        out,
        format_args!("=== Rust OS Kernel Started ===\n\n"),
        ColorCode::info(),
    );

    broadcast_args_with(
        out,
        format_args!("Welcome to minimal x86_64 Rust OS!\n\n"),
        ColorCode::normal(),
    );

    // System information
    display_system_info_table(out);
}

/// Display system information table
///
/// Shows key system component information in a formatted table.
fn display_system_info_table<O: Output>(out: &mut O) {
    broadcast_args_with(
        out,
        format_args!("[System Information]\n"),
        ColorCode::info(),
    );

    for &(label, value) in SYSTEM_INFO {
        display_system_info_with(out, label, value);
    }

    broadcast_args_with(out, format_args!("\n"), ColorCode::normal());
}

/// Display a single system information entry
///
/// # Arguments
///
/// * `out` - The output target
/// * `label` - The information label
/// * `value` - The information value
fn display_system_info_with<O: Output>(out: &mut O, label: &str, value: &str) {
    broadcast_args_with(
        out,
        format_args!("  {label:12}: {value}\n"),
        ColorCode::normal(),
    );
}

/// Display feature list using hardware outputs
pub fn display_feature_list() {
    let mut out = hardware_output();
    display_feature_list_with(&mut out);
}

/// Display feature list to a specific output
///
/// Shows all major kernel features and improvements.
///
/// # Arguments
///
/// * `out` - The output target
pub fn display_feature_list_with<O: Output>(out: &mut O) {
    broadcast_args_with(
        out,
        format_args!("[Kernel Features]\n"),
        ColorCode::success(),
    );

    for (idx, feature) in FEATURES.iter().enumerate() {
        emit_feature_with(out, idx + 1, feature);
    }

    broadcast_with(out, "\n", ColorCode::normal());
}

/// Display a single feature entry
///
/// # Arguments
///
/// * `out` - The output target
/// * `num` - Feature number (1-indexed)
/// * `feature` - Feature description
fn emit_feature_with<O: Output>(out: &mut O, num: usize, feature: &str) {
    broadcast_args_with(
        out,
        format_args!("  {num:2}. {feature}\n"),
        ColorCode::normal(),
    );
}

/// Display usage note using hardware outputs
pub fn display_usage_note() {
    let mut out = hardware_output();
    display_usage_note_with(&mut out);
}

/// Display usage note to a specific output
///
/// Shows usage instructions and hints for interacting with the kernel.
///
/// # Arguments
///
/// * `out` - The output target
pub fn display_usage_note_with<O: Output>(out: &mut O) {
    usage_note_trace("start");
    broadcast_args_with(out, format_args!("[Status]\n"), ColorCode::info());

    // System status
    display_system_status(out);
    usage_note_trace("after status");

    broadcast_args_with(out, format_args!("\n"), ColorCode::normal());

    // Usage hints
    broadcast_args_with(out, format_args!("[Usage Hints]\n"), ColorCode::info());

    for hint in SERIAL_HINTS {
        broadcast_args_with(out, format_args!("  • {hint}\n"), ColorCode::normal());
    }

    broadcast_args_with(out, format_args!("\n"), ColorCode::normal());
    usage_note_trace("end");
}

/// Display current system status
///
/// Shows the operational status of each subsystem.
fn display_system_status<O: Output>(out: &mut O) {
    status_trace("enter");
    let display = default_display_backend();
    let vga_status = if display.is_available() {
        ("VGA", "Operational", ColorCode::success())
    } else {
        ("VGA", "Not accessible", ColorCode::error())
    };
    status_trace("after vga state");

    let serial_status = if crate::serial::is_available() {
        ("Serial", "Operational", ColorCode::success())
    } else {
        ("Serial", "Not available", ColorCode::warning())
    };
    status_trace("after serial state");

    let init_status = if crate::init::is_initialized() {
        ("Init", "Complete", ColorCode::success())
    } else {
        ("Init", "In progress", ColorCode::warning())
    };
    status_trace("after init state");

    let (vga_label, vga_value, vga_color) = vga_status;
    status_trace("before vga write");
    broadcast_args_with(out, format_args!("  {vga_label:10}: {vga_value}\n"), vga_color);

    let (serial_label, serial_value, serial_color) = serial_status;
    status_trace("before serial write");
    broadcast_args_with(
        out,
        format_args!("  {serial_label:10}: {serial_value}\n"),
        serial_color,
    );

    let (init_label, init_value, init_color) = init_status;
    status_trace("before init write");
    broadcast_args_with(
        out,
        format_args!("  {init_label:10}: {init_value}\n"),
        init_color,
    );

    status_trace("exit");
}

// NOTE: Unit tests removed as they require std library features (Vec, String, format!)
// that are not available in this no_std environment.
// Integration tests should be used instead for testing this functionality.
