//! io_uring test program
//!
//! Demonstrates the io_uring-style async I/O interface using the new high-level API.

#![no_std]
#![no_main]

use libuser::{println, process};
use libuser::async_io::{AsyncContext, AsyncOp, AsyncResult};

#[no_mangle]
pub extern "C" fn _start() -> ! {
    println!("=== io_uring Test (New API) ===\n");
    
    // Test 1: Setup io_uring via AsyncContext
    test_context_setup();
    
    // Test 2: Single NOP operation
    test_single_nop();
    
    // Test 3: Batch NOP operations
    test_batch_nop();
    
    // Test 4: Write operation
    test_write();
    
    // Test 5: Batch writes
    test_batch_write();
    
    println!("\n=== io_uring Tests Complete ===");
    process::exit(0);
}

fn test_context_setup() {
    println!("[TEST] AsyncContext::new()");
    
    match AsyncContext::new() {
        Ok(ctx) => {
            println!("  Context created successfully");
            println!("  Available slots: {}", ctx.available());
            println!("  [PASS]");
        }
        Err(e) => {
            println!("  Failed to create context: {}", e.code());
            println!("  [FAIL]");
        }
    }
}

fn test_single_nop() {
    println!("\n[TEST] Single NOP operation");
    
    let mut ctx = match AsyncContext::new() {
        Ok(c) => c,
        Err(e) => {
            println!("  Setup failed: {}", e.code());
            println!("  [SKIP]");
            return;
        }
    };
    
    // Submit a NOP with user_data = 0x12345678
    let ud = ctx.alloc_user_data();
    match ctx.submit(AsyncOp::nop(ud)) {
        Ok(_) => println!("  Submitted NOP with user_data={}", ud),
        Err(_) => {
            println!("  Submit failed");
            println!("  [FAIL]");
            return;
        }
    }
    
    // Flush (executes io_uring_enter)
    match ctx.flush() {
        Ok(n) => println!("  Flush returned {} completions", n),
        Err(e) => {
            println!("  Flush failed: {}", e.code());
            println!("  [FAIL]");
            return;
        }
    }
    
    // Get completion
    if let Some(result) = ctx.get_completion() {
        println!("  Completion: user_data={}, result={}", result.user_data, result.result);
        if result.is_ok() {
            println!("  [PASS]");
        } else {
            println!("  [FAIL]");
        }
    } else {
        println!("  No completion received");
        println!("  [FAIL]");
    }
}

fn test_batch_nop() {
    println!("\n[TEST] Batch NOP operations");
    
    let mut ctx = match AsyncContext::new() {
        Ok(c) => c,
        Err(e) => {
            println!("  Setup failed: {}", e.code());
            println!("  [SKIP]");
            return;
        }
    };
    
    // Submit 5 NOPs
    let ops: [AsyncOp; 5] = [
        AsyncOp::nop(100),
        AsyncOp::nop(101),
        AsyncOp::nop(102),
        AsyncOp::nop(103),
        AsyncOp::nop(104),
    ];
    
    match ctx.submit_batch(&ops) {
        Ok(n) => println!("  Submitted {} operations", n),
        Err(_) => {
            println!("  Batch submit failed");
            println!("  [FAIL]");
            return;
        }
    }
    
    // Flush all
    match ctx.flush() {
        Ok(n) => println!("  Flush returned {} completions", n),
        Err(e) => {
            println!("  Flush failed: {}", e.code());
            println!("  [FAIL]");
            return;
        }
    }
    
    // Drain completions
    let mut count = 0;
    ctx.drain_completions(|result| {
        println!("    Completion {}: user_data={}, result={}", count, result.user_data, result.result);
        count += 1;
    });
    
    if count == 5 {
        println!("  [PASS]");
    } else {
        println!("  Expected 5 completions, got {}", count);
        println!("  [FAIL]");
    }
}

fn test_write() {
    println!("\n[TEST] Single write operation");
    
    let mut ctx = match AsyncContext::new() {
        Ok(c) => c,
        Err(e) => {
            println!("  Setup failed: {}", e.code());
            println!("  [SKIP]");
            return;
        }
    };
    
    let message = b"Hello from io_uring async API!\n";
    let ud = ctx.alloc_user_data();
    
    match ctx.submit(AsyncOp::write(1, message, ud)) {
        Ok(_) => {}
        Err(_) => {
            println!("  Submit failed");
            println!("  [FAIL]");
            return;
        }
    }
    
    match ctx.flush() {
        Ok(_) => {}
        Err(e) => {
            println!("  Flush failed: {}", e.code());
            println!("  [FAIL]");
            return;
        }
    }
    
    if let Some(result) = ctx.get_completion() {
        if result.is_ok() {
            println!("  Wrote {} bytes", result.result);
            println!("  [PASS]");
        } else {
            println!("  Write failed: {}", result.result);
            println!("  [FAIL]");
        }
    } else {
        println!("  No completion");
        println!("  [FAIL]");
    }
}

fn test_batch_write() {
    println!("\n[TEST] Batch write operations");
    
    let mut ctx = match AsyncContext::new() {
        Ok(c) => c,
        Err(e) => {
            println!("  Setup failed: {}", e.code());
            println!("  [SKIP]");
            return;
        }
    };
    
    // Submit multiple writes with single syscall
    let msgs: [&[u8]; 4] = [
        b"[Batch 1] ",
        b"Multiple ",
        b"writes with ",
        b"one syscall!\n",
    ];
    
    for (i, msg) in msgs.iter().enumerate() {
        match ctx.submit(AsyncOp::write(1, msg, i as u64)) {
            Ok(_) => {}
            Err(_) => {
                println!("  Submit {} failed", i);
                println!("  [FAIL]");
                return;
            }
        }
    }
    
    println!("  Submitted {} writes", msgs.len());
    println!("  Pending: {}", ctx.pending());
    
    // Single syscall to process all
    match ctx.flush() {
        Ok(n) => println!("  Flush processed {} completions", n),
        Err(e) => {
            println!("  Flush failed: {}", e.code());
            println!("  [FAIL]");
            return;
        }
    }
    
    // Count successful completions
    let mut success = 0;
    let mut total_bytes = 0i32;
    ctx.drain_completions(|result| {
        if result.is_ok() {
            success += 1;
            total_bytes += result.result;
        }
    });
    
    if success == 4 {
        println!("  Total bytes written: {}", total_bytes);
        println!("  [PASS]");
    } else {
        println!("  Expected 4 successes, got {}", success);
        println!("  [FAIL]");
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("PANIC: {:?}", info);
    loop {}
}
