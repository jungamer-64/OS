#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(tiny_os::test_runner)]
#![reexport_test_harness_main = "test_main"]

use bootloader::{entry_point, BootInfo};
use core::panic::PanicInfo;
use tiny_os::vga_buffer::{ColorCode, DoubleBufferedWriter, CELL_COUNT, VGA_WIDTH};
use tiny_os::{exit_qemu, hlt_loop, init, println, serial_println, test_panic_handler};
use tiny_os::{serial, vga_buffer, QemuExitCode};

extern "Rust" {
    fn test_main();
}

entry_point!(test_kernel_main);

fn test_kernel_main(_boot_info: &'static BootInfo) -> ! {
    if let Err(err) = init::initialize_all() {
        serial_println!("[TEST INIT] initialization failed: {:?}", err);
        exit_qemu(QemuExitCode::Failed);
    }

    unsafe {
        test_main();
    }
    hlt_loop();
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    test_panic_handler(info)
}

#[test_case]
fn serial_vga_lock_integration() {
    serial::log_lines(["[TEST] serial_vga_lock start"].iter().copied());
    vga_buffer::print_colored("[TEST] VGA output active\n", ColorCode::success())
        .expect("VGA output should be available during test");
    serial_println!("[TEST] serial_vga_lock complete");
}

#[test_case]
fn continuous_output_stability() {
    for iteration in 0..128u32 {
        println!("[VGA] iteration {:03}", iteration);
        serial_println!("[SERIAL] iteration {:03}", iteration);
    }
}

#[test_case]
fn double_buffer_swap_present() {
    let mut buffer = DoubleBufferedWriter::new();
    let color = ColorCode::success();
    let encoded = ((color.as_u8() as u16) << 8) | b'D' as u16;

    let cols = VGA_WIDTH.min(8);
    for idx in 0..cols {
        buffer
            .write_cell(idx, encoded)
            .expect("write_cell must succeed for valid indices");
    }

    let updated = buffer
        .swap_buffers()
        .expect("swap_buffers should succeed with valid dirty data");
    assert!(updated >= cols);

    let frame = [encoded; CELL_COUNT];
    buffer.stage_frame(&frame);
    let full = buffer
        .swap_buffers()
        .expect("swap_buffers should succeed after staging frame");
    assert_eq!(full, CELL_COUNT);
}
