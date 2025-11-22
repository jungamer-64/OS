// src/serial/mod.rs

//! Serial port driver (COM1) for debugging output
//!
//! Provides UART communication on COM1 (0x3F8) with:
//! - 38400 baud rate
//! - 8 data bits, no parity, 1 stop bit (8N1)
//! - FIFO buffer support with verification
//! - Hardware transmit buffer checking with timeout
//! - Robust hardware detection with multiple validation tests
//!
//! # Safety and Robustness
//!
//! This driver is designed to handle:
//! - Missing hardware (systems without serial ports)
//! - Hardware timeouts and unresponsive devices
//! - Concurrent access via Mutex protection
//! - Interrupt-safe operation

pub mod backend;
pub(crate) mod constants;
mod error;
pub mod ports;
mod timeout;

pub use error::InitError;
pub use timeout::{
    poll_with_timeout, poll_with_timeout_value, retry_with_timeout, timeout_stats, AdaptiveTimeout,
    RetryConfig, RetryResult, TimeoutConfig, TimeoutContext, TimeoutResult,
};

use crate::constants::*;
use crate::diagnostics::{LockTimingToken, DIAGNOSTICS};
#[cfg(debug_assertions)]
use crate::diagnostics::read_tsc;
use crate::sync::lock_manager::{acquire_lock, LockId};
pub use backend::{DefaultBackend, Register as SerialRegister, SerialHardware};

#[cfg(target_arch = "x86_64")]
pub use backend::PortIoBackend;
#[cfg(not(target_arch = "x86_64"))]
pub use backend::StubSerialBackend;
use constants::MAX_INIT_ATTEMPTS;
use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use ports::{DefaultSerialPorts, SerialPorts};
use spin::{Mutex, MutexGuard};

#[cfg(debug_assertions)]
use core::sync::atomic::AtomicU64;
#[cfg(debug_assertions)]
use core::sync::atomic::AtomicU32;

/// Serial port state tracking with atomic operations for thread safety
static SERIAL_INITIALIZED: AtomicBool = AtomicBool::new(false);
static SERIAL_PORT_AVAILABLE: AtomicBool = AtomicBool::new(false);
/// Tracks initialization attempts to prevent infinite retry loops
static INIT_ATTEMPTS: AtomicU8 = AtomicU8::new(0);

#[cfg(debug_assertions)]
static LOCK_ACQUISITIONS: AtomicU64 = AtomicU64::new(0);
#[cfg(debug_assertions)]
#[allow(dead_code)]
static LOCK_HOLDER_ID: AtomicU64 = AtomicU64::new(0);
#[cfg(debug_assertions)]
const MAX_LOCK_HOLD_TIME: u64 = 1_000_000;
#[cfg(debug_assertions)]
static LOCK_WARNINGS_EMITTED: AtomicU32 = AtomicU32::new(0);
#[cfg(debug_assertions)]
const MAX_LOCK_WARNING_LOGS: u32 = 16;

/// Global serial ports protected by Mutex
///
/// # Locking Order
///
/// CRITICAL: To prevent deadlocks, always acquire locks in this order:
/// 1. SERIAL_PORTS (this mutex)
/// 2. VGA_WRITER (in vga_buffer.rs)
///
/// Never acquire VGA_WRITER while holding SERIAL_PORTS.
static SERIAL_PORTS: Mutex<DefaultSerialPorts> =
    Mutex::new(SerialPorts::new(DefaultBackend::new()));

fn acquire_serial_ports_guard() -> (MutexGuard<'static, DefaultSerialPorts>, LockTimingToken) {
    // Acquire lock order enforcement first
    let _lock_guard = acquire_lock(LockId::Serial)
        .expect("Serial lock should always be acquirable (highest priority)");

    if let Some(guard) = SERIAL_PORTS.try_lock() {
        DIAGNOSTICS.record_lock_acquisition();
        let token = DIAGNOSTICS.begin_lock_timing();
        (guard, token)
    } else {
        DIAGNOSTICS.record_lock_contention();
        let guard = SERIAL_PORTS.lock();
        DIAGNOSTICS.record_lock_acquisition();
        let token = DIAGNOSTICS.begin_lock_timing();
        (guard, token)
    }
}

