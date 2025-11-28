//! Syscall Benchmark Program
//!
//! Compares the performance of different syscall strategies:
//! 
//! 1. **Standard syscall** - Traditional syscall via `syscall` instruction
//! 2. **Benchmark syscall (ID: 1000)** - Minimal overhead syscall
//! 3. **Fast I/O ring** - Syscall-less I/O via shared memory
//!
//! This helps evaluate the effectiveness of syscall optimization strategies.

#![no_std]
#![no_main]

use libuser::{println, process};
use libuser::syscall::{benchmark, benchmark_mode, fast_io_setup, fast_poll, fast_io_flags, rdtsc};

#[no_mangle]
pub extern "C" fn _start() -> ! {
    println!("╔════════════════════════════════════════════════════════════╗");
    println!("║         SYSCALL PERFORMANCE BENCHMARK SUITE                ║");
    println!("║                                                            ║");
    println!("║  Comparing syscall optimization strategies:                ║");
    println!("║  1. Standard syscall (getpid)                              ║");
    println!("║  2. Benchmark syscall (ID: 1000, minimal)                  ║");
    println!("║  3. Fast I/O ring (syscall-less)                           ║");
    println!("╚════════════════════════════════════════════════════════════╝\n");

    // Warmup phase
    println!("[Warmup] Running warmup iterations...");
    for _ in 0..10000 {
        process::getpid();
        let _ = benchmark(benchmark_mode::MINIMAL);
    }
    println!("[Warmup] Complete.\n");

    // Test 1: Standard syscall (getpid)
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("TEST 1: Standard syscall (getpid)");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    
    let iterations = 100_000u64;
    let start = rdtsc();
    
    for _ in 0..iterations {
        process::getpid();
    }
    
    let end = rdtsc();
    let total_cycles = end.saturating_sub(start);
    let avg_cycles = total_cycles / iterations;

    println!("  Iterations:     {}", iterations);
    println!("  Total cycles:   {}", total_cycles);
    println!("  Avg cycles:     {}", avg_cycles);
    println!();

    // Test 2: Benchmark syscall (minimal)
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("TEST 2: Benchmark syscall (ID: 1000, minimal)");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    
    let start = rdtsc();
    
    for _ in 0..iterations {
        let _ = benchmark(benchmark_mode::MINIMAL);
    }
    
    let end = rdtsc();
    let total_cycles_bench = end.saturating_sub(start);
    let avg_cycles_bench = total_cycles_bench / iterations;

    println!("  Iterations:     {}", iterations);
    println!("  Total cycles:   {}", total_cycles_bench);
    println!("  Avg cycles:     {}", avg_cycles_bench);
    println!();

    // Test 3: Benchmark with timestamp read
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("TEST 3: Benchmark syscall with timestamp");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    
    let start = rdtsc();
    
    for _ in 0..iterations {
        let _ = benchmark(benchmark_mode::TIMESTAMP);
    }
    
    let end = rdtsc();
    let total_cycles_ts = end.saturating_sub(start);
    let avg_cycles_ts = total_cycles_ts / iterations;

    println!("  Iterations:     {}", iterations);
    println!("  Total cycles:   {}", total_cycles_ts);
    println!("  Avg cycles:     {}", avg_cycles_ts);
    println!();

    // Test 4: Direct RDTSC (no syscall baseline)
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("TEST 4: Direct RDTSC (no syscall baseline)");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    
    let start = rdtsc();
    
    for _ in 0..iterations {
        let _ = rdtsc();
    }
    
    let end = rdtsc();
    let total_cycles_direct = end.saturating_sub(start);
    let avg_cycles_direct = total_cycles_direct / iterations;

    println!("  Iterations:     {}", iterations);
    println!("  Total cycles:   {}", total_cycles_direct);
    println!("  Avg cycles:     {}", avg_cycles_direct);
    println!();

    // Test 5: Fast I/O setup (if available)
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("TEST 5: Fast I/O Setup (syscall-less mode)");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    
    match fast_io_setup(fast_io_flags::SQPOLL) {
        Ok(ctx) => {
            println!("  Fast I/O context: 0x{:x}", ctx);
            println!("  SQPOLL mode enabled");
            
            // Test fast_poll
            let start = rdtsc();
            for _ in 0..iterations {
                let _ = fast_poll();
            }
            let end = rdtsc();
            let total_cycles_poll = end.saturating_sub(start);
            let avg_cycles_poll = total_cycles_poll / iterations;
            
            println!("  Fast poll avg:    {} cycles", avg_cycles_poll);
        }
        Err(e) => {
            println!("  Fast I/O setup failed: {}", e.description());
        }
    }
    println!();

    // Summary
    println!("╔════════════════════════════════════════════════════════════╗");
    println!("║                        SUMMARY                             ║");
    println!("╠════════════════════════════════════════════════════════════╣");
    println!("║ Standard getpid:           {:>8} cycles                 ║", avg_cycles);
    println!("║ Benchmark minimal:         {:>8} cycles                 ║", avg_cycles_bench);
    println!("║ Benchmark timestamp:       {:>8} cycles                 ║", avg_cycles_ts);
    println!("║ Direct RDTSC (baseline):   {:>8} cycles                 ║", avg_cycles_direct);
    println!("╠════════════════════════════════════════════════════════════╣");
    
    // Calculate syscall overhead
    let syscall_overhead = avg_cycles.saturating_sub(avg_cycles_direct);
    let bench_overhead = avg_cycles_bench.saturating_sub(avg_cycles_direct);
    
    println!("║ Syscall overhead:          {:>8} cycles                 ║", syscall_overhead);
    println!("║ Benchmark overhead:        {:>8} cycles                 ║", bench_overhead);
    
    if bench_overhead > 0 && syscall_overhead > bench_overhead {
        let improvement = ((syscall_overhead - bench_overhead) * 100) / syscall_overhead;
        println!("║ Improvement:               {:>8}%                       ║", improvement);
    }
    
    println!("╚════════════════════════════════════════════════════════════╝");

    process::exit(0);
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("PANIC: {:?}", info);
    loop {}
}
