// kernel/src/arch/x86_64/mod.rs
//! x86_64 architecture-specific implementations.

/// CPU operations and control.
pub mod cpu;
/// QEMU-specific functionality.
pub mod qemu;
pub mod port;
pub mod gdt;
pub mod interrupts;
pub mod pic;
pub mod syscall;

pub use cpu::{X86Cpu, InterruptFlags, critical_section};
pub use cpu::read_timestamp;
pub use qemu::write_debug_byte;
pub use port::{Port, PortReadOnly, PortWriteOnly};
pub use gdt::init as init_gdt;
pub use interrupts::init_idt;
