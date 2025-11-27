#![no_std]
#![no_main]

use libuser::{println, process::exit};
use core::panic::PanicInfo;

/// Simple hello world program for testing
#[no_mangle]
#[link_section = ".text.entry"]
pub extern "C" fn _start() -> ! {
    println!("Hello from Tiny OS!");
    println!("This is a simple test program.");
    println!("");
    println!("System information:");
    println!("  PID: [process ID here]");
    println!("  Userland: Ring 3");
    println!("  Architecture: x86_64");
    println!("");
    println!("Exiting with code 0...");
    
    exit(0);
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    println!("Panic in hello program!");
    exit(1);
}