fn execute_with_serial_ports<F, R>(f: F) -> R
where
    F: FnOnce(&mut DefaultSerialPorts) -> R,
{
    let (mut guard, token) = acquire_serial_ports_guard();

    #[cfg(debug_assertions)]
    let start_time = read_tsc();

    #[cfg(debug_assertions)]
    let _ = LOCK_ACQUISITIONS.fetch_add(1, Ordering::SeqCst);

    let result = f(&mut guard);
    drop(guard);
    DIAGNOSTICS.finish_lock_timing(token);

    #[cfg(debug_assertions)]
    {
        let elapsed = read_tsc().saturating_sub(start_time);
        if elapsed > MAX_LOCK_HOLD_TIME && is_available() {
            let previous = LOCK_WARNINGS_EMITTED.fetch_add(1, Ordering::Relaxed);
            if previous < MAX_LOCK_WARNING_LOGS {
                // serial_println!("[WARN] Lock held for {} cycles", elapsed);
                if previous + 1 == MAX_LOCK_WARNING_LOGS {
                    // serial_println!(
                    //     "[WARN] Serial lock warning limit ({}) reached; suppressing further logs",
                    //     MAX_LOCK_WARNING_LOGS
                    // );
                }
            }
        }
    }

    result
}

pub(crate) fn with_serial_ports<F, R>(f: F) -> R
where
    F: FnOnce(&mut DefaultSerialPorts) -> R,
{
    execute_with_serial_ports(f)
}

/// Initialize serial port with robust error handling
///
/// This function performs comprehensive hardware detection and configuration.
/// It is safe to call multiple times - subsequent calls will return
/// `AlreadyInitialized`.
///
/// # Returns
///
/// - `Ok(())` if initialization succeeds
/// - `Err(InitError::AlreadyInitialized)` if already initialized
/// - `Err(InitError::PortNotPresent)` if hardware not detected
/// - `Err(InitError::Timeout)` if hardware doesn't respond
/// - `Err(InitError::TooManyAttempts)` if called too many times
pub fn init() -> Result<(), InitError> {
    // Fast-path check to avoid inflating attempt counter when already initialized
    if SERIAL_INITIALIZED.load(Ordering::Acquire) {
        return Err(InitError::AlreadyInitialized);
    }

    // Track how often we genuinely attempt initialization
    let attempts = INIT_ATTEMPTS.fetch_add(1, Ordering::SeqCst) + 1;
    if attempts > 1 {
        DIAGNOSTICS.record_serial_reinit();
    }
    if attempts > MAX_INIT_ATTEMPTS {
        INIT_ATTEMPTS.fetch_sub(1, Ordering::SeqCst);
        return Err(InitError::TooManyAttempts);
    }

    // Reserve the initialized flag for this attempt; if another thread won the race, back off
    if SERIAL_INITIALIZED
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        INIT_ATTEMPTS.fetch_sub(1, Ordering::SeqCst);
        return Err(InitError::AlreadyInitialized);
    }

    // Perform hardware detection
    if !is_port_present_robust()? {
        SERIAL_INITIALIZED.store(false, Ordering::Release);
        return Err(InitError::PortNotPresent);
    }

    // Configure UART
    configure_uart()?;

    // Mark as available
    SERIAL_PORT_AVAILABLE.store(true, Ordering::Release);
    Ok(())
}

/// Configure UART hardware
fn configure_uart() -> Result<(), InitError> {
    with_serial_ports(|ports| {
        ports.configure()?;

        // Verify configuration took effect by checking LSR
        let lsr = ports.read_line_status()?;

        if lsr == 0 || lsr == 0xFF {
            return Err(InitError::ConfigurationFailed);
        }

        let report = ports.comprehensive_validation()?;
        if !report.is_fully_valid() {
            return Err(InitError::ConfigurationFailed);
        }

        Ok(())
    })
}

/// Enhanced hardware detection with multiple validation tests
///
/// Performs a comprehensive series of tests to verify serial port presence:
/// 1. Scratch register write/read test with multiple patterns
/// 2. Line Status Register validation
/// 3. Modem Status Register validation
///
/// Uses retry logic for robustness.
///
/// # Returns
///
/// - `Ok(true)` if hardware is present and responsive
/// - `Ok(false)` if hardware is not present
/// - `Err(InitError)` if detection encountered an error
fn is_port_present_robust() -> Result<bool, InitError> {
    use timeout::{retry_with_timeout, RetryConfig, RetryResult};

    // Use retry mechanism for hardware detection
    let result = retry_with_timeout(RetryConfig::quick_retry(), || {
        with_serial_ports(|ports| -> Option<Result<bool, InitError>> {
            // Test 1: Scratch register with primary pattern
            ports.write_scratch(SCRATCH_TEST_PRIMARY).ok()?;
            wait_short();
            if ports.read_scratch().ok()? != SCRATCH_TEST_PRIMARY {
                return Some(Err(InitError::HardwareAccessFailed));
            }

            // Test 2: Scratch register with secondary pattern
            ports.write_scratch(SCRATCH_TEST_SECONDARY).ok()?;
            wait_short();
            if ports.read_scratch().ok()? != SCRATCH_TEST_SECONDARY {
                return Some(Err(InitError::HardwareAccessFailed));
            }

            // Test 3: Scratch register with zero
            ports.write_scratch(0x00).ok()?;
            wait_short();
            if ports.read_scratch().ok()? != 0x00 {
                return Some(Err(InitError::HardwareAccessFailed));
            }

            // Test 4: Verify LSR is not all 0xFF (indicates no hardware)
            let lsr = ports.read_line_status().ok()?;
            if lsr == 0xFF {
                return Some(Ok(false));
            }

            // Test 5: Verify MSR is not all 0xFF
            let msr = ports.read_modem_status().ok()?;
            if msr == 0xFF {
                return Some(Ok(false));
            }

            Some(Ok(true))
        })
    });

    match result {
        RetryResult::Ok(inner_result) => inner_result,
        RetryResult::Failed { .. } => Err(InitError::Timeout),
    }
}

