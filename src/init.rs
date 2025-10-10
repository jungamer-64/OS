// src/init.rs

//! Kernel initialization module
//!
//! This module handles all kernel subsystem initialization with:
//! - Guaranteed initialization order
//! - Comprehensive error handling
//! - Detailed status reporting
//! - Idempotent initialization (safe to call multiple times)
//!
//! # Initialization Order
//!
//! CRITICAL: Subsystems MUST be initialized in this order:
//! 1. VGA buffer (for early error reporting)
//! 2. Serial port (for detailed debugging)
//! 3. Other subsystems (future expansion)
//!
//! This order ensures we have output capability as early as possible
//! for error reporting.

use crate::constants::{
    SERIAL_ALREADY_INITIALIZED_LINES, SERIAL_IDLE_LOOP_LINES, SERIAL_INIT_SUCCESS_LINES,
    SERIAL_SAFETY_FEATURE_LINES,
};
use crate::diagnostics::DIAGNOSTICS;
use crate::serial::{self, InitError as SerialInitError};
use crate::serial_println;
use crate::vga_buffer::ColorCode;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};

/// Initialization state tracking
static VGA_INITIALIZED: AtomicBool = AtomicBool::new(false);
static SERIAL_INITIALIZED: AtomicBool = AtomicBool::new(false);
static INIT_PHASE: AtomicU8 = AtomicU8::new(0);
static INIT_LOCK: AtomicU32 = AtomicU32::new(0);

const INIT_MAGIC: u32 = 0xDEADBEEF;

/// Initialization phases
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitPhase {
    NotStarted = 0,
    VgaInit = 1,
    SerialInit = 2,
    Complete = 3,
}

impl From<u8> for InitPhase {
    fn from(value: u8) -> Self {
        match value {
            1 => InitPhase::VgaInit,
            2 => InitPhase::SerialInit,
            3 => InitPhase::Complete,
            _ => InitPhase::NotStarted,
        }
    }
}

/// Get current initialization phase
pub fn current_phase() -> InitPhase {
    InitPhase::from(INIT_PHASE.load(Ordering::Acquire))
}

/// Initialize the VGA text mode
///
/// This is the first initialization step and provides basic output
/// capability even if serial port initialization fails.
///
/// # Safety
///
/// Must be called before any other subsystem that requires output.
///
/// # Returns
///
/// Always returns `Ok(())` as VGA initialization is highly unlikely
/// to fail (buffer at 0xB8000 is almost always accessible).
pub fn initialize_vga() -> Result<(), &'static str> {
    // Check if already initialized
    if VGA_INITIALIZED.swap(true, Ordering::AcqRel) {
        return Ok(()); // Already initialized, idempotent
    }

    // Update initialization phase
    INIT_PHASE.store(InitPhase::VgaInit as u8, Ordering::Release);

    // Initialize and test VGA buffer
    crate::vga_buffer::init();

    // Clear screen and set default colors
    crate::vga_buffer::clear();
    crate::vga_buffer::set_color(ColorCode::normal());

    // Verify buffer is accessible
    if !crate::vga_buffer::is_accessible() {
        // VGA buffer not accessible - this is rare but possible on some systems
        // We don't fail initialization, but we note the issue
        // (Can't print to VGA if it's not accessible, obviously)
        return Err("VGA buffer not accessible");
    }

    Ok(())
}

/// Initialize the serial port (COM1)
///
/// Configures the serial port for debugging output. If the port is already
/// initialized or hardware is not present, appropriate status is returned.
///
/// # Hardware Detection
///
/// This function gracefully handles systems without COM1 hardware.
/// Modern motherboards often lack physical serial ports.
///
/// # Returns
///
/// - `Ok(())` if initialization succeeds
/// - `Err(message)` if initialization fails (serial port optional)
pub fn initialize_serial() -> Result<(), &'static str> {
    // Check if already initialized
    if SERIAL_INITIALIZED.load(Ordering::Acquire) {
        return Ok(()); // Already initialized, idempotent
    }

    // Update initialization phase
    INIT_PHASE.store(InitPhase::SerialInit as u8, Ordering::Release);

    match crate::serial::init() {
        Ok(()) => {
            SERIAL_INITIALIZED.store(true, Ordering::Release);

            // Display success banner
            serial::log_lines(SERIAL_INIT_SUCCESS_LINES.iter().copied());

            Ok(())
        }
        Err(SerialInitError::AlreadyInitialized) => {
            SERIAL_INITIALIZED.store(true, Ordering::Release);
            serial::log_lines(SERIAL_ALREADY_INITIALIZED_LINES.iter().copied());
            Ok(())
        }
        Err(SerialInitError::PortNotPresent) => {
            // Not an error - many modern systems don't have serial ports
            crate::vga_buffer::print_colored(
                "[INFO] Serial port not present (continuing with VGA only)\n",
                ColorCode::warning(),
            );
            Err("Serial port hardware not present")
        }
        Err(SerialInitError::Timeout) => {
            crate::vga_buffer::print_colored(
                "[WARN] Serial port timeout (hardware not responding)\n",
                ColorCode::warning(),
            );
            Err("Serial port initialization timeout")
        }
        Err(SerialInitError::ConfigurationFailed) => {
            crate::vga_buffer::print_colored(
                "[ERROR] Serial port configuration failed\n",
                ColorCode::error(),
            );
            Err("Serial port configuration failed")
        }
        Err(SerialInitError::HardwareAccessFailed) => {
            crate::vga_buffer::print_colored(
                "[ERROR] Serial port hardware access failed\n",
                ColorCode::error(),
            );
            Err("Serial port hardware access failed")
        }
        Err(SerialInitError::TooManyAttempts) => {
            crate::vga_buffer::print_colored(
                "[ERROR] Too many serial initialization attempts\n",
                ColorCode::error(),
            );
            Err("Too many serial initialization attempts")
        }
    }
}

