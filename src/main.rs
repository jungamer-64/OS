// src/main.rs
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
use tiny_os::{println, debug_println};
use core::panic::PanicInfo;
use tiny_os::arch::{Cpu, ArchCpu};

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
    debug_println!("[KERNEL] Entry point reached");
    tiny_os::arch::x86_64::init_gdt();
    debug_println!("[KERNEL] GDT initialized");
    tiny_os::arch::x86_64::init_idt();
    debug_println!("[KERNEL] IDT initialized");

    if let Some(framebuffer) = boot_info.framebuffer.as_mut() {
        let info = framebuffer.info();
        let buffer = framebuffer.buffer_mut();
        tiny_os::kernel::driver::framebuffer::init_framebuffer(info, buffer);
        debug_println!("[KERNEL] Framebuffer initialized");
    }
    
    // コンソール抽象化レイヤーを初期化
    tiny_os::kernel::driver::init_console();
    debug_println!("[KERNEL] Console abstraction layer initialized");

    debug_println!("[OK] GDT initialized");
    debug_println!("[OK] IDT initialized");
    
    let phys_mem_offset = boot_info.physical_memory_offset.into_option().unwrap_or(0);
    let virt_mem_offset = x86_64::VirtAddr::new(phys_mem_offset);
    
    // ページング初期化
    let _mapper = unsafe { tiny_os::kernel::mm::paging::init(virt_mem_offset) };
    
    // フレームアロケータ初期化
    let _frame_allocator = unsafe {
        tiny_os::kernel::mm::frame::BootInfoFrameAllocator::init(&boot_info.memory_regions)
    };
    debug_println!("[OK] Paging & Frame Allocator initialized");

    // ヒープ初期化
    if let Ok((heap_start_phys, heap_size)) = tiny_os::kernel::mm::init_heap(&boot_info.memory_regions) {
        let heap_start_virt = tiny_os::kernel::mm::VirtAddr::new(heap_start_phys.as_usize() + phys_mem_offset as usize);
        // SAFETY: init_heapで取得した有効な領域を仮想アドレスに変換して使用
        match unsafe { tiny_os::init_heap(heap_start_virt, heap_size) } {
            Ok(()) => debug_println!("[OK] Heap initialized"),
            Err(tiny_os::HeapError::AlreadyInitialized) => {
                debug_println!("[WARN] Heap already initialized");
            }
        }
    } else {
        debug_println!("[FAIL] Heap initialization failed");
    }
    
    // これ以降でprintln!を安全に使える
    println!("========================================");
    println!("  Tiny OS - Ideal Rust Kernel (UEFI)");
    println!("========================================");


    debug_println!("Initializing Hardware Timer...");
    // SAFETY: PICの初期化はカーネル起動時に1回だけ実行される。
    // 割り込みコントローラへのアクセスは排他制御されている。
    unsafe {
        tiny_os::arch::x86_64::pic::PICS.lock().initialize();
    }
    debug_println!("[OK] Hardware Timer initialized (PIT disabled for debugging)");

    debug_println!("Enabling Interrupts...");
    ArchCpu::enable_interrupts();
    debug_println!("[OK] Interrupts enabled");
    
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