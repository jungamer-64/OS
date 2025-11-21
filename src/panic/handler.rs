// src/panic/handler.rs

//! Central panic handling utilities.
//!
//! The new handler coordinates panic state tracking, diagnostic collection,
//! and multi-channel output with the following guarantees:
//! - Nested panic detection backed by [`panic::state`]
//! - Best-effort serial + VGA output with automatic fallback to the QEMU
//!   debug port (0xE9)
//! - Consistent diagnostic logging so that the kernel panic story can be
//!   reconstructed after reboot
//!
//! The functions in this module are intentionally `no_std` friendly and avoid
//! allocations, locks, or other operations that could panic again while the
//! system is already unhealthy.

use crate::{
    diagnostics::DIAGNOSTICS,
    display,
    init,
    panic::state::{enter_panic, PanicLevel},
    serial,
    vga_buffer,
    println,
};
use core::panic::PanicInfo;
use x86_64::instructions::{interrupts, port::Port};

const DEBUG_PORT: u16 = 0xE9;
const DEBUG_PORT_MAX_PATH: usize = 80;

/// Tracks whether we managed to emit panic information anywhere.
#[derive(Debug, Default, Clone, Copy)]
pub struct PanicOutputStatus {
    /// Serial output succeeded
    pub serial: bool,
    /// VGA output succeeded
    pub vga: bool,
    /// Emergency debug port output was attempted
    pub emergency: bool,
}

impl PanicOutputStatus {
    #[inline]
    #[must_use]
    pub const fn any_success(&self) -> bool {
        self.serial || self.vga || self.emergency
    }
}

struct PanicTelemetry {
    level: PanicLevel,
    init_status: &'static str,
    serial_available: bool,
    vga_accessible: bool,
}

impl PanicTelemetry {
    fn capture(level: PanicLevel) -> Self {
        Self {
            level,
            init_status: init::status_string(),
            serial_available: serial::is_available(),
            vga_accessible: vga_buffer::is_accessible(),
        }
    }
}

/// Entry point invoked by the crate-level `#[panic_handler]`.
pub fn handle_panic(info: &PanicInfo) -> ! {
    // Interrupts can trigger nested panics, so shut them off immediately.
    interrupts::disable();

    let level = enter_panic();
    let telemetry = PanicTelemetry::capture(level);

    DIAGNOSTICS.record_panic();

    match level {
        PanicLevel::Primary => handle_primary_panic(info, &telemetry),
        PanicLevel::Nested => handle_nested_panic(info, &telemetry),
        PanicLevel::Critical => handle_critical_failure(info, &telemetry),
    }
}

fn handle_primary_panic(info: &PanicInfo, telemetry: &PanicTelemetry) -> ! {
    let serial = try_serial_output(info, telemetry);
    let vga = try_vga_output(info, telemetry);
    let emergency = if !serial && !vga {
        emergency_panic_output(info)
    } else {
        false
    };

    let outputs = PanicOutputStatus {
        serial,
        vga,
        emergency,
    };

    finalize(outputs, telemetry)
}

fn handle_nested_panic(info: &PanicInfo, telemetry: &PanicTelemetry) -> ! {
    DIAGNOSTICS.mark_nested_panic();

    if telemetry.serial_available {
        println!(
            "[CRITICAL] Nested panic detected! Using emergency output only."
        );
    }

    let outputs = PanicOutputStatus {
        serial: false,
        vga: false,
        emergency: emergency_output_minimal(info),
    };

    finalize(outputs, telemetry)
}

fn handle_critical_failure(info: &PanicInfo, telemetry: &PanicTelemetry) -> ! {
    DIAGNOSTICS.mark_nested_panic();

    if telemetry.serial_available {
        println!(
            "[FATAL] Multiple nested panics detected. Forcing emergency halt."
        );
    }

    debug_port_emergency_message();

    let outputs = PanicOutputStatus {
        serial: false,
        vga: false,
        emergency: emergency_output_minimal(info),
    };

    finalize(outputs, telemetry)
}

fn try_serial_output(info: &PanicInfo, telemetry: &PanicTelemetry) -> bool {
    if !telemetry.serial_available {
        return false;
    }

    display::display_panic_info_serial(info);
    true
}

