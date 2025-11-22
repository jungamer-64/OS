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
/// この関数を呼び出すには、以下の条件を満たす必要があります:
/// 
/// - `heap_start` と `heap_size` が有効なヒープ領域を指していること
/// - [heap_start, heap_start+heap_size) の範囲が他の目的で使用されていないこと
/// - ヒープ領域が書き込み可能であること
/// - `heap_start` がヌルポインタでないこと
/// - `heap_start + heap_size` がオーバーフローしないこと
///
/// # Errors
///
/// - `HeapError::AlreadyInitialized` - 既に初期化済みの場合
///
/// # Examples
///
/// ```no_run
/// # use tiny_os::{init_heap, kernel::mm::{VirtAddr, LayoutSize}};
/// let heap_start = unsafe { VirtAddr::new_unchecked(0x4444_4444_0000) };
/// let heap_size = LayoutSize::new(100 * 1024); // 100 KB
/// 
/// unsafe {
///     init_heap(heap_start, heap_size).expect("Failed to initialize heap");
/// }
/// ```
pub unsafe fn init_heap(
    heap_start: kernel::mm::VirtAddr, 
    heap_size: kernel::mm::LayoutSize
) -> Result<(), HeapError> {
    // 基本的な妥当性チェック（デバッグビルドのみ）
    debug_assert!(heap_start.as_usize() != 0, "Heap start address must not be null");
    debug_assert!(heap_size.as_usize() > 0, "Heap size must be greater than zero");
    debug_assert!(
        heap_start.checked_add(heap_size.as_usize()).is_some(),
        "Heap address range must not overflow"
    );
    debug_assert!(
        heap_start.as_usize() >= 0x1000,
        "Heap start address too low (potential null pointer region)"
    );
    
    // Safety: 呼び出し元が上記の条件を保証している
    // ALLOCATOR.init内部でunsafe操作を行うが、二重初期化はここで防止される
    unsafe {
        ALLOCATOR.init(heap_start, heap_size)
            .map_err(|_| HeapError::AlreadyInitialized)
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
