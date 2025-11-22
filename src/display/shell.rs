// src/display/shell.rs

//! Calm standby shell-style screen rendered after initialization.
//!
//! This module consolidates diagnostics and status information into a
//! single, low-noise view so the system appears stable even while the
//! kernel is idling in the background.

use crate::constants::{FEATURES, SERIAL_HINTS, SYSTEM_INFO};
use crate::diagnostics::DIAGNOSTICS;
use crate::display::output::{broadcast_args_with, hardware_output, Output};
use crate::init;
use crate::serial::timeout_stats;
use crate::vga_buffer::{self, ColorCode};
use crate::arch::Keyboard;
use crate::display::keyboard::scancode_to_char;
use crate::print;

const CPU_CYCLES_PER_MS: u64 = 2_000_000; // Assumes ~2GHz host when running under QEMU
const MAX_FEATURE_LINES: usize = 5;

/// Render the standby shell to the default hardware outputs.
pub fn show_wait_shell() {
    if vga_buffer::is_accessible() {
        let _ = vga_buffer::clear();
        let _ = vga_buffer::set_color(ColorCode::normal());
    }

    let mut out = hardware_output();
    render_shell(&mut out);
}

fn render_shell<O: Output>(out: &mut O) {
    render_header(out);
    render_status(out);
    render_diagnostics(out);
    render_usage(out);
    render_prompt(out);
}

fn render_header<O: Output>(out: &mut O) {
    broadcast_args_with(out, format_args!("\n========================================\n"), ColorCode::info());
    broadcast_args_with(
        out,
        format_args!(" tiny_os interactive shell (standby)\n"),
        ColorCode::info(),
    );
    broadcast_args_with(out, format_args!("========================================\n"), ColorCode::info());
    broadcast_args_with(
        out,
        format_args!(" System stabilized. Waiting for keyboard input...\n\n"),
        ColorCode::normal(),
    );
}

fn render_status<O: Output>(out: &mut O) {
    let detail = init::detailed_status();
    let phase_label = init::status_string();
    let phase_value = detail.phase;
    let vga_status = availability(detail.vga_available);
    let serial_status = availability(detail.serial_available);
    let lock_state = if detail.lock_held { "HELD" } else { "RELEASED" };
    let output_status = availability(detail.has_output());

    broadcast_args_with(out, format_args!("[ system status ]\n"), ColorCode::info());
    broadcast_args_with(
        out,
        format_args!(" phase        : {phase_label} ({phase_value:?})\n"),
        ColorCode::normal(),
    );
    broadcast_args_with(
        out,
        format_args!(" VGA output   : {vga_status}\n"),
        status_color(detail.vga_available),
    );
    broadcast_args_with(
        out,
        format_args!(" Serial port  : {serial_status}\n"),
        status_color(detail.serial_available),
    );
    broadcast_args_with(
        out,
        format_args!(" Kernel lock  : {lock_state}\n"),
        if detail.lock_held {
            ColorCode::warning()
        } else {
            ColorCode::success()
        },
    );
    broadcast_args_with(
        out,
        format_args!(" Output ready : {output_status}\n\n"),
        status_color(detail.has_output()),
    );
}

