// src/serial.rs

//! Serial port driver (COM1) for debugging output
//!
//! Provides UART communication on COM1 (0x3F8) with:
//! - 38400 baud rate
//! - 8 data bits, no parity, 1 stop bit (8N1)
//! - FIFO buffer support
//! - Hardware transmit buffer checking

use core::fmt::{self, Write};
use core::sync::atomic::{AtomicBool, Ordering};
use spin::Mutex;
use x86_64::instructions::port::Port;

/// COM1 base I/O port address
const SERIAL_IO_PORT: u16 = 0x3F8;

/// Register offsets from base port
mod register_offset {
    pub const DATA: u16 = 0; // Data register (DLAB=0)
    pub const INTERRUPT_ENABLE: u16 = 1; // Interrupt Enable (DLAB=0)
    pub const FIFO_CONTROL: u16 = 2; // FIFO Control
    pub const LINE_CONTROL: u16 = 3; // Line Control
    pub const MODEM_CONTROL: u16 = 4; // Modem Control
    pub const LINE_STATUS: u16 = 5; // Line Status
    pub const SCRATCH: u16 = 7; // Scratch register (for testing)
}

/// Line Control Register (LCR) bit masks
mod line_control {
    /// Enable DLAB (Divisor Latch Access Bit)
    pub const DLAB_ENABLE: u8 = 0x80;
    /// 8 bits, no parity, 1 stop bit (8N1)
    pub const CONFIG_8N1: u8 = 0x03;
}

/// Line Status Register (LSR) bit masks
mod line_status {
    /// Transmit buffer empty
    pub const TRANSMIT_EMPTY: u8 = 0x20;
}

/// FIFO Control Register configuration
mod fifo_control {
    /// Enable FIFO, clear them, 14-byte threshold
    pub const ENABLE_AND_CLEAR: u8 = 0xC7;
}

/// Modem Control Register configuration
mod modem_control {
    /// IRQs enabled, RTS/DSR set
    pub const ENABLE_IRQ_RTS_DSR: u8 = 0x0B;
}

/// Baud rate divisor for 38400 baud
/// (115200 / 38400 = 3)
const BAUD_RATE_DIVISOR: u16 = 3;

/// Maximum iterations for timeout in spin loops
///
/// This value is tuned for real hardware compatibility:
/// - Modern CPUs: 3-5 GHz → ~10-16ms actual timeout
/// - Older CPUs: 1-2 GHz → ~50-100ms actual timeout
/// - Sufficient for normal UART operation (1-2ms per character at 38400 baud)
/// - Short enough to prevent boot delays on hardware issues
///
/// Note: This is iteration-based, not timer-based, so actual time varies by CPU speed.
/// For more accurate timing, consider using HPET or TSC in future versions.
const TIMEOUT_ITERATIONS: u32 = 10_000_000; // ~10-50ms depending on CPU

/// Static serial ports (avoid repeated Port::new() calls)
static SERIAL_DATA: Mutex<Port<u8>> = Mutex::new(Port::new(SERIAL_IO_PORT));
static SERIAL_LINE_STATUS_PORT: Mutex<Port<u8>> =
    Mutex::new(Port::new(SERIAL_IO_PORT + register_offset::LINE_STATUS));
static SERIAL_INITIALIZED: AtomicBool = AtomicBool::new(false);
static SERIAL_PORT_AVAILABLE: AtomicBool = AtomicBool::new(false);

/// Serial port initialization result
///
/// Represents possible outcomes of serial port initialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitError {
    /// Port already initialized
    ///
    /// Returned when `init()` is called multiple times. This is not
    /// a fatal error - the port is already configured and ready to use.
    AlreadyInitialized,
    
    /// Port hardware not present
    ///
    /// The COM1 port does not exist or is disabled in BIOS/PCI.
    /// This can happen on systems without physical serial ports or
    /// when the port is disabled in motherboard configuration.
    PortNotPresent,
    
    /// Timeout during initialization
    ///
    /// The port did not respond within the expected time.
    /// This may indicate hardware issues or incorrect port address.
    Timeout,
}

impl core::fmt::Display for InitError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            InitError::AlreadyInitialized => write!(f, "Serial port already initialized"),
            InitError::PortNotPresent => write!(f, "Serial port hardware not present"),
            InitError::Timeout => write!(f, "Serial port initialization timeout"),
        }
    }
}

