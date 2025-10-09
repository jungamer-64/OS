// src/serial/ports.rs

//! Serial port hardware access and operations

use super::constants::{port_addr, register_offset};
use super::error::InitError;
use crate::constants::*;
use x86_64::instructions::port::Port;

/// Private operation enum for centralized unsafe access
pub(super) enum PortOp {
    Configure,
    ScratchWrite(u8),
    ScratchRead,
    LineStatusRead,
    ModemStatusRead,
    /// Poll LSR and write when ready, returns true on success
    PollAndWrite(u8),
}

/// Serial ports with validated safe access patterns
pub struct SerialPorts {
    data: Port<u8>,
    interrupt_enable: Port<u8>,
    fifo: Port<u8>,
    line_control: Port<u8>,
    modem_control: Port<u8>,
    line_status: Port<u8>,
    modem_status: Port<u8>,
    scratch: Port<u8>,
}

impl SerialPorts {
    pub const fn new() -> Self {
        Self {
            data: Port::new(port_addr(register_offset::DATA)),
            interrupt_enable: Port::new(port_addr(register_offset::INTERRUPT_ENABLE)),
            fifo: Port::new(port_addr(register_offset::FIFO_CONTROL)),
            line_control: Port::new(port_addr(register_offset::LINE_CONTROL)),
            modem_control: Port::new(port_addr(register_offset::MODEM_CONTROL)),
            line_status: Port::new(port_addr(register_offset::LINE_STATUS)),
            modem_status: Port::new(port_addr(register_offset::MODEM_STATUS)),
            scratch: Port::new(port_addr(register_offset::SCRATCH)),
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
    pub fn configure(&mut self) -> Result<(), InitError> {
        self.perform_op(PortOp::Configure)
            .map(|_| ())
            .ok_or(InitError::ConfigurationFailed)
    }

    /// Write to the scratch register
    ///
    /// The scratch register (offset 7) is documented in UART spec as
    /// side-effect-free and used for simple presence detection.
    /// Writing arbitrary bytes here cannot change device configuration.
    pub fn write_scratch(&mut self, value: u8) -> Result<(), InitError> {
        self.perform_op(PortOp::ScratchWrite(value))
            .map(|_| ())
            .ok_or(InitError::HardwareAccessFailed)
    }

    /// Read from the scratch register
    ///
    /// Reading the scratch register is safe and used only for detection;
    /// on systems without hardware, reads typically return 0xFF.
    pub fn read_scratch(&mut self) -> Result<u8, InitError> {
        self.perform_op(PortOp::ScratchRead)
            .ok_or(InitError::HardwareAccessFailed)
    }

    /// Read the Line Status Register (LSR)
    ///
    /// LSR reads are read-only and safe to poll. The mutex ensures we don't
    /// race with concurrent configuration writes.
    pub fn read_line_status(&mut self) -> Result<u8, InitError> {
        self.perform_op(PortOp::LineStatusRead)
            .ok_or(InitError::HardwareAccessFailed)
    }

    /// Read the Modem Status Register (MSR)
    ///
    /// Used for additional hardware validation.
    pub fn read_modem_status(&mut self) -> Result<u8, InitError> {
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
    pub fn poll_and_write(&mut self, byte: u8) -> Result<(), InitError> {
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