fn render_diagnostics<O: Output>(out: &mut O) {
    let snapshot = DIAGNOSTICS.snapshot();
    let uptime_ms = cycles_to_ms(snapshot.uptime_cycles);
    let uptime_seconds = uptime_ms / 1000;
    let (timeouts, successes) = timeout_stats();
    let vga_writes = snapshot.vga_writes;
    let vga_failures = snapshot.vga_write_failures;
    let serial_bytes = snapshot.serial_bytes_written;
    let serial_writes = snapshot.serial_writes;
    let serial_timeouts = snapshot.serial_timeouts;
    let panic_count = snapshot.panic_count;
    let nested_panic = snapshot.nested_panic_detected;
    let lock_contentions = snapshot.lock_contentions;
    let max_lock_cycles = snapshot.max_lock_hold_cycles;
    let health_failures = snapshot.health_check_failures;
    let integrity_violations = snapshot.data_integrity_violations;

    broadcast_args_with(out, format_args!("[ diagnostics snapshot ]\n"), ColorCode::info());
    broadcast_args_with(
        out,
        format_args!(" uptime       : {uptime_ms} ms (~{uptime_seconds} s)\n"),
        ColorCode::normal(),
    );
    broadcast_args_with(
        out,
        format_args!(" vga writes   : {vga_writes} (failures: {vga_failures})\n"),
        ColorCode::normal(),
    );
    broadcast_args_with(
        out,
        format_args!(
            " serial bytes : {serial_bytes} (writes: {serial_writes}, timeouts: {serial_timeouts})\n"
        ),
        ColorCode::normal(),
    );
    broadcast_args_with(
        out,
        format_args!(" timeouts     : {timeouts} recorded / {successes} successes\n"),
        ColorCode::normal(),
    );
    broadcast_args_with(
        out,
        format_args!(" panic count  : {panic_count} (nested: {nested_panic})\n"),
        if panic_count == 0 {
            ColorCode::success()
        } else {
            ColorCode::warning()
        },
    );
    broadcast_args_with(
        out,
        format_args!(" lock stats   : {lock_contentions} contentions, max {max_lock_cycles} cycles\n"),
        ColorCode::normal(),
    );
    broadcast_args_with(
        out,
        format_args!(
            " health errs  : {health_failures} failures / {integrity_violations} integrity alerts\n\n"
        ),
        ColorCode::normal(),
    );
}

fn render_usage<O: Output>(out: &mut O) {
    broadcast_args_with(out, format_args!("[ usage hints ]\n"), ColorCode::info());

    for (label, value) in SYSTEM_INFO {
        broadcast_args_with(out, format_args!(" {label:<12}: {value}\n"), ColorCode::normal());
    }

    broadcast_args_with(out, format_args!("\n key features:\n"), ColorCode::info());
    for feature in FEATURES.iter().take(MAX_FEATURE_LINES) {
        broadcast_args_with(out, format_args!("  • {feature}\n"), ColorCode::normal());
    }
    if FEATURES.len() > MAX_FEATURE_LINES {
        broadcast_args_with(
            out,
            format_args!("  • ... ({} more)\n", FEATURES.len() - MAX_FEATURE_LINES),
            ColorCode::normal(),
        );
    }

    broadcast_args_with(out, format_args!("\n serial tips:\n"), ColorCode::info());
    for hint in SERIAL_HINTS {
        broadcast_args_with(out, format_args!("  - {hint}\n"), ColorCode::normal());
    }

    broadcast_args_with(out, format_args!("\n"), ColorCode::normal());
}

fn render_prompt<O: Output>(out: &mut O) {
    broadcast_args_with(out, format_args!("\n"), ColorCode::normal());
    broadcast_args_with(
        out,
        format_args!("tinyos> "),
        ColorCode::info(),
    );
}

/// Run the interactive shell loop
pub fn run_shell() -> ! {
    show_wait_shell();
    
    let mut keyboard = Keyboard::new();
    
    loop {
        if let Some(scancode) = keyboard.read_scancode() {
            // Key press (bit 7 clear)
            if scancode & 0x80 == 0 {
                if let Some(c) = scancode_to_char(scancode) {
                    print!("{}", c);
                    if c == '\n' {
                        print!("tinyos> ");
                    }
                }
            }
        }
        core::hint::spin_loop();
    }
}

const fn availability(flag: bool) -> &'static str {
    if flag {
        "ONLINE"
    } else {
        "OFFLINE"
    }
}

const fn status_color(flag: bool) -> ColorCode {
    if flag {
        ColorCode::success()
    } else {
        ColorCode::warning()
    }
}

const fn cycles_to_ms(cycles: u64) -> u64 {
    if CPU_CYCLES_PER_MS == 0 {
        return 0;
    }

    cycles / CPU_CYCLES_PER_MS
}