/// Initialize the serial port (COM1)
///
/// Sets up UART with:
/// - 38400 baud rate
/// - 8 data bits
/// - No parity
/// - 1 stop bit (8N1 configuration)
/// - FIFO enabled with 14-byte threshold
///
/// # Hardware Detection
///
/// This function first checks if the serial port hardware exists.
/// On systems without COM1 (common on modern motherboards), this
/// prevents CPU hangs from writing to non-existent I/O ports.
///
/// # Returns
///
/// - `Ok(())` - Port initialized successfully
/// - `Err(InitError::AlreadyInitialized)` - Port already configured
/// - `Err(InitError::PortNotPresent)` - Hardware not detected
///
/// Subsequent calls return [`InitError::AlreadyInitialized`], allowing callers
/// to skip redundant hardware configuration safely.
pub fn init() -> Result<(), InitError> {
    if SERIAL_INITIALIZED.swap(true, Ordering::SeqCst) {
        return Err(InitError::AlreadyInitialized);
    }

    // Check if port hardware is present before attempting configuration
    if !is_port_present() {
        return Err(InitError::PortNotPresent);
    }

    unsafe {
        // SAFETY: Serial initialization touches hardware I/O ports during boot
        // and runs while interrupts are disabled, so no concurrent access occurs.
        let mut data_port: Port<u8> = Port::new(SERIAL_IO_PORT + register_offset::DATA);
        let mut int_en_port: Port<u8> =
            Port::new(SERIAL_IO_PORT + register_offset::INTERRUPT_ENABLE);
        let mut fifo_port: Port<u8> = Port::new(SERIAL_IO_PORT + register_offset::FIFO_CONTROL);
        let mut line_ctrl_port: Port<u8> =
            Port::new(SERIAL_IO_PORT + register_offset::LINE_CONTROL);
        let mut modem_ctrl_port: Port<u8> =
            Port::new(SERIAL_IO_PORT + register_offset::MODEM_CONTROL);

        // Disable all interrupts
        int_en_port.write(0x00);

        // Enable DLAB to set baud rate divisor
        line_ctrl_port.write(line_control::DLAB_ENABLE);

        // Set baud rate divisor (38400 baud)
        data_port.write((BAUD_RATE_DIVISOR & 0xFF) as u8); // Low byte
        int_en_port.write(((BAUD_RATE_DIVISOR >> 8) & 0xFF) as u8); // High byte

        // Configure: 8 bits, no parity, one stop bit (8N1)
        // This also disables DLAB
        line_ctrl_port.write(line_control::CONFIG_8N1);

        // Enable FIFO, clear buffers, set 14-byte threshold
        fifo_port.write(fifo_control::ENABLE_AND_CLEAR);

        // Enable IRQs, set RTS/DSR
        modem_ctrl_port.write(modem_control::ENABLE_IRQ_RTS_DSR);
    }

    // Mark port as available
    SERIAL_PORT_AVAILABLE.store(true, Ordering::SeqCst);

    Ok(())
}

/// Check if the serial port hardware is present
///
/// Tests the presence of COM1 by writing to and reading from the
/// scratch register. This prevents CPU hangs on systems without
/// serial port hardware.
///
/// # Returns
///
/// `true` if the port is present and responding, `false` otherwise.
///
/// # Safety
///
/// This function performs unsafe I/O operations but is safe because:
/// - Reading from an absent port returns 0xFF (floating bus)
/// - Writing to an absent port is a no-op
/// - No side effects occur from scratch register access
fn is_port_present() -> bool {
    unsafe {
        let mut scratch_port: Port<u8> = Port::new(SERIAL_IO_PORT + register_offset::SCRATCH);
        
        // Test pattern: write and read back
        const TEST_BYTE: u8 = 0xAA;
        
        // Write test pattern
        scratch_port.write(TEST_BYTE);
        
        // Small delay for hardware response
        for _ in 0..100 {
            core::hint::spin_loop();
        }
        
        // Read back and verify
        let readback = scratch_port.read();
        
        // If port is present, we should get back what we wrote
        // If absent, we typically get 0xFF (floating bus)
        readback == TEST_BYTE
    }
}

/// Return whether the serial port has already been initialized.
pub fn is_initialized() -> bool {
    SERIAL_INITIALIZED.load(Ordering::SeqCst)
}

/// Return whether the serial port hardware is available.
///
/// This can be checked before attempting serial output to avoid
/// issues on systems without COM1 hardware.
pub fn is_available() -> bool {
    SERIAL_PORT_AVAILABLE.load(Ordering::SeqCst)
}

