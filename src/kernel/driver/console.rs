// src/kernel/driver/console.rs
//! コンソール抽象化レイヤー
//!
//! トレイトベースでコンソールI/Oを抽象化し、
//! 具体的なドライバ（Framebuffer、VGA）から独立したインターフェースを提供します。

use core::fmt;
use spin::{Mutex, Once};

/// グローバルコンソールインターフェース
///
/// カーネル初期化時に実際のドライバを設定します。
/// このトレイトオブジェクトを通じて、print!マクロはどのデバイスが
/// 使用されているかを知る必要がなくなります。
pub static CONSOLE: Once<Mutex<ConsoleAdapter>> = Once::new();

/// コンソールアダプター
///
/// 異なるコンソールドライバを統一的に扱うためのアダプター
pub enum ConsoleAdapter {
    /// Framebuffer コンソール
    Framebuffer,
    /// VGA テキストモード
    Vga,
    /// 未初期化
    Uninitialized,
}

impl fmt::Write for ConsoleAdapter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        match self {
            ConsoleAdapter::Framebuffer => {
                if let Some(fb) = crate::kernel::driver::framebuffer::FRAMEBUFFER.get() {
                    fb.lock().write_str(s)
                } else {
                    Err(fmt::Error)
                }
            }
            ConsoleAdapter::Vga => {
                if let Some(vga) = crate::kernel::driver::vga::VGA.get() {
                    vga.lock().write_str(s)
                } else {
                    Err(fmt::Error)
                }
            }
            ConsoleAdapter::Uninitialized => Err(fmt::Error),
        }
    }
}

/// コンソールを初期化
///
/// この関数は、カーネル初期化時に一度だけ呼び出されるべきです。
/// Framebuffer優先で初期化を試みます。
/// 
/// # Examples
///
/// ```no_run
/// use tiny_os::kernel::driver::console;
///
/// // Framebuffer が利用可能ならそれを使用
/// console::init_console();
/// ```
pub fn init_console() {
    CONSOLE.call_once(|| {
        // Framebuffer を優先
        if crate::kernel::driver::framebuffer::FRAMEBUFFER.get().is_some() {
            Mutex::new(ConsoleAdapter::Framebuffer)
        }
        // 次に VGA を試す
        else if crate::kernel::driver::vga::VGA.get().is_some() {
            Mutex::new(ConsoleAdapter::Vga)
        }
        // どちらもなければ未初期化
        else {
            Mutex::new(ConsoleAdapter::Uninitialized)
        }
    });
}

/// コンソールに文字列を書き込む
///
/// グローバルコンソールが初期化されていない場合は何もしません。
pub fn write_console(args: fmt::Arguments) {
    use fmt::Write;
    if let Some(console) = CONSOLE.get() {
        // NOTE: エラーは無視する（標準のprint!マクロの挙動）
        let _ = console.lock().write_fmt(args);
    }
}

/// デバッグ出力に文字列を書き込む
///
/// シリアルポートに直接出力します。
/// この関数は、ブート初期化フェーズから利用可能であることが期待されます。
pub fn write_debug(args: fmt::Arguments) {
    use fmt::Write;
    use crate::kernel::driver::serial::SERIAL1;
    // NOTE: エラーは無視する（標準のprint!マクロの挙動）
    let _ = SERIAL1.lock().write_fmt(args);
}
