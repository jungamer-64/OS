#![no_std]
#![no_main]

mod syscall;
mod console;

use core::panic::PanicInfo;

#[no_mangle]
#[link_section = ".text.entry"]
pub extern "C" fn _start() -> ! {
    uprintln!("Hello from Userland Shell!");
    uprintln!("Type 'exit' to quit.");

    loop {
        uprint!("> ");
        
        // Simple buffer for input
        // Since we don't have a heap yet, use a stack buffer
        // But stack is small (64KB mapped by kernel)
        
        // For now, just wait loop to simulate work
        // TODO: Implement sys_read
        
        // Just yield or wait
        // We don't have sys_yield, but we can sys_write
        
        // For testing, just print something and loop
        for _ in 0..10000000 {
            core::hint::spin_loop();
        }
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    uprintln!("Shell Panic: {}", info);
    syscall::sys_exit(1);
}