/// Wait for serial transmit buffer to be empty with timeout
///
/// This function polls the Line Status Register (LSR) until the transmit
/// buffer empty bit is set. This is necessary to prevent data corruption
/// by ensuring the UART has finished transmitting the previous byte.
///
/// # Timeout Protection
///
/// To prevent infinite loops on real hardware (e.g., if the port stops
/// responding), this function implements a timeout. After a maximum number
/// of iterations, it will return `false` to indicate failure.
///
/// # Performance
///
/// On modern hardware, this typically completes in a few microseconds.
/// The spin loop uses `core::hint::spin_loop()` to hint to the CPU
/// that it should optimize for low power consumption.
///
/// # Returns
///
/// - `true` - Transmit buffer is empty
/// - `false` - Timeout occurred (hardware may be unresponsive)
///
/// # Safety
///
/// This function performs unsafe port I/O, but is safe because:
/// - Access is serialized via Mutex
/// - The port address is correct for COM1
/// - The LSR register is read-only and safe to poll
/// - Timeout prevents infinite loops
fn wait_transmit_empty() -> bool {
    let mut iterations = 0;
    
    unsafe {
        // SAFETY: The line-status register read is guarded by the Mutex locking
        // strategy ensuring serialized access to the underlying port.
        while (SERIAL_LINE_STATUS_PORT.lock().read() & line_status::TRANSMIT_EMPTY) == 0 {
            iterations += 1;
            if iterations > TIMEOUT_ITERATIONS {
                return false; // Timeout - port may be stuck
            }
            core::hint::spin_loop();
        }
    }
    
    true // Success
}

/// Write a single byte to COM1
///
/// This function waits for the transmit buffer to be empty before writing,
/// ensuring hardware-safe operation and preventing data corruption.
///
/// # Arguments
///
/// * `byte` - The byte to write to the serial port
///
/// # Safety
///
/// If the serial port is not available or times out, this function
/// will silently drop the byte rather than hanging the system.
fn write_byte(byte: u8) {
    // Skip if port is not available
    if !is_available() {
        return;
    }
    
    // Wait for transmit buffer with timeout
    if !wait_transmit_empty() {
        // Timeout occurred - skip this byte to prevent hang
        return;
    }
    
    unsafe {
        // SAFETY: Writing to the COM1 data register is synchronized via the static
        // Mutex, ensuring only one writer mutates the hardware port at a time.
        SERIAL_DATA.lock().write(byte);
    }
}

/// Write a string to the serial port
///
/// This is the primary function for serial output. It writes each byte
/// of the string sequentially, with hardware flow control.
///
/// # Arguments
///
/// * `s` - The string slice to write
///
/// # Examples
///
/// ```
/// serial::write_str("Hello, Serial!\n");
/// ```
pub fn write_str(s: &str) {
    for byte in s.bytes() {
        write_byte(byte);
    }
}

/// Serial writer implementing `core::fmt::Write`
///
/// This struct enables the use of Rust's formatting macros
/// (e.g., `write!`, `writeln!`) with the serial port.
///
/// # Examples
///
/// ```
/// use core::fmt::Write;
/// let mut writer = serial::SerialWriter;
/// write!(writer, "Value: {}\n", 42).unwrap();
/// ```
pub struct SerialWriter;

impl Write for SerialWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        write_str(s);
        Ok(())
    }
}

/// Write formatted data to the serial port (used by the logging macros)
#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    let mut writer = SerialWriter;
    let _ = writer.write_fmt(args);
}

/// Serial print macro (analogous to `print!` for VGA output)
#[macro_export]
macro_rules! serial_print {
    ($($arg:tt)*) => ({
        $crate::serial::_print(format_args!($($arg)*));
    });
}

/// Serial println macro (analogous to `println!` for VGA output)
#[macro_export]
macro_rules! serial_println {
    () => ($crate::serial_print!("\n"));
    ($fmt:expr) => ($crate::serial_print!(concat!($fmt, "\n")));
    ($fmt:expr, $($arg:tt)*) => ($crate::serial_print!(concat!($fmt, "\n"), $($arg)*));
}

// Tests are not supported in no_std environment
// For testing, consider using a hosted test harness with mocking
//
// #[cfg(test)]
// mod tests {
//     use super::*;
//
//     #[test]
//     fn test_baud_rate_divisor() {
//         // 115200 / 38400 = 3
//         assert_eq!(BAUD_RATE_DIVISOR, 3);
//     }
// }
