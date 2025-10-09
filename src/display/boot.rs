use super::core::{broadcast_args_with, broadcast_with, hardware_output, Output};
use crate::constants::{FEATURES, SERIAL_HINTS, SYSTEM_INFO};
use crate::vga_buffer::ColorCode;
use bootloader::BootInfo;

pub fn display_boot_environment(boot_info: &'static BootInfo) {
    let mut out = hardware_output();
    display_boot_environment_with(&mut out, boot_info);
}

pub fn display_boot_environment_with<O: Output>(out: &mut O, _boot_info: &'static BootInfo) {
    broadcast_args_with(
        out,
        format_args!("\n--- Boot Environment ---\n"),
        ColorCode::info(),
    );

    broadcast_args_with(
        out,
        format_args!("Display: VGA Text Mode (0xB8000)\n"),
        ColorCode::success(),
    );

    let serial_status = if crate::serial::is_available() {
        ("[OK] COM1 available", ColorCode::success())
    } else {
        ("[WARN] COM1 not present", ColorCode::error())
    };
    broadcast_args_with(
        out,
        format_args!("Serial: {}\n", serial_status.0),
        serial_status.1,
    );

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

    broadcast_args_with(
        out,
        format_args!("------------------------\n\n"),
        ColorCode::info(),
    );
}

pub fn display_boot_information() {
    let mut out = hardware_output();
    display_boot_information_with(&mut out);
}

pub fn display_boot_information_with<O: Output>(out: &mut O) {
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

    for &(label, value) in SYSTEM_INFO {
        display_system_info_with(out, label, value);
    }
}

fn display_system_info_with<O: Output>(out: &mut O, label: &str, value: &str) {
    broadcast_args_with(
        out,
        format_args!("{}: {}\n", label, value),
        ColorCode::normal(),
    );
}

pub fn display_feature_list() {
    let mut out = hardware_output();
    display_feature_list_with(&mut out);
}

pub fn display_feature_list_with<O: Output>(out: &mut O) {
    broadcast_args_with(
        out,
        format_args!("[OK] Major Improvements:\n"),
        ColorCode::success(),
    );

    for feature in FEATURES {
        emit_feature_with(out, feature);
    }

    broadcast_with(out, "\n", ColorCode::normal());
}

fn emit_feature_with<O: Output>(out: &mut O, feature: &str) {
    broadcast_args_with(out, format_args!("- {}\n", feature), ColorCode::normal());
}

pub fn display_usage_note() {
    let mut out = hardware_output();
    display_usage_note_with(&mut out);
}

pub fn display_usage_note_with<O: Output>(out: &mut O) {
    broadcast_args_with(out, format_args!("\nNote: "), ColorCode::warning());
    broadcast_args_with(
        out,
        format_args!("All core features tested and working!\n\n"),
        ColorCode::normal(),
    );

    for hint in SERIAL_HINTS {
        broadcast_args_with(out, format_args!("{}\n", hint), ColorCode::normal());
    }
}
