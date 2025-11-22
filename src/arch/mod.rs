// src/arch/mod.rs

//! Architecture-specific abstractions.

#[cfg(target_arch = "x86_64")]
pub mod x86_64;

#[cfg(target_arch = "x86_64")]
pub use self::x86_64::*;

#[cfg(target_arch = "x86_64")]
pub use self::x86_64::serial::PortIoBackend as SerialBackend;

#[cfg(target_arch = "x86_64")]
pub use self::x86_64::vga::TextModeBuffer as VgaBackend;

#[cfg(not(target_arch = "x86_64"))]
pub use crate::serial::backend::StubSerialBackend as SerialBackend;

#[cfg(not(target_arch = "x86_64"))]
pub use crate::vga_buffer::backend::StubBuffer as VgaBackend;

#[cfg(target_arch = "x86_64")]
/// Architecture-specific CPU implementation.
pub type ArchCpu = self::x86_64::X86Cpu;

/// Trait for CPU-specific operations.
pub trait Cpu {
    /// Halt the CPU until the next interrupt.
    fn halt();
    
    /// Disable interrupts.
    fn disable_interrupts();
    
    /// Enable interrupts.
    fn enable_interrupts();
    
    /// Check if interrupts are enabled.
    fn are_interrupts_enabled() -> bool;
}

/// Read the hardware timestamp counter.
///
/// Returns a monotonically increasing tick count. The frequency is
/// architecture-dependent.
pub fn read_timestamp() -> u64 {
    #[cfg(target_arch = "x86_64")]
    return x86_64::read_timestamp();
    
    #[cfg(not(target_arch = "x86_64"))]
    return 0;
}

/// Write a byte to the platform debug output.
///
/// This is typically a serial port or a debug console used for
/// emergency logging (e.g. during panic).
pub fn write_debug_byte(byte: u8) {
    #[cfg(target_arch = "x86_64")]
    x86_64::write_debug_byte(byte);
}