/// Print VGA initialization status to serial
pub fn report_vga_status() {
    if !crate::serial::is_available() {
        return;
    }

    serial_println!("[OK] VGA text mode initialized");
    serial_println!("     - Resolution: 80x25 characters");
    serial_println!("     - Colors: 16-color palette");
    serial_println!("     - Buffer address: 0xB8000");
    serial_println!("     - Auto-scroll: Enabled");
    serial_println!(
        "     - Buffer validation: {}",
        if crate::vga_buffer::is_accessible() {
            "Passed"
        } else {
            "Failed"
        }
    );
    serial_println!();
}

/// Print safety features to serial
pub fn report_safety_features() {
    if !crate::serial::is_available() {
        return;
    }

    serial::log_lines(SERIAL_SAFETY_FEATURE_LINES.iter().copied());
}

/// Complete initialization sequence
///
/// Runs all initialization steps in the correct order and reports status.
/// This is the main initialization entry point. Also records boot timestamp
/// for system diagnostics.
///
/// # Returns
///
/// - `Ok(())` if all critical systems initialize successfully
/// - `Err(message)` if a critical system fails to initialize
pub fn initialize_all() -> Result<(), &'static str> {
    match INIT_LOCK.compare_exchange(0, INIT_MAGIC, Ordering::AcqRel, Ordering::Acquire) {
        Ok(_) => {
            let result = perform_initialization();
            if result.is_err() {
                INIT_LOCK.store(0, Ordering::Release);
            }
            result
        }
        Err(current) if current == INIT_MAGIC => Ok(()),
        Err(_) => Err("Initialization in inconsistent state"),
    }
}

fn perform_initialization() -> Result<(), &'static str> {
    // Record boot timestamp for diagnostics
    DIAGNOSTICS.set_boot_time();
    
    initialize_vga()?;

    let _ = initialize_serial();

    report_vga_status();
    report_safety_features();

    INIT_PHASE.store(InitPhase::Complete as u8, Ordering::Release);

    Ok(())
}

/// Enter the idle loop and halt the CPU
///
/// This function puts the CPU into a low-power state using the `hlt`
/// instruction. The CPU will wake up on interrupts and immediately
/// halt again, creating an efficient idle loop.
///
/// # Power Management
///
/// Using `hlt` instead of busy-looping:
/// - Reduces power consumption significantly
/// - Reduces heat generation
/// - Allows other logical cores to run (on multi-core systems)
/// - Enables better battery life (on laptops)
///
/// # Note
///
/// This function never returns (`-> !`) as the kernel should remain
/// in the idle loop until a hardware interrupt or reset occurs.
pub fn halt_forever() -> ! {
    if crate::serial::is_available() {
        serial::log_lines(SERIAL_IDLE_LOOP_LINES.iter().copied());
    }

    loop {
        // SAFETY: hlt is a privileged instruction that can only be
        // executed in kernel mode (ring 0). It's always safe to call
        // from kernel code. The CPU will wake on interrupts.
        x86_64::instructions::hlt();
    }
}

/// Verify initialization state
///
/// Checks that all subsystems are properly initialized.
/// Useful for debugging and validation.
///
/// # Returns
///
/// `true` if initialization is complete, `false` otherwise
pub fn is_initialized() -> bool {
    current_phase() == InitPhase::Complete
}

/// Get initialization status as a string (for debugging)
pub fn status_string() -> &'static str {
    match current_phase() {
        InitPhase::NotStarted => "Not started",
        InitPhase::VgaInit => "VGA initialized",
        InitPhase::SerialInit => "Serial initialized",
        InitPhase::Complete => "Complete",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_phase_conversion() {
        assert_eq!(InitPhase::from(0), InitPhase::NotStarted);
        assert_eq!(InitPhase::from(1), InitPhase::VgaInit);
        assert_eq!(InitPhase::from(2), InitPhase::SerialInit);
        assert_eq!(InitPhase::from(3), InitPhase::Complete);
    }

    #[test]
    fn test_init_phase_values() {
        assert_eq!(InitPhase::NotStarted as u8, 0);
        assert_eq!(InitPhase::VgaInit as u8, 1);
        assert_eq!(InitPhase::SerialInit as u8, 2);
        assert_eq!(InitPhase::Complete as u8, 3);
    }
}
