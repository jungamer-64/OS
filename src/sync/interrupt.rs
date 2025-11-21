//! Interrupt controller abstraction.

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

/// An implementation of `InterruptController` for the x86_64 architecture.
pub struct X64InterruptController;

impl InterruptController for X64InterruptController {
    fn without_interrupts<F, R>(f: F) -> R
    where
        F: FnOnce() -> R,
    {
        x86_64::instructions::interrupts::without_interrupts(f)
    }
}
