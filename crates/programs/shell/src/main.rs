//! Shell Program

#![no_std]
#![no_main]

use libuser::io::println;
use libuser::process::exit;
use core::panic::PanicInfo;

#[no_mangle]
#[link_section = ".text.entry"]
pub extern "C" fn _start() -> ! {
    println("Welcome to the shell!");
    println("This is a placeholder shell.");
    
    // TODO: Implement actual shell functionality
    
    exit(0);
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    println("Shell Panic!");
    exit(1);
}