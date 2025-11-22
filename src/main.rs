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
    
    // GDT/IDT初期化
    tiny_os::arch::x86_64::init_gdt();
    debug_println!("[OK] GDT initialized");
    tiny_os::arch::x86_64::init_idt();
    debug_println!("[OK] IDT initialized");
    
    // システムコール機構初期化
    tiny_os::arch::x86_64::syscall::init();

    // 物理メモリオフセットを保存（Phase 2で必要）
    if let Some(offset) = boot_info.physical_memory_offset.into_option() {
        tiny_os::kernel::mm::PHYS_MEM_OFFSET.store(offset, core::sync::atomic::Ordering::Relaxed);
        debug_println!("[OK] Physical memory offset initialized: 0x{:x}", offset);
    } else {
        // マッピングが失敗した場合や設定されていない場合
        // ここでパニックするか、あるいは後でエラーにするか
        debug_println!("[WARNING] Physical memory offset not provided by bootloader!");
    }

    // Framebuffer初期化とコンソール設定
    if let Some(framebuffer) = boot_info.framebuffer.as_mut() {
        let info = framebuffer.info();
        let buffer = framebuffer.buffer_mut();
        tiny_os::kernel::driver::framebuffer::init_framebuffer(info, buffer);
        
        if let Some(fb) = tiny_os::kernel::driver::framebuffer::FRAMEBUFFER.get() {
            let _ = tiny_os::kernel::driver::set_framebuffer_console(fb);
        }
        debug_println!("[OK] Console initialized");
    }
    
    // メモリ管理初期化
    let phys_mem_offset = boot_info.physical_memory_offset.into_option().unwrap_or(0);
    let virt_mem_offset = x86_64::VirtAddr::new(phys_mem_offset);
    
    let _mapper = unsafe { tiny_os::kernel::mm::paging::init(virt_mem_offset) };
    
    // グローバルフレームアロケータの初期化 (Phase 2)
    // 注意: BootInfoFrameAllocatorは一度しか初期化してはならない（同じ領域を指すため）
    let frame_allocator = unsafe {
        tiny_os::kernel::mm::frame::BootInfoFrameAllocator::init(&boot_info.memory_regions)
    };
    {
        let mut allocator = tiny_os::kernel::mm::allocator::BOOT_INFO_ALLOCATOR.lock();
        *allocator = Some(frame_allocator);
    }
    debug_println!("[OK] Paging & Global Frame Allocator initialized");

    // ヒープ初期化
    let (heap_start_phys, heap_size) = tiny_os::kernel::mm::init_heap(&boot_info.memory_regions)
        .expect("Heap initialization failed");
    
    let heap_start_virt = tiny_os::kernel::mm::VirtAddr::new((heap_start_phys.as_u64() + phys_mem_offset) as usize);
    
    unsafe {
        tiny_os::init_heap(heap_start_virt, heap_size)
            .expect("Heap initialization failed");
    }
    debug_println!("[OK] Heap initialized at 0x{:x} (Size: {} bytes)", heap_start_virt.as_usize(), heap_size.as_usize());
    
    // ウェルカムバナー
    println!("========================================");
    println!("  Tiny OS - Ideal Rust Kernel (UEFI)");
    println!("========================================");

    // ハードウェアタイマー初期化
    // SAFETY: PICの初期化はカーネル起動時に1回だけ実行される
    unsafe {
        tiny_os::arch::x86_64::pic::PICS.lock().initialize();
    }
    debug_println!("[OK] Hardware Timer initialized");

    // 割り込み有効化
    ArchCpu::enable_interrupts();
    debug_println!("[OK] Interrupts enabled");
    
    println!("[OK] Kernel initialized successfully!");
    
    // システムコール機構のテスト（カーネル空間から）
    #[cfg(debug_assertions)]
    {
        tiny_os::kernel::syscall::test_syscall_mechanism();
    }
    
    // ユーザーモード実行テスト（オプション）
    #[cfg(feature = "test_usermode")]
    {
        unsafe {
            tiny_os::kernel::usermode::test_usermode_execution();
        }
    }
    
    // メインループ
    loop {
        ArchCpu::halt();
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    use tiny_os::kernel::driver::{enter_panic, NORMAL, FIRST_PANIC};
    
    let panic_level = enter_panic();
    
    match panic_level {
        NORMAL => {
            // 初回パニック: 可能な限り情報を出力
            // NOTE: format_args! はスタック上で動作するため、
            // ヒープアロケーションは発生しない（安全）
            debug_println!("[KERNEL PANIC] {}", info);
        }
        FIRST_PANIC => {
            // 二重パニック: 最小限の情報のみ
            debug_println!("[DOUBLE PANIC]");
        }
        _ => {
            // 三重パニック以降: 何も出力しない（無限ループ防止）
        }
    }
    
    loop {
        ArchCpu::halt();
    }
}