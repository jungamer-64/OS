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
/// CR3 switching diagnostic tests (Phase 3 preparation)
pub mod cr3_test;

pub use cpu::{X86Cpu, InterruptFlags, critical_section};
pub use cpu::read_timestamp;
pub use qemu::write_debug_byte;
pub use port::{Port, PortReadOnly, PortWriteOnly};
pub use gdt::init as init_gdt;
pub use interrupts::init_idt;
pub use cr3_test::run_cr3_diagnostic_tests;
