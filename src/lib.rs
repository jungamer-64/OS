// src/lib.rs
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

/// ヒープ初期化エラー
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeapError {
    /// 既に初期化済み
    AlreadyInitialized,
}

/// ヒープを初期化
///
/// この関数は、ヒープの初期化を一度だけ実行することを保証します。
/// 二回目以降の呼び出しは `Err(HeapError::AlreadyInitialized)` を返します。
///
/// # Safety
///
/// この関数は、カーネルブート時にのみ実行されることが意図されており、
/// 呼び出し元は、提供されるメモリ範囲が有効かつ排他的であることを保証する必要があります。
///
/// # Errors
///
/// - `HeapError::AlreadyInitialized` - 既に初期化済みの場合
pub unsafe fn init_heap(
    heap_start: kernel::mm::VirtAddr, 
    heap_size: kernel::mm::LayoutSize
) -> Result<(), HeapError> {
    // 基本的な妥当性チェック
    debug_assert!(heap_start.as_usize() != 0, "Heap start address must not be null");
    debug_assert!(heap_size.as_usize() > 0, "Heap size must be greater than zero");
    
    // Safety: 呼び出し元がヒープ領域の有効性を保証している
    // ALLOCATOR.init内部でunsafe操作と初期化チェックを行う
    unsafe {
        ALLOCATOR.init(heap_start, heap_size)
            .map_err(|_| HeapError::AlreadyInitialized)
    }
}

pub use qemu::{exit_qemu, QemuExitCode};

/// console_print! マクロ - ユーザー向け画面出力
///
/// このマクロは抽象化されたコンソールインターフェースを使用します。
/// 実際のデバイス（Framebuffer/VGA）は初期化時に決定されます。
/// デバッグ出力には `debug_print!` を使用してください。
#[macro_export]
macro_rules! console_print {
    ($($arg:tt)*) => {{
        $crate::kernel::driver::write_console(format_args!($($arg)*));
    }};
}

/// console_println! マクロ - ユーザー向け画面出力（改行付き）
#[macro_export]
macro_rules! console_println {
    () => ($crate::console_print!("\n"));
    ($($arg:tt)*) => ($crate::console_print!("{}\n", format_args!($($arg)*)));
}

/// debug_print! マクロ - デバッグ専用（シリアルポートのみ）
///
/// このマクロは、抽象化されたデバッグ出力インターフェース (`write_debug`) を使用します。
/// 画面には表示されず、シリアルポートのみに出力されます。
#[macro_export]
macro_rules! debug_print {
    ($($arg:tt)*) => {{
        $crate::kernel::driver::write_debug(format_args!($($arg)*));
    }};
}

/// debug_println! マクロ - デバッグ専用（改行付き）
#[macro_export]
macro_rules! debug_println {
    () => ($crate::debug_print!("\n"));
    ($($arg:tt)*) => ($crate::debug_print!("{}\n", format_args!($($arg)*)));
}

/// println! マクロ - コンソール出力とデバッグ出力の両方
///
/// このマクロは互換性のため、画面とシリアルポートの両方に出力します。
/// 用途に応じて `console_println!` または `debug_println!` の使用を推奨します。
#[macro_export]
macro_rules! println {
    () => {{
        $crate::console_print!("\n");
        $crate::debug_print!("\n");
    }};
    ($($arg:tt)*) => {{
        $crate::console_print!("{}\n", format_args!($($arg)*));
        $crate::debug_print!("{}\n", format_args!($($arg)*));
    }};
}

/// print! マクロ - コンソール出力とデバッグ出力の両方
///
/// このマクロは互換性のため、画面とシリアルポートの両方に出力します。
/// 用途に応じて `console_print!` または `debug_print!` の使用を推奨します。
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {{
        $crate::console_print!($($arg)*);
        $crate::debug_print!($($arg)*);
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
