//! Syscall Benchmark Program
//!
//! Measures the latency of the `getpid` system call using RDTSC.

#![no_std]
#![no_main]

use libuser::{println, process};

#[no_mangle]
pub extern "C" fn _start() -> ! {
    println!("=== Syscall Benchmark ===\n");

    // Warmup
    for _ in 0..1000 {
        process::getpid();
    }

    let iterations = 1_000_000;
    let start = read_tsc();
    
    for _ in 0..iterations {
        process::getpid();
    }
    
    let end = read_tsc();
    let total_cycles = end - start;
    let avg_cycles = total_cycles / iterations;

    println!("Total cycles: {}", total_cycles);
    println!("Iterations: {}", iterations);
    println!("Average cycles/syscall: {}", avg_cycles);

    process::exit(0);
}

#[inline(always)]
fn read_tsc() -> u64 {
    let rax: u64;
    let rdx: u64;
    unsafe {
        core::arch::asm!(
            "rdtsc",
            out("rax") rax,
            out("rdx") rdx,
            options(nomem, nostack)
        );
    }
    (rdx << 32) | rax
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    println!("PANIC!");
    loop {}
}
