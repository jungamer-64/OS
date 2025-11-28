// kernel/src/kernel/driver/console.rs
//! コンソール抽象化レイヤー
//!
//! フレームバッファとVGAテキストモードを統一的に扱うための抽象化層。
//! パニック時のデッドロック回避機能を提供します。

use core::fmt;
use core::sync::atomic::{AtomicU8, Ordering};
use spin::Mutex;
use crate::kernel::core::{KernelResult, DeviceError};

/// パニックレベルの型
pub type PanicLevel = u8;

/// パニック状態の定数
pub const NORMAL: PanicLevel = 0;
/// First panic level constant.
pub const FIRST_PANIC: PanicLevel = 1;
/// Double panic level constant (recursive panic).
pub const DOUBLE_PANIC: PanicLevel = 2;

/// パニックレベル
/// 0 = 通常動作, 1 = 初回パニック, 2+ = 再帰的パニック
static PANIC_LEVEL: AtomicU8 = AtomicU8::new(NORMAL);

/// パニックレベルを1増やし、変更前の値を返す
/// 
/// # Returns
/// 
/// 変更前のパニックレベル
/// - `NORMAL` (0): 通常動作
/// - `FIRST_PANIC` (1): 初回パニック
/// - `DOUBLE_PANIC` (2): 二重パニック
/// 
/// # Note
/// 
/// `Ordering::Relaxed` を使用: パニックフラグは他のメモリ操作と
/// 同期する必要がない（単独のフラグとして機能）
pub fn enter_panic() -> PanicLevel {
    PANIC_LEVEL.fetch_add(1, Ordering::Relaxed)
}

/// コンソール書き込みトレイト
///
/// `fmt::Write` を継承し、統一的な書き込みインターフェースを提供します。
pub trait ConsoleWriter: fmt::Write + Send + Sync {}

/// コンソール実装ラッパー
enum ConsoleImpl {
    /// Framebuffer コンソール
    Framebuffer(&'static Mutex<crate::kernel::driver::framebuffer::Framebuffer>),
    /// VGA テキストモード
    Vga(&'static Mutex<crate::kernel::driver::vga::VgaTextMode>),
}

impl fmt::Write for ConsoleImpl {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        match self {
            Self::Framebuffer(fb) => fb.lock().write_str(s),
            Self::Vga(vga) => vga.lock().write_str(s),
        }
    }
}

/// グローバルコンソール
///
/// 初期化後は不変の参照を保持します。
/// `Option` により未初期化状態を型安全に表現します。
static CONSOLE: Mutex<Option<ConsoleImpl>> = Mutex::new(None);

/// フレームバッファをコンソールとして設定
///
/// この関数は、カーネル初期化時に一度だけ呼び出されるべきです。
///
/// # Errors
///
/// 既に設定されている場合は `DeviceError::InitFailed` を返します。
pub fn set_framebuffer_console(
    fb: &'static Mutex<crate::kernel::driver::framebuffer::Framebuffer>
) -> KernelResult<()> {
    let mut guard = CONSOLE.lock();
    if guard.is_none() {
        *guard = Some(ConsoleImpl::Framebuffer(fb));
        Ok(())
    } else {
        Err(DeviceError::InitFailed.into())
    }
}

/// VGA をコンソールとして設定
///
/// この関数は、カーネル初期化時に一度だけ呼び出されるべきです。
///
/// # Errors
///
/// 既に設定されている場合は `DeviceError::InitFailed` を返します。
pub fn set_vga_console(
    vga: &'static Mutex<crate::kernel::driver::vga::VgaTextMode>
) -> KernelResult<()> {
    let mut guard = CONSOLE.lock();
    if guard.is_none() {
        *guard = Some(ConsoleImpl::Vga(vga));
        Ok(())
    } else {
        Err(DeviceError::InitFailed.into())
    }
}

/// コンソールに書き込む
/// 
/// # 動作モード
/// 
/// - **通常時** (`NORMAL`):
///   - `try_lock()` でロックを試みる
///   - 失敗した場合は出力をスキップ（デッドロック防止）
/// 
/// - **初回パニック時** (`FIRST_PANIC`):
///   - シリアルポートのみに出力
///   - コンソールへの出力はスキップ（安全性優先）
/// 
/// - **二重パニック以降** (`DOUBLE_PANIC+`):
///   - 何も出力しない（無限ループ防止）
/// 
/// # Safety
/// 
/// この関数は以下の保証を提供します:
/// - デッドロックしない（`try_lock()` 使用）
/// - データ競合を起こさない（`force_unlock()` 不使用）
/// - パニック中でも可能な限り出力する（ベストエフォート）
pub fn write_console(args: fmt::Arguments) {
    use fmt::Write;
    
    let panic_level = PANIC_LEVEL.load(Ordering::Relaxed);
    
    match panic_level {
        NORMAL => {
            // 通常時: 安全にロック
            if let Some(mut guard) = CONSOLE.try_lock()
                && let Some(ref mut console) = *guard {
                    let _ = console.write_fmt(args);
                }
        }
        FIRST_PANIC => {
            // 初回パニック: シリアルのみに出力
            write_debug(args);
        }
        _ => {
            // DOUBLE_PANIC 以降は何もしない
        }
    }
}

/// デバッグ出力（シリアルポート経由）
/// 
/// パニック時でもベストエフォートで出力を試みます。
/// `try_lock()`が失敗した場合は出力をスキップします（安全性優先）。
/// 
/// # Note
/// 
/// - `NORMAL` および `FIRST_PANIC`、`DOUBLE_PANIC` 時に出力を試みる
/// - 三重パニック以降は完全に無音（無限ループ防止）
/// - `force_unlock()` は使用しない（データ競合回避）
pub fn write_debug(args: fmt::Arguments) {
    use fmt::Write;
    use crate::kernel::driver::serial::SERIAL1;
    
    let panic_level = PANIC_LEVEL.load(Ordering::Relaxed);
    
    if panic_level <= DOUBLE_PANIC {
        // 通常時およびパニック時（二重パニックまで）
        // force_unlock()を使わず、try_lock()のみで安全に試行
        if let Some(mut serial) = SERIAL1.try_lock() {
            let _ = serial.write_fmt(args);
        }
        // 失敗してもOK（データ競合よりも安全性を優先）
    }
    // 三重パニック以降は出力しない
}


