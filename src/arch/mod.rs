//! Architecture-specific abstractions

#[cfg(target_arch = "x86_64")]
pub mod x86_64;

#[cfg(target_arch = "x86_64")]
pub use self::x86_64::*;

// レガシーバックエンドは削除 - kernel::driver を使用
// #[cfg(target_arch = "x86_64")]
// pub use self::x86_64::serial::PortIoBackend as SerialBackend;
// #[cfg(target_arch = "x86_64")]
// pub use self::x86_64::vga::TextModeBuffer as VgaBackend;
// #[cfg(target_arch = "x86_64")]
// pub use self::x86_64::keyboard::Keyboard;

#[cfg(target_arch = "x86_64")]
/// Architecture-specific CPU implementation
pub type ArchCpu = self::x86_64::X86Cpu;

/// Trait for CPU-specific operations
pub trait Cpu {
    /// Halt the CPU until the next interrupt
    fn halt();
    
    /// Disable interrupts
    fn disable_interrupts();
    
    /// Enable interrupts
    fn enable_interrupts();
    
    /// Check if interrupts are enabled
    fn are_interrupts_enabled() -> bool;
}

/// Read the hardware timestamp counter
#[must_use]
pub fn read_timestamp() -> u64 {
    #[cfg(target_arch = "x86_64")]
    return x86_64::read_timestamp();
    
    #[cfg(not(target_arch = "x86_64"))]
    return 0;
}

/// Write a byte to the platform debug output
pub fn write_debug_byte(byte: u8) {
    #[cfg(target_arch = "x86_64")]
    x86_64::write_debug_byte(byte);
}
