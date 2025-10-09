// src/serial.rs

//! Serial port driver (COM1) for debugging output
//!
//! Provides UART communication on COM1 (0x3F8) with:
//! - 38400 baud rate
//! - 8 data bits, no parity, 1 stop bit (8N1)
//! - FIFO buffer support
//! - Hardware transmit buffer checking

use crate::constants::*;
use core::fmt::{self, Write};
use core::sync::atomic::{AtomicBool, Ordering};
use spin::Mutex;
use x86_64::instructions::port::Port;

/// COM1 base I/O port address (moved to `constants.rs`)

/// Register offsets from base port
mod register_offset {
    pub const DATA: u16 = 0;
    pub const INTERRUPT_ENABLE: u16 = 1;
    pub const FIFO_CONTROL: u16 = 2;
    pub const LINE_CONTROL: u16 = 3;
    pub const MODEM_CONTROL: u16 = 4;
    pub const LINE_STATUS: u16 = 5;
    pub const SCRATCH: u16 = 7;
}

// Bit masks and hardware constants moved to `constants.rs`

// (moved test patterns and other hardware constants to `constants.rs`)

/// Serial port state tracking
static SERIAL_INITIALIZED: AtomicBool = AtomicBool::new(false);
static SERIAL_PORT_AVAILABLE: AtomicBool = AtomicBool::new(false);

/// Lazy-initialized serial ports (created on first use)
struct SerialPorts {
    data: Port<u8>,
    interrupt_enable: Port<u8>,
    fifo: Port<u8>,
    line_control: Port<u8>,
    modem_control: Port<u8>,
    line_status: Port<u8>,
    scratch: Port<u8>,
}

/// Private operation enum for centralized unsafe access
enum PortOp {
    Configure,
    ScratchTransfer(Option<u8>),
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
            scratch: Port::new(SERIAL_IO_PORT + register_offset::SCRATCH),
        }
    }

    /// Configure UART registers (encapsulates unsafe writes)
    /// Configure UART registers
    ///
    /// # Safety
    ///
    /// This function performs port I/O to known COM1 registers. It is
    /// safe to call from the kernel initialization path because:
    /// - The ports are fixed hardware I/O addresses for COM1
    /// - Calls are serialized through the `SERIAL_PORTS` mutex
    /// - This runs early in boot while interrupts are typically disabled
    fn configure(&mut self) {
        let _ = self.perform_op(PortOp::Configure);
    }

    /// Write scratch register (encapsulated)
    /// Write to the scratch register
    ///
    /// The scratch register is documented as side-effect-free and used for
    /// simple presence detection. Writing arbitrary bytes here cannot
    /// change device configuration.
    fn write_scratch(&mut self, value: u8) {
        // Delegate to transfer_scratch to centralize unsafe usage
        let _ = self.transfer_scratch(Some(value));
    }

    /// Read scratch register (encapsulated)
    /// Read from the scratch register
    ///
    /// Reading the scratch register is safe and used only for detection;
    /// on hardware-less systems a read typically returns 0xFF.
    fn read_scratch(&mut self) -> u8 {
        self.transfer_scratch(None)
    }

    /// Transfer to/from scratch register in a single unsafe block
    ///
    /// If `write` is Some(value) we write the value then read back; if None
    /// we only read. Centralizing this keeps the number of `unsafe` blocks low.
    fn transfer_scratch(&mut self, write: Option<u8>) -> u8 {
        // Delegate to central operation performer to minimize unsafe sites
        match self.perform_op(PortOp::ScratchTransfer(write)) {
            Some(v) => v,
            None => 0xFF,
        }
    }

    /// Read line status register (encapsulated)
    /// Read the Line Status Register (LSR)
    ///
    /// LSR reads are read-only and safe to poll. The mutex ensures we don't
    /// race with concurrent configuration writes.
    // read_line_status is intentionally removed; use poll_and_write or
    // transfer_scratch to centralize unsafe usage.

    /// Write data register (encapsulated)
    /// Write a byte to the data register
    ///
    /// This should be invoked only when the transmit buffer is empty.
    /// Poll the LSR and write a byte when transmitter is ready
    ///
    /// This method centralizes the transmit wait and actual write into a
    /// single `unsafe` block to minimize the number of unsafe sites in the
    /// file. Returns `true` on success and `false` on timeout.
    fn poll_and_write(&mut self, byte: u8) -> bool {
        match self.perform_op(PortOp::PollAndWrite(byte)) {
            Some(1) => true,
            _ => false,
        }
    }

    // (PortOp moved to module scope)

    /// Perform a port operation inside a single unsafe block
    fn perform_op(&mut self, op: PortOp) -> Option<u8> {
        // SAFETY: All raw I/O accesses are centralized here. Callers must
        // hold the `SERIAL_PORTS` mutex to ensure exclusive access.
        unsafe {
            match op {
                PortOp::Configure => {
                    self.interrupt_enable.write(0x00);
                    self.line_control.write(DLAB_ENABLE);
                    self.data.write((BAUD_RATE_DIVISOR & 0xFF) as u8);
                    self.interrupt_enable
                        .write(((BAUD_RATE_DIVISOR >> 8) & 0xFF) as u8);
                    self.line_control.write(CONFIG_8N1);
                    self.fifo.write(FIFO_ENABLE_CLEAR);
                    self.modem_control.write(MODEM_CTRL_ENABLE_IRQ_RTS_DSR);
                    None
                }
                PortOp::ScratchTransfer(write) => {
                    if let Some(v) = write {
                        self.scratch.write(v);
                    }
                    Some(self.scratch.read())
                }
                PortOp::PollAndWrite(b) => {
                    for _ in 0..TIMEOUT_ITERATIONS {
                        if (self.line_status.read() & LSR_TRANSMIT_EMPTY) != 0 {
                            self.data.write(b);
                            return Some(1);
                        }
                        core::hint::spin_loop();
                    }
                    None
                }
            }
        }
    }
}

