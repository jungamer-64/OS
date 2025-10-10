// src/main.rs

//! Minimal x86_64 Rust Operating System Kernel
//!
//! This is a bare-metal OS kernel that runs directly on x86_64 hardware
//! without requiring a host operating system. It provides:
//!
//! - VGA text mode output with 16-color support
//! - Serial port (COM1) communication for debugging
//! - Safe, interrupt-protected I/O operations
//! - Comprehensive panic handler with detailed error reporting
//! - Robust initialization with error recovery
//!
//! # Architecture
//!
//! The kernel is organized into several modules:
//! - `constants`: Configuration values and messages
//! - `display`: Output formatting and presentation
//! - `init`: Hardware initialization routines with error handling
//! - `serial`: COM1 serial port driver with timeout protection
//! - `vga_buffer`: VGA text mode driver with bounds checking
//!
//! # Safety and Robustness
//!
//! This kernel implements multiple safety layers:
//! - Mutex-based synchronization for all I/O
//! - Interrupt-safe critical sections
//! - Boundary validation on all buffer operations
//! - Hardware detection before use
//! - Timeout protection on blocking operations
//! - Idempotent initialization
//!
//! # Error Handling Philosophy
//!
//! The kernel follows a "fail gracefully" approach:
//! - Critical failures (VGA init) cause panic with detailed info
//! - Non-critical failures (serial port) log warnings but continue
//! - All panics provide maximum debug information
//! - System attempts to provide output even in failure cases

#![no_std]
#![no_main]
// Enable additional safety features
#![deny(unsafe_op_in_unsafe_fn)]
#![warn(missing_docs)]
// Allow missing docs for entry_point macro
#![allow(missing_docs)]

use tiny_os::constants::SERIAL_NON_CRITICAL_CONTINUATION_LINES;
use tiny_os::{diagnostics, display, init, serial, vga_buffer};
use tiny_os::{print, println, serial_println};

use bootloader::{entry_point, BootInfo};
use core::panic::PanicInfo;

const SERIAL_KERNEL_INIT_SUCCESS_LINES: &[&str] =
    &["[OK] All kernel subsystems initialized successfully", ""];

entry_point!(kernel_main);

/// Kernel entry point
///
/// This function is called by the bootloader after basic hardware
/// initialization. It sets up kernel subsystems, displays boot
/// information, and enters the idle loop.
///
/// # Initialization Sequence
///
/// 1. Initialize VGA buffer (required for output)
/// 2. Initialize serial port (optional, for debugging)
/// 3. Display boot environment information
/// 4. Display feature list and usage notes
/// 5. Enter low-power idle loop
///
/// # Error Handling
///
/// - VGA initialization failure causes panic (no output capability)
/// - All other errors are logged and handled gracefully
///
/// # Arguments
///
/// * `boot_info` - Boot information from the bootloader including:
///   - Memory map
///   - Framebuffer information (if available)
///   - RSDP address (for ACPI)
///   - Physical memory offset
///
/// # Returns
///
/// This function never returns (`-> !`). The kernel runs indefinitely
/// in a low-power idle loop until reset or power-off.
fn kernel_main(boot_info: &'static BootInfo) -> ! {
    // Phase 1: Initialize all subsystems
    // This is the most critical phase - if VGA fails, we can't show errors
    match init::initialize_all() {
        Ok(()) => {
            // Initialization successful
            if serial::is_available() {
                serial::log_lines(SERIAL_KERNEL_INIT_SUCCESS_LINES.iter().copied());
            }
        }
        Err(e) => {
            if matches!(e, init::InitError::VgaFailed(_)) {
                panic!("Critical: VGA initialization failed - no output capability");
            }

            let vga_accessible = vga_buffer::is_accessible();
            let serial_available = serial::is_available();

            // Log the failure through any remaining channels
            if vga_accessible {
                if let Err(err) = vga_buffer::print_colored(
                    "[CRITICAL] Initialization failed\n",
                    vga_buffer::ColorCode::error(),
                ) {
                    log_vga_failure("init failure banner", err);
                }
            }
            if serial_available {
                serial_println!("[CRITICAL] Initialization failed: {:?}", e);
                serial_println!(
                    "[WARN] Non-critical failure encountered. Continuing boot sequence."
                );
                serial::log_lines(SERIAL_NON_CRITICAL_CONTINUATION_LINES.iter().copied());
            }
        }
    }

    // Phase 2: Display boot environment
    display::display_boot_environment(boot_info);

    // Phase 3: Display boot information and features
    display::display_boot_information();
    display::display_feature_list();
    display::display_usage_note();

    // Phase 4: Final system check
    perform_system_check();

    // Phase 5: Display system health report
    diagnostics::print_health_report();

    // Phase 6: Enter low-power idle loop
    // This never returns
    init::halt_forever()
}

