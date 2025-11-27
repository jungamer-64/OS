//! Init process
//!
//! This is the first process started by the kernel.
//! It spawns the shell and reaps zombie processes.

#![no_std]
#![no_main]

use libuser::{println, process};

#[no_mangle]
pub extern "C" fn _start() -> ! {
    println!("[init] Starting init process (PID={})", process::getpid());
    
    // Spawn shell
    println!("[init] Spawning shell...");
    match process::spawn("/bin/shell") {
        Ok(pid) => println!("[init] Shell spawned with PID={}", pid),
        Err(e) => {
            println!("[init] Failed to spawn shell: {:?}", e);
            // If shell fails, we can't do much but loop
        }
    }
    
    println!("[init] Entering main loop");
    
    loop {
        // Wait for any child process to exit (reap zombies)
        // In a real OS, we would use waitpid(-1, ...) blocking
        // Since our wait is blocking, this is fine
        let mut status = 0;
        match process::wait(-1, Some(&mut status)) {
            Ok(pid) => {
                println!("[init] Child {} exited with status {}", pid, status);
                
                // If shell exited, restart it?
                // For now, just log it
            },
            Err(_) => {
                // No children or error
                // Sleep a bit to avoid busy loop if wait returns immediately on error
                // (Our wait currently returns ECHILD if no children)
                
                // TODO: Implement sleep syscall
                // For now, just spin a bit
                for _ in 0..1000000 {
                    core::hint::spin_loop();
                }
            }
        }
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("[init] PANIC: {}", info);
    loop {}
}
