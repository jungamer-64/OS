// src/main.rs

//! Minimal x86_64 Rust Operating System Kernel
//!
//! This is a bare-metal OS kernel that runs directly on x86_64 hardware
//! without requiring a host operating system. It provides:
//!
//! - VGA text mode output with 16-color support
//! - Serial port (COM1) communication for debugging
//! - Safe, interrupt-protected I/O operations
//! - Panic handler with detailed error reporting
//!
//! # Architecture
//!
//! The kernel is organized into several modules:
//! - `constants`: Configuration values and messages
//! - `display`: Output formatting and presentation
//! - `init`: Hardware initialization routines
//! - `serial`: COM1 serial port driver
//! - `vga_buffer`: VGA text mode driver
//!
//! # Safety
//!
//! This kernel uses `no_std` and `no_main` attributes to run in a
//! freestanding environment. All I/O operations are protected by
//! interrupt-safe Mutex wrappers to prevent data races.

#![no_std]
#![no_main]

mod constants;
mod display;
mod init;
mod serial;
mod vga_buffer;

use bootloader::{entry_point, BootInfo};
use core::panic::PanicInfo;

entry_point!(kernel_main);

/// Kernel entry point
///
/// This function is called by the bootloader after basic hardware
/// initialization. It sets up kernel subsystems, displays boot
/// information, and enters the idle loop.
///
/// # Arguments
///
/// * `boot_info` - Boot information from the bootloader (contains framebuffer info, memory map, etc.)
///
/// # Returns
///
/// This function never returns (`-> !`). The kernel runs indefinitely
/// in a low-power idle loop until reset or power-off.
fn kernel_main(boot_info: &'static BootInfo) -> ! {
    // Initialize hardware subsystems
    init::initialize_serial();
    init::initialize_vga();

    // Display boot environment information
    display::display_boot_environment(boot_info);

    // Display boot information and feature list
    display::display_boot_information();
    display::display_feature_list();
    display::display_usage_note();

    // Enter low-power idle loop
    init::halt_forever()
}

/// Kernel panic handler
///
/// This function is called when a panic occurs. It displays detailed
/// error information to both VGA and serial outputs, then halts the CPU.
///
/// # Arguments
///
/// * `info` - Information about the panic, including message and location
///
/// # Returns
///
/// This function never returns (`-> !`). The system halts permanently
/// after displaying the panic information.
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    display::display_panic_info_serial(info);
    display::display_panic_info_vga(info);

    init::halt_forever()
}
