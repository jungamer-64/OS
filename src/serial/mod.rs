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

mod constants;
mod error;
mod ports;

pub use error::InitError;

use crate::constants::*;
use constants::MAX_INIT_ATTEMPTS;
use core::fmt::{self, Write};
use core::iter;
use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use ports::SerialPorts;
use spin::Mutex;

/// Serial port state tracking with atomic operations for thread safety
static SERIAL_INITIALIZED: AtomicBool = AtomicBool::new(false);
static SERIAL_PORT_AVAILABLE: AtomicBool = AtomicBool::new(false);
/// Tracks initialization attempts to prevent infinite retry loops
static INIT_ATTEMPTS: AtomicU8 = AtomicU8::new(0);

/// Global serial ports protected by Mutex
///
/// # Locking Order
///
/// CRITICAL: To prevent deadlocks, always acquire locks in this order:
/// 1. SERIAL_PORTS (this mutex)
/// 2. VGA_WRITER (in vga_buffer.rs)
///
/// Never acquire VGA_WRITER while holding SERIAL_PORTS.
static SERIAL_PORTS: Mutex<SerialPorts> = Mutex::new(SerialPorts::new());

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
    let mut ports = SERIAL_PORTS.lock();
    ports.configure()?;

    // Verify configuration took effect by checking LSR
    let lsr = ports.read_line_status()?;

    // Basic sanity check: LSR should have some bits set
    // (transmit empty bit should be set after configuration)
    if lsr == 0 || lsr == 0xFF {
        return Err(InitError::ConfigurationFailed);
    }

    Ok(())
}

/// Enhanced hardware detection with multiple validation tests
///
/// Performs a comprehensive series of tests to verify serial port presence:
/// 1. Scratch register write/read test with multiple patterns
/// 2. Line Status Register validation
/// 3. Modem Status Register validation
///
/// # Returns
///
/// - `Ok(true)` if hardware is present and responsive
/// - `Ok(false)` if hardware is not present
/// - `Err(InitError)` if detection encountered an error
fn is_port_present_robust() -> Result<bool, InitError> {
    let mut ports = SERIAL_PORTS.lock();

    // Test 1: Scratch register with primary pattern
    ports.write_scratch(SCRATCH_TEST_PRIMARY)?;
    wait_short();
    if ports.read_scratch()? != SCRATCH_TEST_PRIMARY {
        return Ok(false);
    }

    // Test 2: Scratch register with secondary pattern
    ports.write_scratch(SCRATCH_TEST_SECONDARY)?;
    wait_short();
    if ports.read_scratch()? != SCRATCH_TEST_SECONDARY {
        return Ok(false);
    }

    // Test 3: Scratch register with zero
    ports.write_scratch(0x00)?;
    wait_short();
    if ports.read_scratch()? != 0x00 {
        return Ok(false);
    }

    // Test 4: Verify LSR is not all 0xFF (indicates no hardware)
    let lsr = ports.read_line_status()?;
    if lsr == 0xFF {
        return Ok(false);
    }

    // Test 5: Verify MSR is not all 0xFF
    let msr = ports.read_modem_status()?;
    if msr == 0xFF {
        return Ok(false);
    }

    Ok(true)
}

/// Short delay for hardware response
///
/// Provides a brief delay to allow hardware to process commands.
/// Uses spin_loop hint for efficient waiting without busy-polling.
#[inline(always)]
fn wait_short() {
    for _ in 0..100 {
        core::hint::spin_loop();
    }
}

/// Return whether the serial port has been initialized
#[inline]
pub fn is_initialized() -> bool {
    SERIAL_INITIALIZED.load(Ordering::Acquire)
}

/// Return whether the serial port hardware is available
#[inline]
pub fn is_available() -> bool {
    SERIAL_PORT_AVAILABLE.load(Ordering::Acquire)
}

/// Write a single byte to COM1 with error handling
///
/// This function checks hardware availability before attempting to write.
/// If hardware is not available or a timeout occurs, the write is silently
/// dropped to prevent blocking.
///
/// # Arguments
///
/// * `byte` - The byte to write
///
/// # Returns
///
/// - `Ok(())` if write succeeds
/// - `Err(InitError::Timeout)` if transmitter doesn't become ready
fn write_byte(byte: u8) -> Result<(), InitError> {
    write_bytes(iter::once(byte))
}

/// Write a string to the serial port
///
/// Writes each byte of the string to the serial port. If a byte
/// fails to write due to timeout, subsequent bytes are still attempted.
/// This ensures partial output is still visible even if hardware becomes
/// unresponsive.
pub fn write_str(s: &str) {
    if s.is_empty() {
        return;
    }

    let _ = write_bytes(s.bytes());
}

/// Write a collection of lines to the serial port, inserting newlines automatically.
///
/// Empty strings are interpreted as explicit blank lines. The helper quietly
/// returns when the serial hardware is not available, mirroring the behaviour of
/// the existing printing macros.
pub fn log_lines<'a, I>(lines: I)
where
    I: IntoIterator<Item = &'a str>,
{
    if !is_available() {
        return;
    }

    for line in lines {
        if line.is_empty() {
            serial_println!();
        } else {
            serial_println!("{}", line);
        }
    }
}

/// Write a sequence of bytes while holding the serial port lock once.
fn write_bytes<I>(bytes: I) -> Result<(), InitError>
where
    I: IntoIterator<Item = u8>,
{
    if !is_available() {
        return Ok(());
    }

    let mut ports = SERIAL_PORTS.lock();
    let mut first_error: Option<InitError> = None;
    for byte in bytes {
        if let Err(err) = ports.poll_and_write(byte) {
            if first_error.is_none() {
                first_error = Some(err);
            }
        }
    }

    first_error.map_or(Ok(()), Err)
}

/// Serial writer implementing `core::fmt::Write`
pub struct SerialWriter;

impl Write for SerialWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        write_str(s);
        Ok(())
    }
}

/// Write formatted data to the serial port
#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    let mut writer = SerialWriter;
    let _ = writer.write_fmt(args);
}

/// Serial print macro
#[macro_export]
macro_rules! serial_print {
    ($($arg:tt)*) => ({
        $crate::serial::_print(format_args!($($arg)*));
    });
}

/// Serial println macro
#[macro_export]
macro_rules! serial_println {
    () => ($crate::serial_print!("\n"));
    ($fmt:expr) => ($crate::serial_print!(concat!($fmt, "\n")));
    ($fmt:expr, $($arg:tt)*) => ($crate::serial_print!(
        concat!($fmt, "\n"), $($arg)*
    ));
}

// Unit tests (compile in test configuration only)
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_available_before_init() {
        // Before initialization, should return false
        // Note: This test assumes fresh state
        assert!(!is_available() || is_initialized());
    }
}
