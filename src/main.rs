#![no_std]
#![no_main]

use core::panic::PanicInfo;
use x86_64::instructions::port::Port;

// Write a byte to COM1 (0x3F8)
fn serial_write_byte(byte: u8) {
    unsafe {
        let mut port: Port<u8> = Port::new(0x3F8);
        port.write(byte);
    }
}

fn serial_write_str(s: &str) {
    for &b in s.as_bytes() {
        serial_write_byte(b);
    }
}

// Minimal entry point for a bare-metal kernel. We export `_start` with C calling
// convention so the linker can find it as the program entry.
#[no_mangle]
pub extern "C" fn _start() -> ! {
    // Write to serial so QEMU -serial stdio can capture the message.
    serial_write_str("Hello, OS (no bootloader) - minimal kernel!\n");

    // Also write to VGA for visual confirmation in graphical QEMU.
    let vga_buffer = 0xb8000 as *mut u8;
    let message = b"Hello, OS (no bootloader) - minimal kernel!";
    unsafe {
        for (i, &byte) in message.iter().enumerate() {
            *vga_buffer.offset((i * 2) as isize) = byte;
            *vga_buffer.offset((i * 2 + 1) as isize) = 0x07;
        }
    }

    loop {}
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    serial_write_str("Kernel panic!\n");
    loop {}
}
