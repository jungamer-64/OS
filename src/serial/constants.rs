// src/serial/constants.rs

//! Constants and register definitions for serial port operations

use crate::constants::SERIAL_IO_PORT;

/// Register offsets from base port
pub mod register_offset {
    pub const DATA: u16 = 0;
    pub const INTERRUPT_ENABLE: u16 = 1;
    pub const FIFO_CONTROL: u16 = 2;
    pub const LINE_CONTROL: u16 = 3;
    pub const MODEM_CONTROL: u16 = 4;
    pub const LINE_STATUS: u16 = 5;
    pub const MODEM_STATUS: u16 = 6;
    pub const SCRATCH: u16 = 7;
}

/// Maximum initialization attempts before giving up
pub const MAX_INIT_ATTEMPTS: u8 = 3;

/// Build serial port addresses from base and offset
pub const fn port_addr(offset: u16) -> u16 {
    SERIAL_IO_PORT + offset
}