/// Perform final system checks before entering idle loop
///
/// This function validates that critical systems are functioning
/// and logs any warnings about degraded functionality.
fn perform_system_check() {
    let status = init::detailed_status();
    let vga_ok = status.vga_available;
    let serial_ok = status.serial_available;
    let init_ok = matches!(status.phase, init::InitPhase::Complete);
    let output_ok = status.has_output();

    // Log system status to serial if available
    if serial_ok {
        serial_println!("[CHECK] Final system check:");
        serial_println!("     - VGA buffer: {}", ok_failed(vga_ok));
        serial_println!("     - Serial port: {}", availability_label(serial_ok));
        serial_println!("     - Initialization phase: {:?}", status.phase);
        serial_println!(
            "     - Output capability: {}",
            availability_label(output_ok)
        );
        serial_println!();
    }

    // Display warnings on VGA for any issues
    if !vga_ok {
        return;
    }

    if !serial_ok {
        if let Err(err) = vga_buffer::print_colored(
            "[WARN] Serial port not available - debugging limited\n",
            vga_buffer::ColorCode::warning(),
        ) {
            log_vga_failure("system check serial warning", err);
        }
    }

    if !init_ok {
        if let Err(err) = vga_buffer::set_color(vga_buffer::ColorCode::warning()) {
            log_vga_failure("system check warning color", err);
        }
        print!("[WARN] Core initialization incomplete: ");
        println!("{}", init::status_string());
        if let Err(err) = vga_buffer::set_color(vga_buffer::ColorCode::normal()) {
            log_vga_failure("system check reset color", err);
        }
    }

    if serial_ok && init_ok {
        if let Err(err) = vga_buffer::print_colored(
            "[OK] All core systems operational\n\n",
            vga_buffer::ColorCode::success(),
        ) {
            log_vga_failure("system check success banner", err);
        }
    }
}

fn ok_failed(ok: bool) -> &'static str {
    if ok {
        "OK"
    } else {
        "FAILED"
    }
}

fn availability_label(ok: bool) -> &'static str {
    if ok {
        "Available"
    } else {
        "Unavailable"
    }
}

fn log_vga_failure(context: &str, err: vga_buffer::VgaError) {
    if serial::is_available() {
        serial_println!(
            "[WARN] VGA output failed during {}: {}",
            context,
            err.as_str()
        );
    }
}

/// Kernel panic handler
///
/// This function is called when a panic occurs anywhere in the kernel.
/// It performs the following actions:
///
/// 1. Displays detailed error information to serial output
/// 2. Displays summary information to VGA output
/// 3. Attempts to provide as much debug info as possible
/// 4. Halts the CPU in a safe state
///
/// # Panic Information
///
/// The panic handler extracts and displays:
/// - Panic message
/// - Source file location
/// - Line and column numbers
/// - Current initialization phase
/// - System state at panic time
///
/// # Safety Considerations
///
/// The panic handler is extremely defensive:
/// - Checks availability before using each output method
/// - Uses interrupt-safe operations only
/// - Never allocates memory
/// - Never performs operations that could panic recursively
/// - Halts CPU if panic occurs during panic handling
///
/// # Arguments
///
/// * `info` - Information about the panic including message and location
///
/// # Returns
///
/// This function never returns (`-> !`). The system halts permanently
/// after displaying the panic information.
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    use tiny_os::diagnostics::DIAGNOSTICS;

    // Record panic in diagnostics
    let panic_num = DIAGNOSTICS.record_panic();

    if panic_num > 0 {
        DIAGNOSTICS.mark_nested_panic();

        if serial::is_available() {
            serial_println!("[CRITICAL] Nested panic detected! Halting immediately.");
        }

        loop {
            x86_64::instructions::hlt();
        }
    }

    let mut output_success = false;

    if serial::is_available() {
        display::display_panic_info_serial(info);
        output_success = true;
    }

    if vga_buffer::is_accessible() {
        display::display_panic_info_vga(info);
        output_success = true;
    }

    if serial::is_available() {
        serial_println!();
        serial_println!("[STATE] System state at panic:");
        serial_println!("     - Initialization phase: {}", init::status_string());
        serial_println!("     - VGA accessible: {}", vga_buffer::is_accessible());
        serial_println!("     - Serial available: {}", serial::is_available());
        serial_println!();
    }

    if vga_buffer::is_accessible() {
        if let Err(err) = vga_buffer::print_colored(
            "\nThe system has encountered a critical error and must halt.\n",
            vga_buffer::ColorCode::error(),
        ) {
            log_vga_failure("panic banner", err);
        }
        if let Err(err) = vga_buffer::print_colored(
            "Please check serial output for detailed information.\n",
            vga_buffer::ColorCode::warning(),
        ) {
            log_vga_failure("panic advisory", err);
        }
        if let Err(err) =
            vga_buffer::print_colored("System halted.\n\n", vga_buffer::ColorCode::normal())
        {
            log_vga_failure("panic footer", err);
        }
    }

    if !output_success {
        emergency_panic_output(info);
    }

    init::halt_forever()
}

/// 最終手段のパニック出力（VGA/Serialが両方失敗した場合）
///
/// **警告**: これはデバッグエミュレータ（QEMU/Bochsなど）が提供する
/// 非標準の E9 I/O ポートを利用します。実機では動作しない可能性が高いです。
/// 目的は、パニック発生をホスト側へ通知する最後の手段です。
fn emergency_panic_output(info: &PanicInfo) {
    use x86_64::instructions::port::Port;

    // SAFETY: The 0xE9 port is treated as a debug console by common emulators.
    // We only touch it after other output mechanisms failed, while interrupts are disabled.
    let mut port = Port::<u8>::new(0xE9);
    let msg = b"!!! KERNEL PANIC - OUTPUT FAILED !!!\n";

    unsafe {
        for &byte in msg {
            port.write(byte);
        }
    }

    if let Some(location) = info.location() {
        unsafe {
            let prefix = b"File: ";
            for &byte in prefix {
                port.write(byte);
            }

            for &byte in location.file().as_bytes().iter().take(80) {
                port.write(byte);
            }

            let suffix = b"\nLocation info truncated\n";
            for &byte in suffix {
                port.write(byte);
            }
        }
    }
}

// Optional: Add a global allocator error handler if allocation were enabled
// Since we're no_std without alloc, this is not needed yet
// #[alloc_error_handler]
// fn alloc_error_handler(layout: alloc::alloc::Layout) -> ! {
//     panic!("Allocation error: {:?}", layout);
// }
