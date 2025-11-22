// src/arch/x86_64/mod.rs

pub mod cpu;
pub mod qemu;
// pub mod serial;  // レガシー - kernel::driver::serial を使用
// pub mod vga;     // レガシー -kernel::driver::vga を使用
// pub mod keyboard; // レガシー - kernel::driver::keyboard を使用
pub mod port;
pub mod gdt;
pub mod interrupts;

pub use cpu::X86Cpu;
pub use cpu::read_timestamp;
pub use qemu::write_debug_byte;
pub use port::{Port, PortReadOnly, PortWriteOnly};
pub use gdt::init as init_gdt;
pub use interrupts::init_idt;
