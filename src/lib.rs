//! Tiny OS - 理想的な Rust カーネル
//!
//! trait ベースの抽象化と型安全性を最大化したカーネルアーキテクチャ

#![no_std]
#![feature(abi_x86_interrupt)]
#![cfg_attr(test, no_main)]
#![cfg_attr(test, feature(custom_test_frameworks))]
#![cfg_attr(test, test_runner(crate::test_runner))]
#![cfg_attr(test, reexport_test_harness_main = "test_main")]
#![deny(unsafe_op_in_unsafe_fn)]
#![warn(missing_docs)]
#![allow(missing_docs)]

extern crate alloc;

pub mod errors;
pub mod qemu;
pub mod arch;
pub mod kernel;

use core::panic::PanicInfo;
use crate::arch::{Cpu, ArchCpu};

// グローバルヒープアロケータ
#[global_allocator]
static ALLOCATOR: kernel::mm::LockedHeap = kernel::mm::LockedHeap::new();

/// ヒープを初期化
///
/// # Safety
///
/// heap_start と heap_size は有効なヒープ領域を指している必要があります。
/// この関数は一度だけ呼ばれる必要があります。
pub unsafe fn init_heap(heap_start: usize, heap_size: usize) {
    unsafe {
        ALLOCATOR.init(heap_start, heap_size);
    }
}

pub use qemu::{exit_qemu, QemuExitCode};

/// println! マクロ - 新 VGA ドライバを使用
#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}

/// print! マクロ - 新 VGA ドライバを使用
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        // まず Framebuffer を試す
        if let Some(fb) = $crate::kernel::driver::framebuffer::FRAMEBUFFER.get() {
            // NOTE: print!マクロでの書き込みエラーは無視する（標準の挙動）
            let _ = write!(fb.lock(), $($arg)*);
        }
        // 次に VGA を試す（UEFI では無効だが念のため）
        else if let Some(vga) = $crate::kernel::driver::vga::VGA.get() {
            // NOTE: print!マクロでの書き込みエラーは無視する（標準の挙動）
            let _ = write!(vga.lock(), $($arg)*);
        }
        // シリアルポートにも出力（デバッグ用）
        {
            use $crate::kernel::driver::serial::SERIAL1;
            let mut serial = SERIAL1.lock();
            // NOTE: print!マクロでの書き込みエラーは無視する（標準の挙動）
            let _ = write!(serial, $($arg)*);
        }
    }};
}

/// Halt loop
#[inline]
pub fn hlt_loop() -> ! {
    loop {
        ArchCpu::halt();
    }
}

/// Test trait
pub trait Testable {
    fn run(&self);
}

impl<T> Testable for T
where
    T: Fn(),
{
    fn run(&self) {
        print!("[TEST] {} ... ", core::any::type_name::<T>());
        self();
        println!("ok");
    }
}

/// Test runner
pub fn test_runner(tests: &[&dyn Testable]) {
    println!("[TEST RUNNER] running {} tests", tests.len());
    for test in tests {
        test.run();
    }
    exit_qemu(QemuExitCode::Success);
}

/// Test panic handler
#[cfg(all(test, feature = "std-tests"))]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    test_panic_handler(info)
}

#[inline(never)]
pub fn test_panic_handler(info: &PanicInfo) -> ! {
    println!("[TEST PANIC] {}", info);
    exit_qemu(QemuExitCode::Failed);
}
