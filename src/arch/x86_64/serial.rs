use x86_64::instructions::port::Port;
use crate::serial::backend::{SerialHardware, Register};
use crate::serial::constants::register_offset;
use crate::constants::SERIAL_IO_PORT;

/// x86 specific implementation backed by port I/O instructions.
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

impl Default for PortIoBackend {
    fn default() -> Self {
        Self::new()
    }
}

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
