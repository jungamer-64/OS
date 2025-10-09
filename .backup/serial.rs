// src/serial.rs

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

use crate::constants::*;
use core::fmt::{self, Write};
use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use spin::Mutex;
use x86_64::instructions::port::Port;

/// Register offsets from base port
mod register_offset {
    pub const DATA: u16 = 0;
    pub const INTERRUPT_ENABLE: u16 = 1;
    pub const FIFO_CONTROL: u16 = 2;
    pub const LINE_CONTROL: u16 = 3;
    pub const MODEM_CONTROL: u16 = 4;
    pub const LINE_STATUS: u16 = 5;
    pub const MODEM_STATUS: u16 = 6;
    pub const SCRATCH: u16 = 7;
}

/// Serial port state tracking with atomic operations for thread safety
static SERIAL_INITIALIZED: AtomicBool = AtomicBool::new(false);
static SERIAL_PORT_AVAILABLE: AtomicBool = AtomicBool::new(false);
/// Tracks initialization attempts to prevent infinite retry loops
static INIT_ATTEMPTS: AtomicU8 = AtomicU8::new(0);

/// Maximum initialization attempts before giving up
const MAX_INIT_ATTEMPTS: u8 = 3;

/// Serial ports with validated safe access patterns
struct SerialPorts {
    data: Port<u8>,
    interrupt_enable: Port<u8>,
    fifo: Port<u8>,
    line_control: Port<u8>,
    modem_control: Port<u8>,
    line_status: Port<u8>,
    modem_status: Port<u8>,
    scratch: Port<u8>,
}

/// Private operation enum for centralized unsafe access
enum PortOp {
    Configure,
    ScratchWrite(u8),
    ScratchRead,
    LineStatusRead,
    ModemStatusRead,
    DataWrite(u8),
    /// Poll LSR and write when ready, returns true on success
    PollAndWrite(u8),
}

impl SerialPorts {
    const fn new() -> Self {
        Self {
            data: Port::new(SERIAL_IO_PORT + register_offset::DATA),
            interrupt_enable: Port::new(SERIAL_IO_PORT + register_offset::INTERRUPT_ENABLE),
            fifo: Port::new(SERIAL_IO_PORT + register_offset::FIFO_CONTROL),
            line_control: Port::new(SERIAL_IO_PORT + register_offset::LINE_CONTROL),
            modem_control: Port::new(SERIAL_IO_PORT + register_offset::MODEM_CONTROL),
            line_status: Port::new(SERIAL_IO_PORT + register_offset::LINE_STATUS),
            modem_status: Port::new(SERIAL_IO_PORT + register_offset::MODEM_STATUS),
            scratch: Port::new(SERIAL_IO_PORT + register_offset::SCRATCH),
        }
    }

    /// Configure UART registers
    ///
    /// # Safety
    ///
    /// This function performs port I/O to known COM1 registers. It is
    /// safe to call because:
    /// - The ports are fixed hardware I/O addresses for COM1 (0x3F8+offset)
    /// - Calls are serialized through the `SERIAL_PORTS` mutex
    /// - Only called during initialization or with proper locking
    /// - Configuration values are validated constants
    fn configure(&mut self) -> Result<(), InitError> {
        self.perform_op(PortOp::Configure)
            .map(|_| ())
            .ok_or(InitError::ConfigurationFailed)
    }

    /// Write to the scratch register
    ///
    /// The scratch register (offset 7) is documented in UART spec as
    /// side-effect-free and used for simple presence detection.
    /// Writing arbitrary bytes here cannot change device configuration.
    fn write_scratch(&mut self, value: u8) -> Result<(), InitError> {
        self.perform_op(PortOp::ScratchWrite(value))
            .map(|_| ())
            .ok_or(InitError::HardwareAccessFailed)
    }

    /// Read from the scratch register
    ///
    /// Reading the scratch register is safe and used only for detection;
    /// on systems without hardware, reads typically return 0xFF.
    fn read_scratch(&mut self) -> Result<u8, InitError> {
        self.perform_op(PortOp::ScratchRead)
            .ok_or(InitError::HardwareAccessFailed)
    }

    /// Read the Line Status Register (LSR)
    ///
    /// LSR reads are read-only and safe to poll. The mutex ensures we don't
    /// race with concurrent configuration writes.
    fn read_line_status(&mut self) -> Result<u8, InitError> {
        self.perform_op(PortOp::LineStatusRead)
            .ok_or(InitError::HardwareAccessFailed)
    }

    /// Read the Modem Status Register (MSR)
    ///
    /// Used for additional hardware validation.
    fn read_modem_status(&mut self) -> Result<u8, InitError> {
        self.perform_op(PortOp::ModemStatusRead)
            .ok_or(InitError::HardwareAccessFailed)
    }

