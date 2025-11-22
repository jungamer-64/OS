// src/kernel/driver/mod.rs
//! デバイスドライバモジュール

pub mod serial;
pub mod vga;
pub mod keyboard;

pub use serial::SerialPort;
pub use vga::VgaTextMode;
pub use keyboard::PS2Keyboard;

pub use serial::SERIAL1;
pub use vga::{init_vga, vga};
