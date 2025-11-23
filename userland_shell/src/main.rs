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

    uprintln!("Starting CoW Test");
    
    let pid = syscall::sys_fork();
    if pid == 0 {
        uprintln!("I am the child!");
        
        // Trigger CoW on stack
        let mut x = 100;
        uprintln!("Child: x = {}", x);
        x = 200;
        uprintln!("Child: x modified to {}", x);
        
        uprintln!("Child exiting...");
        syscall::sys_exit(0);
    } else if pid > 0 {
        uprintln!("I am the parent, child PID: {}", pid);
        syscall::sys_wait(pid, None);
        uprintln!("Child terminated. Parent exiting.");
        syscall::sys_exit(0);
    } else {
        uprintln!("Fork failed!");
        syscall::sys_exit(1);
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    uprintln!("Shell Panic: {}", info);
    syscall::sys_exit(1);
}
