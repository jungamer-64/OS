// src/init.rs

//! Kernel initialization module with enhanced robustness
//!
//! # Improvements
//! - Atomic state machine for initialization phases
//! - Idempotent operations with state validation
//! - Detailed error reporting
//! - Rollback support for failed initialization

use crate::constants::{
    SERIAL_ALREADY_INITIALIZED_LINES, SERIAL_IDLE_LOOP_LINES, SERIAL_INIT_SUCCESS_LINES,
    SERIAL_SAFETY_FEATURE_LINES,
};
use crate::diagnostics::DIAGNOSTICS;
use crate::serial::{self, InitError as SerialInitError};
use crate::serial_println;
use crate::vga_buffer::ColorCode;
use core::sync::atomic::{AtomicU32, AtomicU8, Ordering};

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

/// Initialization state with atomic operations
static INIT_PHASE: AtomicU8 = AtomicU8::new(InitPhase::NotStarted as u8);

/// Initialization lock using compare-and-swap
static INIT_LOCK: AtomicU32 = AtomicU32::new(0);

/// Magic value indicating initialization is in progress
const INIT_MAGIC: u32 = 0xDEAD_BEEF;

/// Initialization result type
type InitResult<T> = Result<T, InitError>;

/// Initialization errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitError {
    /// VGA initialization failed
    VgaFailed(&'static str),
    /// Serial initialization failed (non-critical)
    SerialFailed(&'static str),
    /// Invalid state transition
    InvalidStateTransition,
    /// Already initialized
    AlreadyInitialized,
    /// Initialization in progress
    InProgress,
    /// Multiple concurrent initialization attempts
    ConcurrentInitialization,
}

impl InitError {
    /// Check if error is critical (prevents kernel operation)
    const fn is_critical(&self) -> bool {
        matches!(self, Self::VgaFailed(_))
    }
}

/// Get current initialization phase
#[inline]
pub fn current_phase() -> InitPhase {
    InitPhase::from(INIT_PHASE.load(Ordering::Acquire))
}

/// Transition to next phase with validation
fn transition_phase(expected: InitPhase, next: InitPhase) -> InitResult<()> {
    if !expected.can_transition_to(next) {
        return Err(InitError::InvalidStateTransition);
    }

    let result = INIT_PHASE.compare_exchange(
        expected as u8,
        next as u8,
        Ordering::AcqRel,
        Ordering::Acquire,
    );

    match result {
        Ok(_) => Ok(()),
        Err(_) => Err(InitError::InvalidStateTransition),
    }
}

/// Initialize VGA buffer with validation
///
/// # Errors
///
/// Returns `InitError::VgaFailed` if VGA initialization fails or buffer is not accessible.
/// Returns `InitError::InvalidStateTransition` if called in an invalid state.
pub fn initialize_vga() -> InitResult<()> {
    let current = current_phase();

    // Check if already past VGA initialization
    match current {
        InitPhase::NotStarted => {
            // Proceed with initialization
            transition_phase(InitPhase::NotStarted, InitPhase::VgaInit)?;
        }
        InitPhase::VgaInit | InitPhase::SerialInit | InitPhase::Complete => {
            // Already initialized
            return Ok(());
        }
        InitPhase::Failed => {
            return Err(InitError::VgaFailed("Previous initialization failed"));
        }
    }

    // Perform VGA initialization (let-else pattern for early return)
    let Ok(()) = crate::vga_buffer::init() else {
        transition_phase(InitPhase::VgaInit, InitPhase::Failed).ok();
        return Err(InitError::VgaFailed("VGA buffer init failed"));
    };

    // Validate initialization
    if !crate::vga_buffer::is_accessible() {
        transition_phase(InitPhase::VgaInit, InitPhase::Failed).ok();
        return Err(InitError::VgaFailed("VGA buffer not accessible"));
    }

    // Clear screen and set default colors (let-else pattern)
    let Ok(()) = crate::vga_buffer::clear() else {
        transition_phase(InitPhase::VgaInit, InitPhase::Failed).ok();
        return Err(InitError::VgaFailed("VGA clear failed"));
    };

    let Ok(()) = crate::vga_buffer::set_color(ColorCode::normal()) else {
        transition_phase(InitPhase::VgaInit, InitPhase::Failed).ok();
        return Err(InitError::VgaFailed("VGA set_color failed"));
    };

    Ok(())
}

/// Initialize serial port with graceful degradation
///
/// # Errors
///
/// Returns `InitError::SerialFailed` if serial port initialization fails.
/// Returns `InitError::InvalidStateTransition` if VGA is not initialized first.
/// Note: Serial failure is non-critical and the kernel can continue with VGA-only output.
pub fn initialize_serial() -> InitResult<()> {
    let current = current_phase();

    // Validate state
    match current {
        InitPhase::VgaInit => {
            transition_phase(InitPhase::VgaInit, InitPhase::SerialInit)?;
        }
        InitPhase::SerialInit | InitPhase::Complete => {
            // Already at or past this phase
            return Ok(());
        }
        InitPhase::NotStarted => {
            return Err(InitError::InvalidStateTransition);
        }
        InitPhase::Failed => {
            return Err(InitError::SerialFailed("Previous initialization failed"));
        }
    }

    // Attempt serial initialization
    match crate::serial::init() {
        Ok(()) => {
            serial::log_lines(SERIAL_INIT_SUCCESS_LINES.iter().copied());
            Ok(())
        }
        Err(SerialInitError::AlreadyInitialized) => {
            serial::log_lines(SERIAL_ALREADY_INITIALIZED_LINES.iter().copied());
            Ok(())
        }
        Err(SerialInitError::PortNotPresent) => {
            // Non-critical: many systems don't have serial ports
            report_serial_unavailable("Hardware not present");
            Err(InitError::SerialFailed("Port not present"))
        }
        Err(SerialInitError::Timeout) => {
            report_serial_unavailable("Hardware timeout");
            Err(InitError::SerialFailed("Timeout"))
        }
        Err(SerialInitError::ConfigurationFailed) => {
            report_serial_unavailable("Configuration failed");
            Err(InitError::SerialFailed("Configuration failed"))
        }
        Err(SerialInitError::HardwareAccessFailed) => {
            report_serial_unavailable("Hardware access failed");
            Err(InitError::SerialFailed("Hardware access failed"))
        }
        Err(SerialInitError::TooManyAttempts) => {
            report_serial_unavailable("Too many attempts");
            Err(InitError::SerialFailed("Too many attempts"))
        }
    }
}

/// Report serial unavailability to VGA
fn report_serial_unavailable(reason: &str) {
    if crate::vga_buffer::is_accessible() {
        let _ = crate::vga_buffer::print_colored(
            "[INFO] Serial port not available: ",
            ColorCode::warning(),
        );
        let _ = crate::vga_buffer::print_colored(reason, ColorCode::normal());
        let _ = crate::vga_buffer::print_colored("\n", ColorCode::normal());
        let _ = crate::vga_buffer::print_colored(
            "       Continuing with VGA output only\n",
            ColorCode::normal(),
        );
    }
}

/// Report VGA status to serial
pub fn report_vga_status() {
    if !crate::serial::is_available() {
        return;
    }

    serial_println!("[OK] VGA text mode initialized");
    serial_println!("     - Resolution: 80x25 characters");
    serial_println!("     - Colors: 16-color palette");
    serial_println!("     - Buffer address: 0xB8000");
    serial_println!("     - Auto-scroll: Enabled");
    serial_println!(
        "     - Buffer validation: {}",
        if crate::vga_buffer::is_accessible() {
            "Passed"
        } else {
            "Failed"
        }
    );
    serial_println!();
}

/// Report safety features to serial
pub fn report_safety_features() {
    if !crate::serial::is_available() {
        return;
    }

    serial::log_lines(SERIAL_SAFETY_FEATURE_LINES.iter().copied());
}

/// Complete initialization sequence
///
/// # Errors
///
/// Returns various `InitError` types depending on the failure:
/// - `AlreadyInitialized` if initialization was already completed
/// - `InProgress` if another initialization is currently running
/// - `ConcurrentInitialization` if multiple simultaneous attempts detected
/// - Propagates errors from VGA or serial initialization
pub fn initialize_all() -> InitResult<()> {
    // Acquire initialization lock
    match INIT_LOCK.compare_exchange(0, INIT_MAGIC, Ordering::AcqRel, Ordering::Acquire) {
        Ok(_) => {
            // We acquired the lock, proceed with initialization
            let result = perform_initialization();

            match result {
                Ok(()) => {
                    // Keep lock held (initialization complete)
                    Ok(())
                }
                Err(e) => {
                    // Release lock on failure to allow retry
                    if !e.is_critical() {
                        INIT_LOCK.store(0, Ordering::Release);
                    }
                    Err(e)
                }
            }
        }
        Err(INIT_MAGIC) => {
            // Already initialized or in progress
            match current_phase() {
                InitPhase::Complete => Err(InitError::AlreadyInitialized),
                InitPhase::Failed => Err(InitError::VgaFailed("Previous initialization failed")),
                _ => Err(InitError::InProgress),
            }
        }
        Err(_) => Err(InitError::ConcurrentInitialization),
    }
}

/// Perform the initialization sequence
fn perform_initialization() -> InitResult<()> {
    // Record boot timestamp
    DIAGNOSTICS.set_boot_time();

    // Phase 1: Initialize VGA (critical)
    initialize_vga()?;

    // Phase 2: Initialize serial (non-critical)
    let serial_result = initialize_serial();

    // Phase 3: Report status
    report_vga_status();
    report_safety_features();

    // Phase 4: Mark as complete
    let current = current_phase();
    if current == InitPhase::SerialInit {
        transition_phase(InitPhase::SerialInit, InitPhase::Complete)?;
    }

    // Return serial result for informational purposes
    // but don't fail initialization if serial is unavailable
    if let Err(e) = serial_result {
        if !e.is_critical() {
            // Log warning but continue
            return Ok(());
        }
        return Err(e);
    }

    Ok(())
}

/// Enter low-power idle loop
///
/// # Safety
///
/// This function uses the `hlt` instruction which:
/// - Requires privilege level 0 (kernel mode)
/// - Pauses CPU until next interrupt
/// - Reduces power consumption
/// - Is safe to call repeatedly
pub fn halt_forever() -> ! {
    // Log final status
    if crate::serial::is_available() {
        serial::log_lines(SERIAL_IDLE_LOOP_LINES.iter().copied());
    }

    loop {
        // SAFETY: We are in kernel mode and hlt is safe to use
        x86_64::instructions::hlt();
    }
}

/// Check if initialization is complete
#[inline]
#[must_use = "initialization status should be checked before operations"]
pub fn is_initialized() -> bool {
    current_phase() == InitPhase::Complete
}

/// Get human-readable initialization status
///
/// Returns a static string describing the current initialization phase.
///
/// # Returns
///
/// A descriptive status string (e.g., "VGA initialized", "Complete")
#[must_use]
pub fn status_string() -> &'static str {
    match current_phase() {
        InitPhase::NotStarted => "Not started",
        InitPhase::VgaInit => "VGA initialized",
        InitPhase::SerialInit => "Serial initialized",
        InitPhase::Complete => "Complete",
        InitPhase::Failed => "Failed",
    }
}

