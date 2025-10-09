// src/init.rs

//! Kernel initialization module
//!
//! This module handles all kernel subsystem initialization including:
//! - Serial port (COM1) setup and configuration
//! - VGA text mode initialization
//! - Hardware verification
//!
//! Initialization functions are called early in the kernel boot process
//! to prepare the system for operation.

use crate::serial::InitError;
use crate::serial_println;
use crate::vga_buffer::ColorCode;

/// Initialize the serial port (COM1)
///
/// Configures the serial port for debugging output. If the port is already
/// initialized (e.g., by bootloader or previous initialization), this function
/// will skip hardware setup and only log a message.
///
/// # Hardware Detection
///
/// This function gracefully handles systems without COM1 hardware.
/// On modern motherboards without physical serial ports, the kernel
/// will continue to function normally using only VGA output.
///
/// # Examples
///
/// ```
/// use crate::init::initialize_serial;
///
/// initialize_serial();
/// ```
pub fn initialize_serial() {
    match crate::serial::init() {
        Ok(()) => {
            serial_println!("=== Rust OS Kernel Started ===");
            serial_println!("Serial port initialized (38400 baud, 8N1, FIFO checked)");
        }
        Err(InitError::AlreadyInitialized) => {
            serial_println!("Serial port already initialized; skipping hardware setup");
        }
        Err(InitError::PortNotPresent) => {
            // No serial port - this is normal on modern systems
            // VGA output will still work, so no action needed
            // We intentionally don't panic here
        }
        Err(InitError::Timeout) => {
            // Port exists but not responding
            // Continue anyway - not critical for kernel operation
        }
    }

    debug_assert!(crate::serial::is_initialized());
}

/// Initialize the VGA text mode
///
/// Clears the screen, sets the default color scheme, and prepares
/// the VGA buffer for output. Also logs initialization status to
/// the serial console.
///
/// # Examples
///
/// ```
/// use crate::init::initialize_vga;
///
/// initialize_vga();
/// ```
pub fn initialize_vga() {
    crate::vga_buffer::clear();
    crate::vga_buffer::set_color(ColorCode::normal());
    serial_println!("VGA text mode initialized (80x25, color support)");
    serial_println!("SAFE: Using Mutex-protected VGA writer (interrupt-safe!)");
}

/// Enter the idle loop and halt the CPU
///
/// This function puts the CPU into a low-power state using the `hlt`
/// instruction. The CPU will wake up on interrupts and immediately
/// halt again, creating an efficient idle loop.
///
/// # Note
///
/// This function never returns (`-> !`) as the kernel should remain
/// in the idle loop until a hardware interrupt or reset occurs.
///
/// # Examples
///
/// ```
/// use crate::init::halt_forever;
///
/// // After kernel initialization
/// halt_forever();
/// ```
pub fn halt_forever() -> ! {
    loop {
        x86_64::instructions::hlt();
    }
}
