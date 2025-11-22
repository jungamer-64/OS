//! Tiny OS - 理想的な Rust カーネル
//!
//! trait ベースの抽象化と型安全性を最大化したカーネルアーキテクチャ

#![no_std]
#![no_main]
#![deny(unsafe_op_in_unsafe_fn)]
#![warn(missing_docs)]
#![allow(missing_docs)]

use bootloader_api::{entry_point, BootInfo};
use core::panic::PanicInfo;
use tiny_os::arch::{Cpu, ArchCpu};
use tiny_os::println;
use x86_64::instructions::port::PortWriteOnly;

entry_point!(kernel_main);

fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    macro_rules! serial_print {
        ($msg:expr) => {
            unsafe {
                let mut serial = PortWriteOnly::<u8>::new(0x3F8);
                for byte in $msg {
                    serial.write(*byte);
                }
            }
        };
    }

    serial_print!(b"[KERNEL] Entry point reached\n");
    tiny_os::arch::x86_64::init_gdt();
    serial_print!(b"[KERNEL] GDT initialized\n");
    tiny_os::arch::x86_64::init_idt();
    serial_print!(b"[KERNEL] IDT initialized\n");

    if let Some(framebuffer) = boot_info.framebuffer.as_mut() {
        let info = framebuffer.info();
        let buffer = framebuffer.buffer_mut();
        tiny_os::kernel::driver::framebuffer::init_framebuffer(info, buffer);
        serial_print!(b"[KERNEL] Framebuffer initialized\n");
    }

    serial_print!(b"========================================\n");
    serial_print!(b"  Tiny OS - Ideal Rust Kernel (UEFI)\n");
    serial_print!(b"========================================\n");
    serial_print!(b"[OK] GDT initialized\n");
    serial_print!(b"[OK] IDT initialized\n");
    serial_print!(b"[SKIP] Heap initialization (need boot_info memory map)\n");
    
    serial_print!(b"Initializing Hardware Timer...\n");
    unsafe {
        tiny_os::arch::x86_64::pic::PICS.lock().initialize();
        // tiny_os::kernel::driver::pit::PIT.lock().set_frequency(100).expect("Failed to set PIT frequency");
        // tiny_os::arch::x86_64::pic::PICS.lock().unmask_irq(0);
    }
    serial_print!(b"[OK] Hardware Timer initialized (PIT disabled for debugging)\n");

    serial_print!(b"Enabling Interrupts...\n");
    ArchCpu::enable_interrupts();
    serial_print!(b"[OK] Interrupts enabled\n");
    
    serial_print!(b"[OK] Kernel initialized successfully!\n");
    
    loop {
        ArchCpu::halt();
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    ArchCpu::disable_interrupts();
    unsafe {
        let mut serial = PortWriteOnly::<u8>::new(0x3F8);
        for byte in b"\n\n[KERNEL PANIC]\n" {
            serial.write(*byte);
        }
    }
    loop {
        ArchCpu::halt();
    }
}
