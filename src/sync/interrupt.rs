//! Interrupt controller abstraction.

use crate::arch::{Cpu, ArchCpu};

/// A trait for controlling CPU interrupts.
///
/// This trait abstracts over the hardware-specific details of enabling and
/// disabling interrupts.
pub trait InterruptController {
    /// Disables interrupts and returns a token that will re-enable them when dropped.
    fn without_interrupts<F, R>(f: F) -> R
    where
        F: FnOnce() -> R;
}

/// Generic implementation of `InterruptController` using `ArchCpu`.
pub struct GenericInterruptController;

impl InterruptController for GenericInterruptController {
    fn without_interrupts<F, R>(f: F) -> R
    where
        F: FnOnce() -> R,
    {
        let saved = ArchCpu::are_interrupts_enabled();
        if saved {
            ArchCpu::disable_interrupts();
        }
        
        let ret = f();
        
        if saved {
            ArchCpu::enable_interrupts();
        }
        ret
    }
}

/// Architecture-specific interrupt controller.
pub type ArchInterruptController = GenericInterruptController;

#[cfg(test)]
mod kernel_tests {
    use super::*;

    #[test_case]
    fn test_without_interrupts_execution() {
        let mut executed = false;
        GenericInterruptController::without_interrupts(|| {
            executed = true;
        });
        assert!(executed);
    }
    
    #[test_case]
    fn test_without_interrupts_return_value() {
        let result = GenericInterruptController::without_interrupts(|| {
            42
        });
        assert_eq!(result, 42);
    }
}
