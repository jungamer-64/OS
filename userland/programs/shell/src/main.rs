//! Shell Program - io_uring Test Version
//! 
//! Temporarily using shell to test io_uring high-level API

#![no_std]
#![no_main]

use libuser::io::println;
use libuser::process::exit;
use libuser::async_io::{AsyncContext, AsyncOp};
use core::panic::PanicInfo;

#[no_mangle]
#[link_section = ".text.entry"]
pub extern "C" fn _start() -> ! {
    println("=== io_uring Test (Shell) ===");
    
    test_context_setup();
    test_single_nop();
    test_write();
    
    println("=== io_uring Tests Complete ===");
    exit(0);
}

fn test_context_setup() {
    println("[TEST] AsyncContext::new()");
    
    match AsyncContext::new() {
        Ok(ctx) => {
            println("  Context created successfully");
            println("  [PASS]");
            drop(ctx);
        }
        Err(e) => {
            println("  [FAIL] Context creation failed");
            exit(1);
        }
    }
}

fn test_single_nop() {
    println("[TEST] Single NOP operation");
    
    let mut ctx = match AsyncContext::new() {
        Ok(c) => c,
        Err(_) => {
            println("  [SKIP] Setup failed");
            return;
        }
    };
    
    let ud = ctx.alloc_user_data();
    match ctx.submit(AsyncOp::nop(ud)) {
        Ok(_) => println("  Submitted NOP"),
        Err(_) => {
            println("  [FAIL] Submit failed");
            return;
        }
    }
    
    match ctx.flush() {
        Ok(n) => println("  Flush OK"),
        Err(_) => {
            println("  [FAIL] Flush failed");
            return;
        }
    }
    
    if let Some(result) = ctx.get_completion() {
        if result.is_ok() {
            println("  [PASS]");
        } else {
            println("  [FAIL] Bad result");
        }
    } else {
        println("  [FAIL] No completion");
    }
}

fn test_write() {
    println("[TEST] Single write operation");
    
    let mut ctx = match AsyncContext::new() {
        Ok(c) => c,
        Err(_) => {
            println("  [SKIP] Setup failed");
            return;
        }
    };
    
    let message = b"Hello from io_uring!\n";
    let ud = ctx.alloc_user_data();
    
    match ctx.submit(AsyncOp::write(1, message, ud)) {
        Ok(_) => {}
        Err(_) => {
            println("  [FAIL] Submit failed");
            return;
        }
    }
    
    match ctx.flush() {
        Ok(_) => {}
        Err(_) => {
            println("  [FAIL] Flush failed");
            return;
        }
    }
    
    if let Some(result) = ctx.get_completion() {
        if result.is_ok() {
            println("  [PASS]");
        } else {
            println("  [FAIL] Write failed");
        }
    } else {
        println("  [FAIL] No completion");
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    println("Shell Panic!");
    exit(1);
}
