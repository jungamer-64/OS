#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(tiny_os::test_runner)]
#![reexport_test_harness_main = "test_main"]

use bootloader_api::{entry_point, BootInfo};
use core::panic::PanicInfo;
use tiny_os::{println, serial_println};

entry_point!(test_kernel_main);

fn test_kernel_main(_boot_info: &'static mut BootInfo) -> ! {
    tiny_os::init::initialize_all().unwrap();
    test_main();
    tiny_os::hlt_loop();
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    tiny_os::test_panic_handler(info)
}

#[test_case]
fn test_println() {
    println!("test_println output");
}
