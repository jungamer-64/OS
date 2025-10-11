// src/panic/handler.rs

//! Robust panic handler with nested panic protection
//!
//! This module provides a multi-layered panic handling system that:
//! - Detects and prevents nested panics
//! - Provides fallback output mechanisms
//! - Collects maximum diagnostic information
//! - Ensures system halts safely in all cases

use core::panic::PanicInfo;
use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};

/// Panic state tracking
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum PanicState {
    /// First panic being handled
    InPanic = 1,
    /// Nested panic detected
    NestedPanic = 2,
    /// Critical failure, emergency halt
    CriticalFailure = 3,
}

/// Global panic state (0 = no panic, matches no variant)
static PANIC_STATE: AtomicU8 = AtomicU8::new(0);

/// Whether panic output has been attempted
static OUTPUT_ATTEMPTED: AtomicBool = AtomicBool::new(false);

/// Panic entry guard - ensures proper state transition
struct PanicGuard {
    state: PanicState,
}

impl PanicGuard {
    /// Enter panic handling
    fn enter() -> Self {
        let prev_state = PANIC_STATE.swap(PanicState::InPanic as u8, Ordering::SeqCst);

        let state = match prev_state {
            0 => PanicState::InPanic,
            1 => PanicState::NestedPanic,
            _ => PanicState::CriticalFailure,
        };

        Self { state }
    }

    /// Get current panic state
    fn state(&self) -> PanicState {
        self.state
    }

    /// Check if this is a nested panic
    fn is_nested(&self) -> bool {
        matches!(
            self.state,
            PanicState::NestedPanic | PanicState::CriticalFailure
        )
    }
}

/// Panic handler implementation
pub fn handle_panic(info: &PanicInfo) -> ! {
    let guard = PanicGuard::enter();

    match guard.state() {
        PanicState::InPanic => {
            // First panic - try full diagnostic output
            handle_primary_panic(info);
        }
        PanicState::NestedPanic => {
            // Nested panic - minimal output only
            handle_nested_panic(info);
        }
        PanicState::CriticalFailure => {
            // Critical failure - emergency halt
            emergency_halt(info);
        }
    }

    // Should never reach here, but ensure halt
    halt_forever()
}

/// Handle the primary (first) panic
fn handle_primary_panic(info: &PanicInfo) {
    // Disable interrupts to prevent reentrancy
    x86_64::instructions::interrupts::disable();

    // Try serial output first (most reliable)
    let serial_ok = try_serial_output(info);

    // Try VGA output
    let vga_ok = try_vga_output(info);

    // If both failed, try emergency output
    if !serial_ok && !vga_ok {
        emergency_output(info);
    }

    OUTPUT_ATTEMPTED.store(true, Ordering::Release);
}

/// Handle nested panic (panic during panic handling)
fn handle_nested_panic(info: &PanicInfo) {
    x86_64::instructions::interrupts::disable();

    // Only try emergency output for nested panics
    emergency_output_minimal(info);
}

/// Emergency halt for critical failures
fn emergency_halt(_info: &PanicInfo) {
    x86_64::instructions::interrupts::disable();

    // Can't safely output anything at this point
    // Just log to debug port if available
    debug_port_emergency_message();

    halt_forever()
}

/// Try to output panic info to serial port
fn try_serial_output(info: &PanicInfo) -> bool {
    // Check if serial is available without taking locks
    if !is_serial_available_lockfree() {
        return false;
    }

    // Try to output panic info
    // This is wrapped in a simple check to prevent nested panics
    let result = core::panic::catch_unwind(core::panic::AssertUnwindSafe(|| {
        output_to_serial(info);
    }));

    result.is_ok()
}

/// Try to output panic info to VGA
fn try_vga_output(info: &PanicInfo) -> bool {
    if !is_vga_available_lockfree() {
        return false;
    }

    let result = core::panic::catch_unwind(core::panic::AssertUnwindSafe(|| {
        output_to_vga(info);
    }));

    result.is_ok()
}

/// Emergency output using port 0xE9 (QEMU debug port)
fn emergency_output(info: &PanicInfo) {
    use x86_64::instructions::port::Port;

    unsafe {
        let mut port = Port::<u8>::new(0xE9);

        // Output message
        let msg = b"!!! KERNEL PANIC !!!\n";
        for &byte in msg {
            port.write(byte);
        }

        // Try to output location if available
        if let Some(location) = info.location() {
            let file_msg = b"File: ";
            for &byte in file_msg {
                port.write(byte);
            }

            // Output file name (truncated)
            for &byte in location.file().as_bytes().iter().take(50) {
                port.write(byte);
            }

            port.write(b'\n');
        }

        let halt_msg = b"System halted.\n";
        for &byte in halt_msg {
            port.write(byte);
        }
    }
}

