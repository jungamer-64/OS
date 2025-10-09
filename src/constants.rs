// src/constants.rs

//! Kernel constants and configuration values
//!
//! This module centralizes all constant values used throughout the kernel,
//! including feature descriptions, system information, and UI messages.

/// List of major kernel features and improvements
///
/// These features are displayed during boot to inform the user
/// about the kernel's capabilities.
pub const FEATURES: &[&str] = &[
    "Replaced static mut with Mutex (SAFE!)",
    "Interrupt-safe locking (no deadlock!)",
    "Implemented fmt::Write trait",
    "Optimized scroll with copy_nonoverlapping",
    "Modular code structure (vga_buffer, serial)",
    "Serial FIFO transmit check",
    "VGA color support (16 colors)",
    "VGA auto-scroll",
    "CPU hlt instruction",
    "Detailed panic handler",
];

/// System component information
///
/// Each tuple contains a (label, value) pair describing
/// a kernel component or configuration.
pub const SYSTEM_INFO: &[(&str, &str)] = &[
    ("Bootloader", "0.9.33"),
    ("Serial", "COM1 (0x3F8) with FIFO check"),
];

/// Usage hints displayed to serial output
///
/// These messages provide guidance on interacting with
/// the kernel when running under QEMU or similar emulators.
pub const SERIAL_HINTS: &[&str] = &[
    "Kernel running. System in low-power hlt loop.",
    "Press Ctrl+A, X to exit QEMU.",
];

// Hardware-related constants (shared across modules)
// Serial (COM1)
pub const SERIAL_IO_PORT: u16 = 0x3F8;
pub const BAUD_RATE_DIVISOR: u16 = 3; // 115200 / 38400
pub const TIMEOUT_ITERATIONS: u32 = 10_000_000;
pub const FIFO_ENABLE_CLEAR: u8 = 0xC7;
pub const MODEM_CTRL_ENABLE_IRQ_RTS_DSR: u8 = 0x0B;
pub const DLAB_ENABLE: u8 = 0x80;
pub const CONFIG_8N1: u8 = 0x03;
pub const LSR_TRANSMIT_EMPTY: u8 = 0x20;
pub const SCRATCH_TEST_PRIMARY: u8 = 0xAA;
pub const SCRATCH_TEST_SECONDARY: u8 = 0x55;