/// Short delay for hardware response
///
/// Provides a brief delay to allow hardware to process commands.
/// Uses spin_loop hint for efficient waiting without busy-polling.
#[inline]
pub(super) fn wait_short() {
    for _ in 0..100 {
        core::hint::spin_loop();
    }
}

/// Wait with timeout for a condition
///
/// Uses the timeout module for robust waiting with backoff.
#[inline]
#[allow(dead_code)]
pub(super) fn wait_with_timeout<F>(condition: F, config: TimeoutConfig) -> Result<(), InitError>
where
    F: FnMut() -> bool,
{
    match timeout::poll_with_timeout(config, condition) {
        timeout::TimeoutResult::Ok(()) => Ok(()),
        timeout::TimeoutResult::Timeout { .. } => Err(InitError::Timeout),
    }
}

/// Check if serial port has been initialized
///
/// Returns `true` if `init()` has completed successfully, even if
/// the hardware is not actually present.
///
/// # Returns
///
/// `true` if initialization has been attempted, `false` otherwise
#[inline]
pub fn is_initialized() -> bool {
    SERIAL_INITIALIZED.load(Ordering::Acquire)
}

/// Check if serial port hardware is available
///
/// Returns `true` only if both initialized and hardware detected.
/// Use this before attempting serial writes to avoid hangs on
/// systems without COM1 hardware.
///
/// # Returns
///
/// `true` if serial hardware is present and functional, `false` otherwise
#[inline]
#[must_use = "serial availability should be checked to avoid I/O failures"]
pub fn is_available() -> bool {
    SERIAL_PORT_AVAILABLE.load(Ordering::Acquire)
}

/// Get serial port timeout statistics
///
/// Get timeout statistics
///
/// Returns `(successes, failures, multiplier_percentage)`
#[must_use]
pub fn get_timeout_stats() -> (u32, u32, u32) {
    if !is_available() {
        return (0, 0, 100);
    }
    with_serial_ports(|ports| ports.timeout_stats())
}

/// Reset serial port timeout statistics
pub fn reset_timeout_stats() {
    if !is_available() {
        return;
    }
    with_serial_ports(|ports| ports.reset_timeout_stats())
}

/// Get global timeout statistics from timeout module
/// Get global timeout statistics
///
/// Returns `(total_writes, total_timeouts)`
#[must_use]
pub fn get_global_timeout_stats() -> (u64, u64) {
    timeout::timeout_stats()
}

// Unit tests (compile in test configuration only)
#[cfg(all(test, feature = "std-tests"))]
mod tests {
    use super::*;

    #[test]
    fn test_is_available_before_init() {
        // Before initialization, should return false
        // Note: This test assumes fresh state
        assert!(!is_available() || is_initialized());
    }
}

/// Log a sequence of lines to the serial port
pub fn log_lines<I>(lines: I)
where
    I: IntoIterator,
    I::Item: core::fmt::Display,
{
    for line in lines {
        crate::serial_println!("{}", line);
    }
}

#[doc(hidden)]
pub fn print_impl(args: ::core::fmt::Arguments) {
    use core::fmt::Write;
    with_serial_ports(|ports| ports.write_fmt(args)).expect("Printing to serial failed");
}

/// Prints to the host through the serial interface.
#[macro_export]
macro_rules! serial_print {
    ($($arg:tt)*) => {
        $crate::serial::print_impl(format_args!($($arg)*));
    };
}

/// Prints to the host through the serial interface, appending a newline.
#[macro_export]
macro_rules! serial_println {
    () => ($crate::serial_print!("\n"));
    ($fmt:expr) => ($crate::serial_print!(concat!($fmt, "\n")));
    ($fmt:expr, $($arg:tt)*) => ($crate::serial_print!(concat!($fmt, "\n"), $($arg)*));
}
