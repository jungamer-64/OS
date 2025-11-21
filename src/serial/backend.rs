// src/serial/backend.rs

//! Hardware abstraction for the serial driver.
//!
//! The goal of this module is to hide architecture- or platform-specific
//! register access details behind a lightweight trait so that the higher level
//! serial logic can be reused on targets that do not expose x86 style I/O
//! ports.

use super::constants::register_offset;
use crate::constants::SERIAL_IO_PORT;

#[cfg(target_arch = "x86_64")]
use x86_64::instructions::port::Port;

/// Registers that the UART driver interacts with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Register {
    Data,
    InterruptEnable,
    FifoControl,
    LineControl,
    ModemControl,
    LineStatus,
    ModemStatus,
    Scratch,
}

/// Minimal abstraction over UART register access.
pub trait SerialHardware {
    /// Write a value to a UART register.
    fn write(&mut self, register: Register, value: u8);
    /// Read the current value of a UART register.
    fn read(&mut self, register: Register) -> u8;
}

/// x86 specific implementation backed by port I/O instructions.
#[cfg(target_arch = "x86_64")]
pub struct PortIoBackend {
    data: Port<u8>,
    interrupt_enable: Port<u8>,
    fifo: Port<u8>,
    line_control: Port<u8>,
    modem_control: Port<u8>,
    line_status: Port<u8>,
    modem_status: Port<u8>,
    scratch: Port<u8>,
}

#[cfg(target_arch = "x86_64")]
impl PortIoBackend {
    /// Create a new backend backed by the standard COM1 base address.
    pub const fn new() -> Self {
        Self::with_base(SERIAL_IO_PORT)
    }

    /// Create a backend using a custom I/O base address.
    pub const fn with_base(base: u16) -> Self {
        Self {
            data: Port::new(base + register_offset::DATA),
            interrupt_enable: Port::new(base + register_offset::INTERRUPT_ENABLE),
            fifo: Port::new(base + register_offset::FIFO_CONTROL),
            line_control: Port::new(base + register_offset::LINE_CONTROL),
            modem_control: Port::new(base + register_offset::MODEM_CONTROL),
            line_status: Port::new(base + register_offset::LINE_STATUS),
            modem_status: Port::new(base + register_offset::MODEM_STATUS),
            scratch: Port::new(base + register_offset::SCRATCH),
        }
    }
}

#[cfg(target_arch = "x86_64")]
impl Default for PortIoBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(target_arch = "x86_64")]
impl SerialHardware for PortIoBackend {
    #[inline]
    fn write(&mut self, register: Register, value: u8) {
        unsafe {
            match register {
                Register::Data => self.data.write(value),
                Register::InterruptEnable => self.interrupt_enable.write(value),
                Register::FifoControl => self.fifo.write(value),
                Register::LineControl => self.line_control.write(value),
                Register::ModemControl => self.modem_control.write(value),
                Register::LineStatus => self.line_status.write(value),
                Register::ModemStatus => self.modem_status.write(value),
                Register::Scratch => self.scratch.write(value),
            }
        }
    }

    #[inline]
    fn read(&mut self, register: Register) -> u8 {
        unsafe {
            match register {
                Register::Data => self.data.read(),
                Register::InterruptEnable => self.interrupt_enable.read(),
                Register::FifoControl => self.fifo.read(),
                Register::LineControl => self.line_control.read(),
                Register::ModemControl => self.modem_control.read(),
                Register::LineStatus => self.line_status.read(),
                Register::ModemStatus => self.modem_status.read(),
                Register::Scratch => self.scratch.read(),
            }
        }
    }
}

#[cfg(not(target_arch = "x86_64"))]
const REGISTER_COUNT: usize = 8;

/// Fallback backend for targets without port I/O support.
#[cfg(not(target_arch = "x86_64"))]
#[derive(Debug, Clone)]
pub struct StubSerialBackend {
    registers: [u8; REGISTER_COUNT],
}

#[cfg(not(target_arch = "x86_64"))]
impl StubSerialBackend {
    pub const fn new() -> Self {
        Self {
            registers: [0; REGISTER_COUNT],
        }
    }
}

#[cfg(not(target_arch = "x86_64"))]
impl Default for StubSerialBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(not(target_arch = "x86_64"))]
impl SerialHardware for StubSerialBackend {
    #[inline]
    fn write(&mut self, register: Register, value: u8) {
        self.registers[register as usize] = value;
    }

    #[inline]
    fn read(&mut self, register: Register) -> u8 {
        self.registers[register as usize]
    }
}

/// Alias to the backend that should be used by default on the current target.
#[cfg(target_arch = "x86_64")]
pub type DefaultBackend = PortIoBackend;

#[cfg(not(target_arch = "x86_64"))]
pub type DefaultBackend = StubSerialBackend;
