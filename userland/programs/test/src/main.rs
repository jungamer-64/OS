//! libuser API test program
//!
//! This program tests various libuser APIs

#![no_std]
#![no_main]

use libuser::{println, process, mem};

#[no_mangle]
pub extern "C" fn _start() -> ! {
    println!("=== libuser API Test ===");
    
    // Test 1: Process info
    test_process_info();
    
    // Test 2: Memory allocation
    test_memory();
    
    // Test 3: Fork/wait
    test_fork();
    
    println!("=== All tests completed ===");
    process::exit(0);
}

fn test_process_info() {
    println!("\n[Test] Process Info");
    let pid = process::getpid();
    println!("  PID: {}", pid);
}

fn test_memory() {
    println!("\n[Test] Memory Allocation");
    
    match mem::alloc(4096) {
        Ok(addr) => {
            println!("  Allocated 4KB at 0x{:x}", addr);
            
            // Test write
            unsafe {
                let ptr = addr as *mut u8;
                *ptr = 42;
                let val = *ptr;
                println!("  Write/read test: {}", if val == 42 { "PASS" } else { "FAIL" });
            }
            
            // Deallocate
            match mem::dealloc(addr, 4096) {
                Ok(()) => println!("  Deallocated successfully"),
                Err(_) => println!("  Deallocation FAILED"),
            }
        }
        Err(_) => println!("  Allocation FAILED"),
    }
}

fn test_fork() {
    println!("\n[Test] Fork/Wait");
    
    match process::fork() {
        Ok(0) => {
            // Child process
            println!("  Child: I am the child");
            process::exit(42);
        }
        Ok(child_pid) => {
            // Parent process
            println!("  Parent: Child PID = {}", child_pid);
            
            let mut status = 0i32;
            match process::wait(-1, Some(&mut status)) {
                Ok(pid) => {
                    println!("  Parent: Child {} exited with status {}", pid, status);
                },
                Err(_) => println!("  Parent: Wait FAILED"),
            }
        }
        Err(_) => println!("  Fork FAILED"),
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
