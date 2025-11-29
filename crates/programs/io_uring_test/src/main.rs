//! io_uring test program
//!
//! Demonstrates the io_uring-style async I/O interface using the new V2 API.

#![no_std]
#![no_main]

use libuser::{print, println, process};
use libuser::ring_io::{Ring, Sqe, Opcode};

#[no_mangle]
pub extern "C" fn _start() -> ! {
    println!("=== io_uring Test (V2 API) ===\n");
    
    test_ring_setup();
    
    test_ring_sqpoll_doorbell();
    
    println!("\n=== io_uring Tests Complete ===");
    process::exit(0);
}

fn test_ring_setup() {
    println!("[TEST] Ring::setup() - New API (syscall 2002)");
    println!("  Calling Ring::setup(false)...");
    
    // Test the new Ring API (syscall 2002)
    match Ring::setup(false) {
        Ok(mut ring) => {
            println!("  Ring created successfully");
            
            // Debug: Check addresses
            println!("  Ring base at default address");
            
            // Submit a NOP directly
            println!("  Submitting NOP...");
            let sqe = Sqe::nop(0x12345678);
            match ring.submit(sqe) {
                Ok(_idx) => {
                    println!("  Submitted NOP");
                    
                    // Enter to process
                    println!("  Calling enter()...");
                    match ring.enter() {
                        Ok(_) => {
                            println!("  Enter succeeded");
                            // Wait for completion
                            if ring.has_completions() {
                                let cqe = ring.wait_cqe();
                                if cqe.user_data == 0x12345678 {
                                    println!("  Got correct completion");
                                    println!("  [PASS]");
                                }
                            }
                        }
                        Err(e) => {
                            print!("  Enter failed: ");
                            print_error(e.code());
                            println!("  [FAIL]");
                        }
                    }
                }
                Err(e) => {
                    print!("  Submit failed: errno=");
                    print_error(e.code());
                    println!("  [FAIL]");
                }
            }
        }
        Err(e) => {
            print!("  Ring setup failed: ");
            print_error(e.code());
            println!("  [FAIL]");
        }
    }
}

fn test_ring_sqpoll_doorbell() {
    println!("[TEST] Ring::setup(true) - SQPOLL + Doorbell (Zero-syscall)");
    println!("[DEBUG] Entering test_ring_sqpoll_doorbell()") ;
    println!("  Calling Ring::setup(true)...");
    match Ring::setup(true) {
        Ok(mut ring) => {
            println!("  Ring (SQPOLL) created successfully");
            let ud = 0xABCDu64;
            let sqe = Sqe::nop(ud);

            match ring.submit(sqe) {
                Ok(_) => println!("  Submitted NOP - now ring the doorbell (no syscall)..."),
                Err(_) => {
                    println!("  Submit failed");
                    println!("  [FAIL]");
                    return;
                }
            }

            ring.ring_doorbell(); // No syscall

            // Wait for kernel to set CQ ready via doorbell (poll)
            let mut attempts: u32 = 0;
            while !ring.check_cq_ready() && attempts < 100_000 {
                attempts += 1;
                core::hint::spin_loop();
            }

            if !ring.check_cq_ready() {
                println!("  SQPOLL did not set cq_ready after doorbell ring");
                println!("  [FAIL]");
                return;
            }

            ring.clear_cq_ready();
            if let Some(cqe) = ring.try_get_cqe() {
                if cqe.user_data == ud {
                    println!("  Received expected completion (user_data={})", ud);
                    println!("  [PASS]");
                    return;
                } else {
                    println!("  Completion user_data mismatch");
                    println!("  [FAIL]");
                    return;
                }
            } else {
                println!("  No CQE found even though cq_ready flag was set");
                println!("  [FAIL]");
                return;
            }
        }
        Err(e) => {
            print!("  Ring setup failed: ");
            print_error(e.code());
            println!("  [FAIL]");
        }
    }
}

fn print_error(code: i64) {
    // Print error code without using {} formatting
    if code == 11 {
        println!("EAGAIN (11)");
    } else if code == 14 {
        println!("EFAULT (14)");
    } else if code == 12 {
        println!("ENOMEM (12)");
    } else if code == 22 {
        println!("EINVAL (22)");
    } else {
        println!("unknown error");
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("PANIC: {:?}", info);
    loop {}
}