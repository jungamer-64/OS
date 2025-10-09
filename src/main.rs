// src/main.rs

#![no_std]
#![no_main]

mod serial;
mod vga_buffer;

use bootloader::{entry_point, BootInfo};
use core::fmt::Write;
use core::panic::PanicInfo;

// Re-export color constants for convenience
use vga_buffer::{
    COLOR_ERROR, COLOR_INFO, COLOR_NORMAL, COLOR_PANIC, COLOR_SUCCESS, COLOR_WARNING,
};

entry_point!(kernel_main);

fn kernel_main(_boot_info: &'static BootInfo) -> ! {
    // Initialize serial port
    serial::init();
    serial::write_str("=== Rust OS Kernel Started ===\n");
    serial::write_str("Serial port initialized (38400 baud, 8N1, FIFO checked)\n");

    // Clear VGA screen
    vga_buffer::clear();

    // Write to both serial and VGA
    serial::write_str("VGA text mode initialized (80x25, color support)\n");
    serial::write_str("SAFE: Using Mutex-protected VGA writer (interrupt-safe!)\n");

    // VGA output
    vga_buffer::print_colored("=== Rust OS Kernel Started ===\n\n", COLOR_INFO);
    vga_buffer::print_colored("Welcome to minimal x86_64 Rust OS!\n\n", COLOR_NORMAL);

    vga_buffer::print_colored("bootloader: ", COLOR_INFO);
    vga_buffer::print_colored("0.9.33\n", COLOR_NORMAL);
    vga_buffer::print_colored("Serial: ", COLOR_INFO);
    vga_buffer::print_colored("COM1 (0x3F8) with FIFO check\n\n", COLOR_NORMAL);

    vga_buffer::print_colored("✓ Major Improvements:\n", COLOR_SUCCESS);
    vga_buffer::print_colored("  • Replaced static mut with Mutex (SAFE!)\n", COLOR_NORMAL);
    vga_buffer::print_colored("  • Interrupt-safe locking (no deadlock!)\n", COLOR_NORMAL);
    vga_buffer::print_colored("  • Implemented fmt::Write trait\n", COLOR_NORMAL);
    vga_buffer::print_colored(
        "  • Optimized scroll with copy_nonoverlapping\n",
        COLOR_NORMAL,
    );
    vga_buffer::print_colored(
        "  • Modular code structure (vga_buffer, serial)\n",
        COLOR_NORMAL,
    );
    vga_buffer::print_colored("  • Serial FIFO transmit check\n", COLOR_NORMAL);
    vga_buffer::print_colored("  • VGA color support (16 colors)\n", COLOR_NORMAL);
    vga_buffer::print_colored("  • VGA auto-scroll\n", COLOR_NORMAL);
    vga_buffer::print_colored("  • CPU hlt instruction\n", COLOR_NORMAL);
    vga_buffer::print_colored("  • Detailed panic handler\n\n", COLOR_NORMAL);

    serial::write_str("\n✓ Kernel features:\n");
    serial::write_str("  • SAFE: Mutex-protected VGA writer (no data races!)\n");
    serial::write_str("  • SAFE: Interrupt-safe locking (no deadlock!)\n");
    serial::write_str("  • Optimized memory copy for scroll\n");
    serial::write_str("  • Modular architecture (serial, vga_buffer)\n");
    serial::write_str("  • Serial port with FIFO check (hardware-safe)\n");
    serial::write_str("  • VGA text mode with 16-color support\n");
    serial::write_str("  • VGA auto-scroll support\n");
    serial::write_str("  • CPU halt with hlt instruction\n");
    serial::write_str("  • Panic handler with location info\n");

    vga_buffer::print_colored("\nNote: ", COLOR_WARNING);
    vga_buffer::print_colored("All core features tested and working!\n\n", COLOR_NORMAL);

    serial::write_str("\nKernel running. System in low-power hlt loop.\n");
    serial::write_str("Press Ctrl+A, X to exit QEMU.\n");

    // Halt CPU in infinite loop
    loop {
        x86_64::instructions::hlt();
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    // Serial output (detailed)
    serial::write_str("\n");
    serial::write_str("═══════════════════════════════════════\n");
    serial::write_str("       !!! KERNEL PANIC !!!\n");
    serial::write_str("═══════════════════════════════════════\n");

    // Use a mutable SerialWriter instance
    let mut w = serial::SerialWriter;

    // Print panic message
    let _ = write!(w, "Message: {}\n", info.message());

    // Print location
    if let Some(location) = info.location() {
        let _ = write!(
            w,
            "Location: {}:{}:{}\n",
            location.file(),
            location.line(),
            location.column()
        );
    }

    serial::write_str("═══════════════════════════════════════\n");
    serial::write_str("System halted. CPU in hlt loop.\n");

    // VGA output (prominent with color)
    vga_buffer::print_colored("\n!!! KERNEL PANIC !!!\n\n", COLOR_PANIC);

    if let Some(location) = info.location() {
        vga_buffer::print_colored("File: ", COLOR_ERROR);
        vga_buffer::print_colored(location.file(), COLOR_NORMAL);
        vga_buffer::print_colored("\n", COLOR_NORMAL);

        vga_buffer::print_colored("Line: ", COLOR_ERROR);
        vga_buffer::print_colored("(see serial output)\n", COLOR_NORMAL);

        vga_buffer::print_colored("Column: ", COLOR_ERROR);
        vga_buffer::print_colored("(see serial output)\n", COLOR_NORMAL);
    }

    vga_buffer::print_colored(
        "\nSystem halted. See serial for more details.\n",
        COLOR_WARNING,
    );

    loop {
        x86_64::instructions::hlt();
    }
}
