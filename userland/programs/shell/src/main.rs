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
    if let Err(_) = pipe(&mut pipefd) {
        println("Failed to create pipe");
        exit(1);
    }
    
    println("Pipe created successfully");
    
    // Fork
    let pid = match fork() {
        Ok(p) => p,
        Err(_) => {
            println("Fork failed!");
            exit(1);
        }
    };
    
    if pid == 0 {
        // Child process: write to pipe
        println("[Child] Writing to pipe...");
        
        let message = b"Hello from child process!";
        match write(pipefd[1], message) {
            Ok(n) => {
                println("[Child] Wrote bytes to pipe");
                n
            }
            Err(_) => {
                println("[Child] Write failed");
                exit(1);
            }
        };
        
        println("[Child] Exiting");
        exit(0);
    } else {
        // Parent process: read from pipe
        println("[Parent] Reading from pipe...");
        
        let mut buffer = [0u8; 64];
        match read(pipefd[0], &mut buffer) {
            Ok(n) if n > 0 => {
                println("[Parent] Read bytes from pipe");
                
                // Print as string
                if let Ok(_s) = core::str::from_utf8(&buffer[..n]) {
                    println("[Parent] Message received from child");
                }
            }
            _ => {
                println("[Parent] Read failed");
            }
        }
        
        // Wait for child
        let _ = wait(pid as i64, None);
        println("[Parent] Child terminated");
        println("\n=== Pipe Test Complete ===");
        exit(0);
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    println("Shell Panic!");
    exit(1);
}
