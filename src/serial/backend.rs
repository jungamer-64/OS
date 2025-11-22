// src/serial/backend.rs

//! Hardware abstraction for the serial driver.
//!
//! The goal of this module is to hide architecture- or platform-specific
//! register access details behind a lightweight trait so that the higher level
//! serial logic can be reused on targets that do not expose x86 style I/O
//! ports.

use super::constants::register_offset;
use crate::constants::SERIAL_IO_PORT;

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

/// Default backend implementation based on architecture.
pub type DefaultBackend = crate::arch::SerialBackend;

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
