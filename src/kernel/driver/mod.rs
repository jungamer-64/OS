//! デバイスドライバモジュール

pub mod console;
pub mod serial;
pub mod vga;
pub mod keyboard;
pub mod pit;
pub mod framebuffer;

pub use console::{
    ConsoleWriter, set_framebuffer_console, set_vga_console, write_console, write_debug,
    enter_panic, NORMAL, FIRST_PANIC, DOUBLE_PANIC, PanicLevel,
};
pub use serial::SerialPort;
pub use vga::VgaTextMode;
pub use keyboard::PS2Keyboard;
pub use pit::ProgrammableIntervalTimer;

pub use serial::SERIAL1;
pub use vga::{init_vga, vga};
pub use pit::PIT;