static SERIAL_PORTS: Mutex<SerialPorts> = Mutex::new(SerialPorts::new());

/// Serial port initialization result
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitError {
    AlreadyInitialized,
    PortNotPresent,
    #[allow(dead_code)]
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

pub fn init() -> Result<(), InitError> {
    if SERIAL_INITIALIZED.swap(true, Ordering::AcqRel) {
        return Err(InitError::AlreadyInitialized);
    }

    // Enhanced hardware detection with multiple tests
    if !is_port_present_robust() {
        SERIAL_INITIALIZED.store(false, Ordering::Release);
        return Err(InitError::PortNotPresent);
    }

    configure_uart()?;

    SERIAL_PORT_AVAILABLE.store(true, Ordering::Release);
    Ok(())
}

/// Configure UART hardware
fn configure_uart() -> Result<(), InitError> {
    SERIAL_PORTS.lock().configure();
    Ok(())
}

/// Enhanced hardware detection with multiple test patterns
fn is_port_present_robust() -> bool {
    let mut ports = SERIAL_PORTS.lock();

    // Test 1
    ports.write_scratch(SCRATCH_TEST_PRIMARY);
    wait_short();
    if ports.read_scratch() != SCRATCH_TEST_PRIMARY {
        return false;
    }

    // Test 2
    ports.write_scratch(SCRATCH_TEST_SECONDARY);
    wait_short();
    if ports.read_scratch() != SCRATCH_TEST_SECONDARY {
        return false;
    }

    // Test 3
    ports.write_scratch(0x00);
    wait_short();
    ports.read_scratch() == 0x00
}

/// Short delay for hardware response
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

// wait_transmit_empty removed in favor of SerialPorts::poll_and_write

/// Write a single byte to COM1
fn write_byte(byte: u8) {
    if !is_available() {
        return;
    }

    let mut ports = SERIAL_PORTS.lock();
    let _ = ports.poll_and_write(byte);
}

/// Write a string to the serial port
pub fn write_str(s: &str) {
    for byte in s.bytes() {
        write_byte(byte);
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