fn try_vga_output(info: &PanicInfo, telemetry: &PanicTelemetry) -> bool {
    if !telemetry.vga_accessible {
        return false;
    }

    display::display_panic_info_vga(info);
    true
}

fn finalize(outputs: PanicOutputStatus, telemetry: &PanicTelemetry) -> ! {
    log_system_state(telemetry);
    log_output_summary(outputs, telemetry);

    init::halt_forever()
}

fn log_system_state(telemetry: &PanicTelemetry) {
    if !telemetry.serial_available {
        return;
    }

    println!();
    println!("[STATE] System state at panic:");
    println!("     - Level: {:?}", telemetry.level);
    println!("     - Initialization phase: {}", telemetry.init_status);
    println!("     - VGA accessible: {}", telemetry.vga_accessible);
    println!("     - Serial available: {}", telemetry.serial_available);
    println!();
}

fn log_output_summary(outputs: PanicOutputStatus, telemetry: &PanicTelemetry) {
    if !telemetry.serial_available {
        return;
    }

    println!(
        "[PANIC] Output summary -> serial: {}, vga: {}, emergency_port: {}",
        outputs.serial,
        outputs.vga,
        outputs.emergency
    );
}

fn emergency_panic_output(info: &PanicInfo) -> bool {
    let mut port = Port::<u8>::new(DEBUG_PORT);

    write_bytes(&mut port, b"!!! KERNEL PANIC - OUTPUT FAILED !!!\n");

    if let Some(location) = info.location() {
        write_bytes(&mut port, b"File: ");
        for &byte in location.file().as_bytes().iter().take(DEBUG_PORT_MAX_PATH) {
            write_byte(&mut port, byte);
        }
        write_byte(&mut port, b'\n');

        write_bytes(&mut port, b"Line: ");
        write_decimal_to_port(&mut port, location.line());
        write_byte(&mut port, b'\n');
    }

    true
}

fn emergency_output_minimal(info: &PanicInfo) -> bool {
    let mut port = Port::<u8>::new(DEBUG_PORT);

    write_bytes(&mut port, b"\n!!! NESTED PANIC DETECTED !!!\n");

    if let Some(location) = info.location() {
        write_bytes(&mut port, b"Location: ");
        for &byte in location.file().as_bytes().iter().take(DEBUG_PORT_MAX_PATH) {
            write_byte(&mut port, byte);
        }
        write_byte(&mut port, b':');
        write_decimal_to_port(&mut port, location.line());
        write_byte(&mut port, b'\n');
    }

    write_bytes(
        &mut port,
        b"System halting to prevent corruption.\n",
    );

    true
}

fn debug_port_emergency_message() {
    let mut port = Port::<u8>::new(DEBUG_PORT);

    write_bytes(&mut port, b"\n!!! CRITICAL PANIC FAILURE !!!\n");
    write_bytes(
        &mut port,
        b"Context: Multiple panic attempts detected\n",
    );
    write_bytes(
        &mut port,
        b"Action: Emergency system halt to prevent data corruption\n",
    );
    write_bytes(
        &mut port,
        b"Recommendation: Review panic logs for race conditions\n",
    );
}

fn write_decimal_to_port(port: &mut Port<u8>, mut num: u32) {
    if num == 0 {
        write_byte(port, b'0');
        return;
    }

    let mut digits = [0u8; 10];
    let mut idx = 0;

    while num > 0 && idx < digits.len() {
        digits[idx] = b'0' + (num % 10) as u8;
        num /= 10;
        idx += 1;
    }

    while idx > 0 {
        idx -= 1;
        write_byte(port, digits[idx]);
    }
}

#[inline]
fn write_byte(port: &mut Port<u8>, byte: u8) {
    // SAFETY: Writing to the debug port is safe within the panic handler as it
    // does not rely on any shared resources or locks.
    unsafe {
        port.write(byte);
    }
}

fn write_bytes(port: &mut Port<u8>, bytes: &[u8]) {
    for &byte in bytes {
        write_byte(port, byte);
    }
}

#[cfg(all(test, feature = "std-tests"))]
mod tests {
    use super::PanicOutputStatus;

    #[test]
    fn output_status_success_detection() {
        let mut status = PanicOutputStatus::default();
        assert!(!status.any_success());

        status.serial = true;
        assert!(status.any_success());

        status.serial = false;
        status.vga = true;
        assert!(status.any_success());
    }
}