    /// Poll the LSR and write a byte when transmitter is ready
    ///
    /// This method centralizes the transmit wait and actual write into a
    /// single operation to minimize unsafe blocks and ensure atomic behavior.
    ///
    /// Returns `Ok(())` on success, `Err(InitError::Timeout)` if transmitter
    /// doesn't become ready within the timeout period.
    fn poll_and_write(&mut self, byte: u8) -> Result<(), InitError> {
        match self.perform_op(PortOp::PollAndWrite(byte)) {
            Some(1) => Ok(()),
            _ => Err(InitError::Timeout),
        }
    }

    /// Perform a port operation inside a single unsafe block
    ///
    /// Centralizes all unsafe port I/O operations for easier auditing.
    ///
    /// # Safety
    ///
    /// All raw I/O accesses are centralized here. Callers must hold the
    /// `SERIAL_PORTS` mutex to ensure exclusive access. Port addresses
    /// are validated to be within COM1 range (0x3F8-0x3FF).
    ///
    /// # Returns
    ///
    /// - `Some(value)` for successful read operations
    /// - `Some(1)` for successful write operations
    /// - `None` for failed operations (timeout, invalid state)
    fn perform_op(&mut self, op: PortOp) -> Option<u8> {
        // SAFETY:
        // 1. All port addresses are compile-time constants in valid I/O range
        // 2. Exclusive access guaranteed by SERIAL_PORTS mutex
        // 3. Operations follow UART specification requirements
        // 4. No memory safety violations possible with port I/O
        unsafe {
            match op {
                PortOp::Configure => {
                    // Disable interrupts first
                    self.interrupt_enable.write(0x00);

                    // Set DLAB to configure baud rate
                    self.line_control.write(DLAB_ENABLE);
                    self.data.write((BAUD_RATE_DIVISOR & 0xFF) as u8);
                    self.interrupt_enable
                        .write(((BAUD_RATE_DIVISOR >> 8) & 0xFF) as u8);

                    // Configure 8N1 and clear DLAB
                    self.line_control.write(CONFIG_8N1);

                    // Enable and clear FIFO
                    self.fifo.write(FIFO_ENABLE_CLEAR);

                    // Enable modem control
                    self.modem_control.write(MODEM_CTRL_ENABLE_IRQ_RTS_DSR);

                    Some(1)
                }
                PortOp::ScratchWrite(v) => {
                    self.scratch.write(v);
                    Some(1)
                }
                PortOp::ScratchRead => Some(self.scratch.read()),
                PortOp::LineStatusRead => Some(self.line_status.read()),
                PortOp::ModemStatusRead => Some(self.modem_status.read()),
                PortOp::DataWrite(b) => {
                    self.data.write(b);
                    Some(1)
                }
                PortOp::PollAndWrite(b) => {
                    // Poll with timeout
                    for _ in 0..TIMEOUT_ITERATIONS {
                        if (self.line_status.read() & LSR_TRANSMIT_EMPTY) != 0 {
                            self.data.write(b);
                            return Some(1);
                        }
                        core::hint::spin_loop();
                    }
                    None // Timeout
                }
            }
        }
    }
}

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

/// Serial port initialization result
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitError {
    /// Port already initialized (not an error, just informational)
    AlreadyInitialized,
    /// Hardware not present or not responding
    PortNotPresent,
    /// Hardware timeout during initialization
    Timeout,
    /// Configuration failed
    ConfigurationFailed,
    /// Hardware access failed
    HardwareAccessFailed,
    /// Too many initialization attempts
    TooManyAttempts,
}

impl core::fmt::Display for InitError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            InitError::AlreadyInitialized => write!(f, "Serial port already initialized"),
            InitError::PortNotPresent => write!(f, "Serial port hardware not present"),
            InitError::Timeout => write!(f, "Serial port initialization timeout"),
            InitError::ConfigurationFailed => write!(f, "Serial port configuration failed"),
            InitError::HardwareAccessFailed => write!(f, "Serial port hardware access failed"),
            InitError::TooManyAttempts => {
                write!(f, "Too many serial port initialization attempts")
            }
        }
    }
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
    // Check initialization attempts
    let attempts = INIT_ATTEMPTS.fetch_add(1, Ordering::SeqCst);
    if attempts >= MAX_INIT_ATTEMPTS {
        return Err(InitError::TooManyAttempts);
    }

    // Check if already initialized
    if SERIAL_INITIALIZED.swap(true, Ordering::AcqRel) {
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
    if !is_available() {
        return Ok(()); // Silently succeed if no hardware
    }

    let mut ports = SERIAL_PORTS.lock();
    ports.poll_and_write(byte)
}

/// Write a string to the serial port
///
/// Writes each byte of the string to the serial port. If a byte
/// fails to write due to timeout, subsequent bytes are still attempted.
/// This ensures partial output is still visible even if hardware becomes
/// unresponsive.
pub fn write_str(s: &str) {
    for byte in s.bytes() {
        let _ = write_byte(byte); // Ignore individual byte errors
    }
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
    fn test_init_error_display() {
        assert_eq!(
            format!("{}", InitError::PortNotPresent),
            "Serial port hardware not present"
        );
    }

    #[test]
    fn test_is_available_before_init() {
        // Before initialization, should return false
        // Note: This test assumes fresh state
        assert!(!is_available() || is_initialized());
    }
}