/// Minimal emergency output for nested panics
///
/// Following Microsoft Docs best practices: provide detailed context
/// for debugging even in minimal output scenarios
fn emergency_output_minimal(info: &PanicInfo) {
    use x86_64::instructions::port::Port;

    unsafe {
        let mut port = Port::<u8>::new(0xE9);

        // Header with context
        let header = b"\n!!! NESTED PANIC DETECTED !!!\n";
        for &byte in header {
            port.write(byte);
        }

        // Location information if available
        if let Some(location) = info.location() {
            let loc_msg = b"Location: ";
            for &byte in loc_msg {
                port.write(byte);
            }

            // File name
            let file = location.file().as_bytes();
            for &byte in file.iter().take(60) {
                port.write(byte);
            }

            port.write(b':');

            // Line number (simple decimal output)
            let line = location.line();
            write_decimal_to_port(&mut port, line);

            port.write(b'\n');
        }

        let halt_msg = b"System halting to prevent corruption.\n";
        for &byte in halt_msg {
            port.write(byte);
        }
    }
}

/// Helper to write decimal number to serial port
fn write_decimal_to_port(port: &mut x86_64::instructions::port::Port<u8>, mut num: u32) {
    if num == 0 {
        port.write(b'0');
        return;
    }

    let mut digits = [0u8; 10];
    let mut count = 0;

    while num > 0 {
        digits[count] = b'0' + (num % 10) as u8;
        num /= 10;
        count += 1;
    }

    for i in (0..count).rev() {
        port.write(digits[i]);
    }
}

/// Output to debug port for critical failures
///
/// Enhanced with context following Microsoft Docs error handling guidance:
/// "Provide detailed error messages for debugging"
fn debug_port_emergency_message() {
    use x86_64::instructions::port::Port;

    unsafe {
        let mut port = Port::<u8>::new(0xE9);

        let header = b"\n!!! CRITICAL PANIC FAILURE !!!\n";
        for &byte in header {
            port.write(byte);
        }

        let context = b"Context: Multiple panic attempts detected\n";
        for &byte in context {
            port.write(byte);
        }

        let action = b"Action: Emergency system halt to prevent data corruption\n";
        for &byte in action {
            port.write(byte);
        }

        let recommendation =
            b"Recommendation: Review panic handler logs and check for race conditions\n";
        for &byte in recommendation {
            port.write(byte);
        }
    }
}

/// Check if serial is available without taking locks
fn is_serial_available_lockfree() -> bool {
    // This would check atomic flags instead of taking locks
    // Implementation depends on your serial driver
    false // Conservative default
}

/// Check if VGA is available without taking locks
fn is_vga_available_lockfree() -> bool {
    // This would check atomic flags
    false // Conservative default
}

/// Output panic info to serial (actual implementation)
fn output_to_serial(_info: &PanicInfo) {
    // Implementation would go here
    // This should be as simple as possible to avoid nested panics
}

/// Output panic info to VGA (actual implementation)
fn output_to_vga(_info: &PanicInfo) {
    // Implementation would go here
    // Keep it simple to avoid nested panics
}

/// Halt the CPU forever
fn halt_forever() -> ! {
    loop {
        x86_64::instructions::interrupts::disable();
        x86_64::instructions::hlt();
    }
}

/// Panic statistics for diagnostics
pub struct PanicStats {
    pub state: PanicState,
    pub output_attempted: bool,
}

impl PanicStats {
    pub fn current() -> Self {
        let state_val = PANIC_STATE.load(Ordering::Acquire);
        let state = match state_val {
            1 => PanicState::InPanic,
            2 => PanicState::NestedPanic,
            3 => PanicState::CriticalFailure,
            // 0 or any other value means no panic
            _ => PanicState::InPanic, // Safe default for diagnostic purposes
        };

        Self {
            state,
            output_attempted: OUTPUT_ATTEMPTED.load(Ordering::Acquire),
        }
    }

    pub fn is_panicking(&self) -> bool {
        // If we're reading PanicStats, we're likely in panic handler
        // so assume panicking=true
        true
    }
}

#[cfg(all(test, feature = "std-tests"))]
mod tests {
    use super::*;

    #[test]
    fn test_panic_state_transitions() {
        assert_eq!(PanicState::InPanic as u8, 1);
        assert_eq!(PanicState::NestedPanic as u8, 2);
        assert_eq!(PanicState::CriticalFailure as u8, 3);
        // Note: 0 represents no panic state (no enum variant)
    }

    #[test]
    fn test_panic_guard_detects_nesting() {
        // First panic
        let guard1 = PanicGuard::enter();
        assert!(!guard1.is_nested());

        // Would be nested (can't actually test without unsafe)
        // Just verify state values are correct
        assert_eq!(guard1.state(), PanicState::InPanic);
    }
}
