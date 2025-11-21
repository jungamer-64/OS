// src/arch/mod.rs

//! Architecture-specific abstractions.

#[cfg(target_arch = "x86_64")]
pub mod x86_64;

#[cfg(target_arch = "x86_64")]
pub use self::x86_64::*;

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
