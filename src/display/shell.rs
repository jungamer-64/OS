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
use crate::println;

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

const MAX_COMMAND_LEN: usize = 64;

pub struct Shell {
    buffer: [u8; MAX_COMMAND_LEN],
    cursor: usize,
    keyboard: Keyboard,
}

impl Shell {
    pub fn new() -> Self {
        Self {
            buffer: [0; MAX_COMMAND_LEN],
            cursor: 0,
            keyboard: Keyboard::new(),
        }
    }

    pub fn run(&mut self) -> ! {
        show_wait_shell();
        
        loop {
            if let Some(scancode) = self.keyboard.read_scancode() {
                // Key press (bit 7 clear)
                if scancode & 0x80 == 0 {
                    self.handle_key(scancode);
                }
            }
            core::hint::spin_loop();
        }
    }

    fn handle_key(&mut self, scancode: u8) {
        if let Some(c) = scancode_to_char(scancode) {
            match c {
                '\n' => self.process_command(),
                '\x08' => self.handle_backspace(),
                c => self.append_char(c),
            }
        }
    }

    fn append_char(&mut self, c: char) {
        // Safety: Ensure we only accept ASCII characters to prevent truncation issues
        // when casting to u8, and filter out non-printable control characters (except space).
        if !c.is_ascii() || (!c.is_ascii_graphic() && c != ' ') {
            return;
        }

        if self.cursor < MAX_COMMAND_LEN {
            self.buffer[self.cursor] = c as u8;
            self.cursor += 1;
            print!("{}", c);
        }
    }

    fn handle_backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            // Securely clear the character from the buffer
            self.buffer[self.cursor] = 0;
            print!("\x08 \x08");
        }
    }

    fn process_command(&mut self) {
        print!("\n");
        if self.cursor > 0 {
            let cmd = core::str::from_utf8(&self.buffer[..self.cursor])
                .unwrap_or("")
                .trim();
                
            match cmd {
                "help" => self.print_help(),
                "clear" => self.clear_screen(),
                "status" => self.print_status(),
                "version" => println!("TinyOS v0.4.0"),
                "" => {}, // Ignore empty commands
                _ => println!("Unknown command: {}", cmd),
            }
            
            // Safety: Clear the buffer to prevent data residue
            self.buffer.fill(0);
            self.cursor = 0;
        }
        print!("tinyos> ");
    }

    fn print_help(&self) {
        println!("Available commands:");
        println!("  help     - Show this help message");
        println!("  clear    - Clear the screen");
        println!("  status   - Show system status");
        println!("  version  - Show version info");
    }

    fn clear_screen(&self) {
        crate::display::clear_screen();
    }

    fn print_status(&self) {
        let mut out = crate::display::hardware_output();
        render_status(&mut out);
    }
}

/// Run the interactive shell loop
pub fn run_shell() -> ! {
    let mut shell = Shell::new();
    shell.run();
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
