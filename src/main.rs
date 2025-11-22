//! Tiny OS - 理想的な Rust カーネル
//!
//! trait ベースの抽象化と型安全性を最大化したカーネルアーキテクチャ
//!
//! # 機能
//!
//! - デバイス trait による統一的なドライバインターフェース
//! - 型安全な MMIO とポート I/O
//! - リンクリストヒープアロケータ
//! - ビットマップフレームアロケータ
//! - Future ベースの非同期処理基盤
//!
//! # アーキテクチャ
//!
//! - `kernel/core` - trait, types, result, prelude
//! - `kernel/driver` - デバイスドライバ (Serial, VGA, Keyboard)
//! - `kernel/mm` - メモリ管理 (paging, allocator, frame)
//! - `kernel/async` - 非同期処理 (executor, waker, timer)
//! - `arch/x86_64` - アーキテクチャ依存コード

#![no_std]
#![no_main]
#![deny(unsafe_op_in_unsafe_fn)]
#![warn(missing_docs)]
#![allow(missing_docs)]

use tiny_os::kernel::driver::init_vga;
use tiny_os::println;
use tiny_os::constants::{HEAP_START, HEAP_SIZE};
use bootloader_api::{entry_point, BootInfo};
use core::panic::PanicInfo;
use core::fmt::Write;
use tiny_os::arch::{Cpu, ArchCpu};

entry_point!(kernel_main);

fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    // まず最初にGDT/IDTを初期化（VGAより前）
    tiny_os::arch::x86_64::init_gdt();
    tiny_os::arch::x86_64::init_idt();
    
    // VGA を初期化
    init_vga().expect("Failed to initialize VGA");
    
    // ヘッダー表示
    println!("========================================");
    println!("  Tiny OS - Ideal Rust Kernel");
    println!("========================================");
    println!();

    // 初期化完了メッセージ
    println!("[OK] GDT initialized");
    println!("[OK] IDT initialized");
    println!("[OK] VGA initialized");

    // ヒープ初期化
    println!("Initializing Heap...");
    unsafe {
        tiny_os::init_heap(HEAP_START, HEAP_SIZE);
    }
    println!("[OK] Heap initialized");

    // 割り込み有効化
    println!("Enabling Interrupts...");
    ArchCpu::enable_interrupts();
    println!("[OK] Interrupts enabled");
    
    // カーネル情報表示
    println!("Kernel Information:");
    println!("  - Architecture: x86_64");
    println!("  - Boot Protocol: UEFI");
    println!("  - Allocator: LinkedList (Global)");
    println!("  - Async Runtime: Future Executor");
    println!();
    
    // ブート情報表示
    println!("Boot Information:");
    if let Some(framebuffer) = boot_info.framebuffer.as_ref() {
        let info = framebuffer.info();
        println!("  - Framebuffer: {}x{}", info.width, info.height);
        println!("  - Pixel Format: {:?}", info.pixel_format);
    }
    if let Some(rsdp_addr) = boot_info.rsdp_addr.into_option() {
        println!("  - RSDP Address: {:#x}", rsdp_addr);
    }
    println!();
    
    // アーキテクチャ情報表示
    println!("Architecture Features:");
    println!("  - trait-based Device abstraction");
    println!("  - Type-safe Port I/O and MMIO");
    println!("  - Lifetime-based resource management");
    println!("  - Prelude module for ergonomics");
    println!();
    
    // モジュール情報表示
    println!("Kernel Modules:");
    println!("  [✓] kernel/core     - Traits, Types, Result");
    println!("  [✓] kernel/driver   - Serial, VGA, Keyboard");
    println!("  [✓] kernel/mm       - Paging, Allocator, Frame");
    println!("  [✓] kernel/async    - Executor, Waker, Timer");
    println!("  [✓] arch/x86_64     - Port I/O");
    println!();
    
    // 成功メッセージ
    println!("========================================");
    println!("  Kernel initialized successfully!");
    println!("========================================");
    println!();
    println!("Entering halt loop...");
    
    // Halt loop
    loop {
        ArchCpu::halt();
    }
}

/// Panic handler
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    use tiny_os::arch::Cpu;
    
    // 割り込みを無効化
    ArchCpu::disable_interrupts();
    
    // VGA が初期化されていれば使用
    if let Some(vga) = tiny_os::kernel::driver::vga::VGA.get() {
        let _ = writeln!(vga.lock(), "\n\n[KERNEL PANIC]\n{}", info);
    }
    
    // Halt loop
    loop {
        ArchCpu::halt();
    }
}