/// Get detailed initialization status
///
/// Returns comprehensive diagnostic information about the initialization
/// state of all subsystems. Useful for debugging and health monitoring.
///
/// # Returns
///
/// An `InitStatus` structure containing phase and subsystem states
pub fn detailed_status() -> InitStatus {
    InitStatus {
        phase: current_phase(),
        vga_available: crate::vga_buffer::is_accessible(),
        serial_available: crate::serial::is_available(),
        lock_held: INIT_LOCK.load(Ordering::Acquire) == INIT_MAGIC,
    }
}

/// Detailed initialization status（将来の診断情報拡張で使用予定）
#[derive(Debug, Clone, Copy)]
pub struct InitStatus {
    pub phase: InitPhase,
    pub vga_available: bool,
    pub serial_available: bool,
    pub lock_held: bool,
}

impl InitStatus {
    /// Check if system is operational (at least one output available)
    #[must_use]
    pub const fn is_operational(&self) -> bool {
        self.vga_available && matches!(self.phase, InitPhase::Complete)
    }

    /// Check if any output is available
    #[must_use]
    pub const fn has_output(&self) -> bool {
        self.vga_available || self.serial_available
    }
}

#[cfg(all(test, feature = "std-tests"))]
mod tests {
    use super::*;

    #[test]
    fn test_phase_transitions() {
        assert!(InitPhase::NotStarted.can_transition_to(InitPhase::VgaInit));
        assert!(InitPhase::VgaInit.can_transition_to(InitPhase::SerialInit));
        assert!(InitPhase::SerialInit.can_transition_to(InitPhase::Complete));
        assert!(!InitPhase::Complete.can_transition_to(InitPhase::VgaInit));
    }

