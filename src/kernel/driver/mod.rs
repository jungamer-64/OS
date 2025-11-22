//! デバイスドライバモジュール

pub mod console;
pub mod serial;
pub mod vga;
pub mod keyboard;
pub mod pit;
pub mod framebuffer;

pub use console::{ConsoleAdapter, init_console, write_console, write_debug, CONSOLE};
pub use serial::SerialPort;
pub use vga::VgaTextMode;
pub use keyboard::PS2Keyboard;
pub use pit::ProgrammableIntervalTimer;

pub use serial::SERIAL1;
pub use vga::{init_vga, vga};
pub use pit::PIT;
