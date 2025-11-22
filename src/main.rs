//! Tiny OS - 理想的な Rust カーネル
//!
//! trait ベースの抽象化と型安全性を最大化したカーネルアーキテクチャ

#![no_std]
#![no_main]
#![deny(unsafe_op_in_unsafe_fn)]
#![warn(missing_docs)]
#![allow(missing_docs)]

use bootloader_api::{entry_point, BootInfo, BootloaderConfig};
use bootloader_api::config::Mapping;
use tiny_os::println;
use core::panic::PanicInfo;
use tiny_os::arch::{Cpu, ArchCpu};
use x86_64::instructions::port::PortWriteOnly;

/// Bootloader configuration.
pub static BOOTLOADER_CONFIG: BootloaderConfig = {
    let mut config = BootloaderConfig::new_default();
    // フレームバッファと全物理メモリを仮想アドレス空間に動的にマッピングするよう要求
    config.mappings.framebuffer = Mapping::Dynamic;
    config.mappings.physical_memory = Some(Mapping::Dynamic);
    config
};

entry_point!(kernel_main, config = &BOOTLOADER_CONFIG);

fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    macro_rules! serial_print {
        ($msg:expr) => {
            // SAFETY: 0x3F8はCOM1シリアルポートの標準アドレス。初期初期化コードで使用し、
            // 他のデバイスが初期化される前にデバッグ出力を行うため。
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

    let phys_mem_offset = boot_info.physical_memory_offset.into_option().unwrap_or(0);

    if let Some(framebuffer) = boot_info.framebuffer.as_mut() {
        let info = framebuffer.info();
        let buffer = framebuffer.buffer_mut();
        tiny_os::kernel::driver::framebuffer::init_framebuffer(info, buffer);
        serial_print!(b"[KERNEL] Framebuffer initialized\n");
    }

    serial_print!(b"[OK] GDT initialized\n");
    serial_print!(b"[OK] IDT initialized\n");
    
    let phys_mem_offset = boot_info.physical_memory_offset.into_option().unwrap_or(0);
    let virt_mem_offset = x86_64::VirtAddr::new(phys_mem_offset);
    
    // ページング初期化
    let mut mapper = unsafe { tiny_os::kernel::mm::paging::init(virt_mem_offset) };
    
    // フレームアロケータ初期化
    let mut frame_allocator = unsafe {
        tiny_os::kernel::mm::frame::BootInfoFrameAllocator::init(&boot_info.memory_regions)
    };
    serial_print!(b"[OK] Paging & Frame Allocator initialized\n");

    // ヒープ初期化
    if let Ok((heap_start_phys, heap_size)) = tiny_os::kernel::mm::init_heap(&boot_info.memory_regions) {
        let heap_start_virt = heap_start_phys + phys_mem_offset as usize;
        // SAFETY: init_heapで取得した有効な領域を仮想アドレスに変換して使用
        unsafe {
            tiny_os::init_heap(heap_start_virt, heap_size);
        }
        serial_print!(b"[OK] Heap initialized\n");
    } else {
        serial_print!(b"[FAIL] Heap initialization failed\n");
    }
    
    // これ以降でprintln!を安全に使える
    println!("========================================");
    println!("  Tiny OS - Ideal Rust Kernel (UEFI)");
    println!("========================================");


    serial_print!(b"Initializing Hardware Timer...\n");
    // SAFETY: PICの初期化はカーネル起動時に1回だけ実行される。
    // 割り込みコントローラへのアクセスは排他制御されている。
    unsafe {
        tiny_os::arch::x86_64::pic::PICS.lock().initialize();
    }
    serial_print!(b"[OK] Hardware Timer initialized (PIT disabled for debugging)\n");

    serial_print!(b"Enabling Interrupts...\n");
    ArchCpu::enable_interrupts();
    serial_print!(b"[OK] Interrupts enabled\n");
    
    println!("[OK] Kernel initialized successfully!");
    
    loop {
        ArchCpu::halt();
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    // パニック時もprintln!を使えるようにする（シリアル出力のため）
    println!("[KERNEL PANIC] {}", info);
    loop {
        ArchCpu::halt();
    }
}