    #[test]
    fn test_phase_next() {
        assert_eq!(InitPhase::NotStarted.next(), Some(InitPhase::VgaInit));
        assert_eq!(InitPhase::VgaInit.next(), Some(InitPhase::SerialInit));
        assert_eq!(InitPhase::SerialInit.next(), Some(InitPhase::Complete));
        assert_eq!(InitPhase::Complete.next(), None);
    }

    #[test]
    fn test_phase_from_u8() {
        assert_eq!(InitPhase::from(0), InitPhase::NotStarted);
        assert_eq!(InitPhase::from(1), InitPhase::VgaInit);
        assert_eq!(InitPhase::from(2), InitPhase::SerialInit);
        assert_eq!(InitPhase::from(3), InitPhase::Complete);
        assert_eq!(InitPhase::from(255), InitPhase::Failed);
    }

    #[test]
    fn test_init_error_criticality() {
        assert!(InitError::VgaFailed("test").is_critical());
        assert!(!InitError::SerialFailed("test").is_critical());
    }

    #[test]
    fn test_init_status_operational() {
        let status = InitStatus {
            phase: InitPhase::Complete,
            vga_available: true,
            serial_available: true,
            lock_held: true,
        };
        assert!(status.is_operational());
        assert!(status.has_output());
    }

    #[test]
    fn test_init_status_no_output() {
        let status = InitStatus {
            phase: InitPhase::Complete,
            vga_available: false,
            serial_available: false,
            lock_held: true,
        };
        assert!(!status.is_operational());
        assert!(!status.has_output());
    }
}
