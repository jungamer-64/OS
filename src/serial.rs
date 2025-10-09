// src/serial.rs

//! Serial port driver (COM1) for debugging output
//!
//! Provides UART communication on COM1 (0x3F8) with:
//! - 38400 baud rate
//! - 8 data bits, no parity, 1 stop bit (8N1)
//! - FIFO buffer support
//! - Hardware transmit buffer checking

use core::fmt::{self, Write};
use spin::Mutex;
use x86_64::instructions::port::Port;

/// COM1 base port and register offsets
const SERIAL_IO_PORT: u16 = 0x3F8;
const SERIAL_LINE_STATUS: u16 = SERIAL_IO_PORT + 5;

/// Static serial ports (avoid repeated Port::new() calls)
static SERIAL_DATA: Mutex<Port<u8>> = Mutex::new(Port::new(SERIAL_IO_PORT));
static SERIAL_LINE_STATUS_PORT: Mutex<Port<u8>> = Mutex::new(Port::new(SERIAL_LINE_STATUS));

/// Initialize the serial port (COM1)
/// Sets up UART with 38400 baud, 8N1 (8 data bits, no parity, 1 stop bit)
pub fn init() {
    unsafe {
        let mut data_port: Port<u8> = Port::new(SERIAL_IO_PORT);
        let mut int_en_port: Port<u8> = Port::new(SERIAL_IO_PORT + 1);
        let mut fifo_port: Port<u8> = Port::new(SERIAL_IO_PORT + 2);
        let mut line_ctrl_port: Port<u8> = Port::new(SERIAL_IO_PORT + 3);
        let mut modem_ctrl_port: Port<u8> = Port::new(SERIAL_IO_PORT + 4);

        // Disable interrupts
        int_en_port.write(0x00);

        // Enable DLAB (set baud rate divisor)
        line_ctrl_port.write(0x80);

        // Set divisor to 3 (lo byte) 38400 baud
        data_port.write(0x03);
        int_en_port.write(0x00); // (hi byte)

        // 8 bits, no parity, one stop bit (8N1)
        line_ctrl_port.write(0x03);

        // Enable FIFO, clear them, with 14-byte threshold
        fifo_port.write(0xC7);

        // IRQs enabled, RTS/DSR set
        modem_ctrl_port.write(0x0B);
    }
}

/// Wait for serial transmit buffer to be empty
fn wait_transmit_empty() {
    unsafe {
        // Wait until bit 5 (transmit buffer empty) is set
        while (SERIAL_LINE_STATUS_PORT.lock().read() & 0x20) == 0 {
            core::hint::spin_loop();
        }
    }
}

/// Write a byte to COM1 (with FIFO check for hardware compatibility)
fn write_byte(byte: u8) {
    wait_transmit_empty();
    unsafe {
        SERIAL_DATA.lock().write(byte);
    }
}

/// Write a string to serial port
pub fn write_str(s: &str) {
    for b in s.bytes() {
        write_byte(b);
    }
}

/// Serial writer for core::fmt::Write trait
pub struct SerialWriter;

impl Write for SerialWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        write_str(s);
        Ok(())
    }
}
