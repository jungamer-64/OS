#![no_std]
#![no_main]

use libuser::io::println;
use libuser::process::{fork, wait, exit};
use libuser::syscall::{pipe, read, write};
use core::panic::PanicInfo;

#[no_mangle]
#[link_section = ".text.entry"]
pub extern "C" fn _start() -> ! {
    println("Hello from Userland Shell!");
    println("=== Testing IPC Pipes ===\n");
    
    // Create a pipe
    let mut pipefd = [0u64; 2];
    let result = pipe(&mut pipefd);
    
    if result < 0 {
        println("Failed to create pipe");
        exit(1);
    }
    
    println("Pipe created successfully");
    
    // Fork
    let pid = fork();
    
    if pid == 0 {
        // Child process: write to pipe
        println("[Child] Writing to pipe...");
        
        let message = b"Hello from child process!";
        let written = write(pipefd[1], message);
        
        if written >= 0 {
            println("[Child] Wrote bytes to pipe");
        } else {
            println("[Child] Write failed");
        }
        
        println("[Child] Exiting");
        exit(0);
    } else if pid > 0 {
        // Parent process: read from pipe
        println("[Parent] Reading from pipe...");
        
        let mut buffer = [0u8; 64];
        let read_count = read(pipefd[0], &mut buffer);
        
        if read_count > 0 {
            println("[Parent] Read bytes from pipe");
            
            // Print as string
            if let Ok(_s) = core::str::from_utf8(&buffer[..read_count as usize]) {
                println("[Parent] Message received from child");
            }
        } else {
            println("[Parent] Read failed");
        }
        
        // Wait for child
        wait(pid, None);
        println("[Parent] Child terminated");
        println("\n=== Pipe Test Complete ===");
        exit(0);
    } else {
        println("Fork failed!");
        exit(1);
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    println("Shell Panic!");
    exit(1);
}
