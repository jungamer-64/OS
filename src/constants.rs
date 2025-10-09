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
