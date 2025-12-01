//! Simple syscall test example for Phase 6
//!
//! Demonstrates basic system call testing

#![no_std]
#![no_main]

use libuser::{println, process, syscall, mem};

#[no_mangle]
pub extern "C" fn _start() -> ! {
    println!("=== System Call Tests ===\n");
    
    // Test 1: getpid
    test_getpid();
    
    // Test 2: mmap/munmap
    test_mmap();
    
    println!("\n=== All Tests Complete ===");
    process::exit(0);
}

fn test_getpid() {
    println!("[TEST] get PID");
    let pid = process::getpid();
    println!("  PID = {}", pid);
    if pid > 0 {
        println!("  [PASS]");
    } else {
        println!("  [FAIL]");
    }
}

fn test_mmap() {
    println!("\n[TEST] mmap/munmap");
    match mem::alloc(4096) {
        Ok(addr) => {
            println!("  Allocated at 0x{:x}", addr);
            
            // Test write
            unsafe {
                *(addr as *mut u64) = 0xDEADBEEF;
                let val = *(addr as *const u64);
                if val == 0xDEADBEEF {
                    println!("  Write/Read: [PASS]");
                } else {
                    println!("  Write/Read: [FAIL]");
                }
            }
            
            // Deallocate
            match mem::dealloc(addr, 4096) {
                Ok(()) => println!("  Dealloc: [PASS]"),
                Err(_) => println!("  Dealloc: [FAIL]"),
            }
        }
        Err(_) => println!("  [FAIL] Allocation failed"),
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    println!("PANIC!");
    loop {}
}
