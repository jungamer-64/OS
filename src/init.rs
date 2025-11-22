// src/init.rs

//! Kernel initialization module with enhanced robustness
//!
//! # Improvements
//! - Atomic state machine for initialization phases
//! - Idempotent operations with state validation
//! - Detailed error reporting
//! - Rollback support for failed initialization

use crate::diagnostics::DIAGNOSTICS;
use crate::println;
use crate::serial::{InitError as SerialInitError};
use crate::display::color::ColorCode;
use core::sync::atomic::{AtomicU8, Ordering};
use crate::arch::{Cpu, ArchCpu};

/// Initialization phases with explicit state machine
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitPhase {
    NotStarted = 0,
    VgaInit = 1,
    SerialInit = 2,
    Complete = 3,
    Failed = 255,
}

impl InitPhase {
    /// Check if this phase can transition to the next phase
    const fn can_transition_to(self, next: Self) -> bool {
        if matches!(next, Self::Failed) {
            return true;
        }

        matches!(self.next(), Some(expected) if (expected as u8) == (next as u8))
    }

    /// Get next phase in sequence（将来の自動リカバリで使用予定）
    const fn next(self) -> Option<Self> {
        match self {
            Self::NotStarted => Some(Self::VgaInit),
            Self::VgaInit => Some(Self::SerialInit),
            Self::SerialInit => Some(Self::Complete),
            Self::Complete | Self::Failed => None,
        }
    }
}

impl From<u8> for InitPhase {
    fn from(value: u8) -> Self {
        match value {
            1 => Self::VgaInit,
            2 => Self::SerialInit,
            3 => Self::Complete,
            255 => Self::Failed,
            _ => Self::NotStarted,
        }
    }
}

static CURRENT_PHASE: AtomicU8 = AtomicU8::new(InitPhase::NotStarted as u8);

/// Get a human-readable status string for the current initialization phase
pub fn status_string() -> &'static str {
    match InitPhase::from(CURRENT_PHASE.load(Ordering::Relaxed)) {
        InitPhase::NotStarted => "Not Started",
        InitPhase::VgaInit => "VGA Init",
        InitPhase::SerialInit => "Serial Init",
        InitPhase::Complete => "Complete",
        InitPhase::Failed => "Failed",
    }
}

/// Halt the CPU forever
pub fn halt_forever() -> ! {
    loop {
        ArchCpu::halt();
    }
}

#[cfg(test)]
mod kernel_tests {
    use super::*;

    #[test_case]
    fn test_init_phase_transitions() {
        assert!(InitPhase::NotStarted.can_transition_to(InitPhase::VgaInit));
        assert!(InitPhase::VgaInit.can_transition_to(InitPhase::SerialInit));
        assert!(InitPhase::SerialInit.can_transition_to(InitPhase::Complete));
        
        // Invalid transitions
        assert!(!InitPhase::NotStarted.can_transition_to(InitPhase::SerialInit));
        assert!(!InitPhase::Complete.can_transition_to(InitPhase::VgaInit));
    }

    #[test_case]
    fn test_init_phase_failure_transition() {
        // Any phase can transition to Failed
        assert!(InitPhase::NotStarted.can_transition_to(InitPhase::Failed));
        assert!(InitPhase::VgaInit.can_transition_to(InitPhase::Failed));
        assert!(InitPhase::SerialInit.can_transition_to(InitPhase::Failed));
    }

    #[test_case]
    fn test_init_phase_from_u8() {
        assert_eq!(InitPhase::from(0), InitPhase::NotStarted);
        assert_eq!(InitPhase::from(1), InitPhase::VgaInit);
        assert_eq!(InitPhase::from(2), InitPhase::SerialInit);
        assert_eq!(InitPhase::from(3), InitPhase::Complete);
        assert_eq!(InitPhase::from(255), InitPhase::Failed);
        assert_eq!(InitPhase::from(100), InitPhase::NotStarted); // Default
    }
}
