#![no_std]
#![no_main]

use core::panic::PanicInfo;
use bootloader::{entry_point, BootInfo};

entry_point!(kernel_main);

fn kernel_main(_boot_info: &'static BootInfo) -> ! {
    let vga_buffer = 0xb8000 as *mut u8;
    let message = b"Hello, OS from Rust with bootloader!";
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
    loop {}
}
