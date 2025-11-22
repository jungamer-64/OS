//! Utilities for interacting with QEMU test infrastructure.

use crate::arch::{Cpu, ArchCpu};
use crate::arch::qemu;

/// Exit codes understood by QEMU's ISA debug exit device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum QemuExitCode {
    /// Signal that the test run completed successfully.
    Success = 0x10,
    /// Signal that at least one test failed.
    Failed = 0x11,
}

/// Write the exit code to QEMU's debug exit port and halt the CPU.
#[inline]
pub fn exit_qemu(code: QemuExitCode) -> ! {
    // SAFETY: Port 0xF4 is the QEMU ISA debug exit. Writing to it is safe in
    // the kernel context and causes QEMU to exit with the provided status.
    qemu::exit_qemu(code as u32);

    loop {
        // SAFETY: We are in ring 0 and halting the CPU is safe here.
        ArchCpu::halt();
    }
}
