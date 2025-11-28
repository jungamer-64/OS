// crates/kernel/src/arch/x86_64/mod.rs
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
/// Ring-based syscall system (io_uring-style async message passing)
pub mod syscall_ring;
/// CR3 switching diagnostic tests (Phase 3 preparation)
pub mod cr3_test;
/// Task State Segment (Phase 2: Process Management)
pub mod tss;
/// FPU/SSE state management (Phase 2: Process Management)
pub mod fpu;
/// CPU feature detection (using raw-cpuid)
pub mod cpu_features;
/// Per-CPU data management (Phase 3: swapgs-based syscall)
pub mod per_cpu;
/// Symmetric Multi-Processing support (Phase 3: SMP)
pub mod smp;

pub use cpu::{X86Cpu, InterruptFlags, critical_section};
pub use cpu::read_timestamp;
pub use qemu::write_debug_byte;
pub use port::{Port, PortReadOnly, PortWriteOnly};
pub use gdt::init as init_gdt;
pub use interrupts::init_idt;
pub use cr3_test::run_cr3_diagnostic_tests;
pub use tss::{init as init_tss, update_kernel_stack};
pub use fpu::{init as init_fpu, save_fpu_state, restore_fpu_state};
pub use cpu_features::{detect as detect_cpu_features, get as get_cpu_features};
pub use per_cpu::{init as init_per_cpu, update_kernel_stack as update_per_cpu_stack};
pub use smp::{init as init_smp, cpu_count, current_cpu_id};
pub use syscall_ring::{
    init as init_syscall_ring,
    RingContext, IdealSqe, IdealCqe, RingOpcode, RingMappingInfo,
    ideal_syscall_entry, kernel_poll_all, kernel_poller_loop,
    init_ring_for_process,
};
pub use syscall::{SyscallMode, init_with_mode, switch_mode, current_mode};
