// src/serial/ports.rs

//! Serial port hardware access and operations
//!
//! # Safety Improvements
//! - Centralized unsafe operations with explicit documentation
//! - Timeout handling with proper error propagation
//! - Validated state transitions
//! - Hardware validation with detailed reporting

use super::backend::{DefaultBackend, Register, SerialHardware};
use super::error::InitError;
use super::timeout::{self, AdaptiveTimeout, TimeoutConfig, TimeoutResult};
use crate::constants::{
    BAUD_RATE_DIVISOR, CONFIG_8N1, DLAB_ENABLE, FIFO_ENABLE_CLEAR, LSR_TRANSMIT_EMPTY,
    MODEM_CTRL_ENABLE_IRQ_RTS_DSR,
};

use core::fmt;

impl<H: SerialHardware> fmt::Write for SerialPorts<H> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for byte in s.bytes() {
            // Attempt to write a byte, but ignore errors, similar to
            // how the existing `serial_println!` macro behaves. In a kernel
            // environment, there's often no good way to recover from a
            // failed serial write, so we just continue.
            let _ = self.poll_and_write(byte);
        }
        Ok(())
    }
}

/// Hardware operation state tracking
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HardwareState {
    Uninitialized,
    Configuring,
    Ready,
    Error,
}

/// Private operation enum for centralized unsafe access
pub(super) enum PortOp {
    Configure,
    ScratchWrite(u8),
    ScratchRead,
    LineStatusRead,
    ModemStatusRead,
}

/// Result type for port operations
type PortResult<T> = Result<T, InitError>;

/// Serial ports with validated safe access patterns
pub struct SerialPorts<H: SerialHardware> {
    hardware: H,
    state: HardwareState,
    /// Adaptive timeout for write operations
    adaptive_timeout: AdaptiveTimeout,
}

/// Default serial port implementation backed by x86 port I/O.
pub type DefaultSerialPorts = SerialPorts<DefaultBackend>;

#[derive(Debug, Clone, Copy)]
pub struct ValidationReport {
    scratch_tests: [ScratchTestResult; 4],
    scratch_count: usize,
    pub(crate) lsr_valid: bool,
    pub(crate) fifo_functional: bool,
    pub(crate) baud_config_valid: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScratchTestResult {
    pub pattern: u8,
    pub readback: u8,
    pub passed: bool,
}

impl ValidationReport {
    pub const fn new() -> Self {
        Self {
            scratch_tests: [
                ScratchTestResult {
                    pattern: 0,
                    readback: 0,
                    passed: false,
                },
                ScratchTestResult {
                    pattern: 0,
                    readback: 0,
                    passed: false,
                },
                ScratchTestResult {
                    pattern: 0,
                    readback: 0,
                    passed: false,
                },
                ScratchTestResult {
                    pattern: 0,
                    readback: 0,
                    passed: false,
                },
            ],
            scratch_count: 0,
            lsr_valid: false,
            fifo_functional: false,
            baud_config_valid: false,
        }
    }

    pub fn record_scratch(&mut self, pattern: u8, readback: u8, passed: bool) {
        if self.scratch_count < self.scratch_tests.len() {
            self.scratch_tests[self.scratch_count] = ScratchTestResult {
                pattern,
                readback,
                passed,
            };
            self.scratch_count += 1;
        }
    }

    pub fn scratch_tests(&self) -> &[ScratchTestResult] {
        &self.scratch_tests[..self.scratch_count]
    }

    pub fn is_fully_valid(&self) -> bool {
        let scratch_passed = self.scratch_tests.iter().take(self.scratch_count).all(|t| t.passed);
        self.lsr_valid && self.fifo_functional && self.baud_config_valid && scratch_passed
    }
}

#[cfg(test)]
mod kernel_tests {
    use super::*;

    #[test_case]
    fn test_validation_report_new() {
        let report = ValidationReport::new();
        assert_eq!(report.scratch_count, 0);
        assert!(!report.lsr_valid);
        assert!(!report.fifo_functional);
        assert!(!report.baud_config_valid);
    }

    #[test_case]
    fn test_validation_report_record() {
        let mut report = ValidationReport::new();
        report.record_scratch(0xAA, 0xAA, true);
        assert_eq!(report.scratch_count, 1);
        assert_eq!(report.scratch_tests()[0].pattern, 0xAA);
        assert!(report.scratch_tests()[0].passed);
    }

    #[test_case]
    fn test_hardware_state_enum() {
        let state = HardwareState::Uninitialized;
        assert_eq!(state, HardwareState::Uninitialized);
        assert_ne!(state, HardwareState::Ready);
    }
}

impl<H: SerialHardware> SerialPorts<H> {
    pub const fn new(hardware: H) -> Self {
        Self {
            hardware,
            state: HardwareState::Uninitialized,
            adaptive_timeout: AdaptiveTimeout::new(TimeoutConfig::default_timeout()),
        }
    }

    #[inline]
    fn hw_write(&mut self, register: Register, value: u8) {
        SerialHardware::write(&mut self.hardware, register, value);
    }

    #[inline]
    fn hw_read(&mut self, register: Register) -> u8 {
        SerialHardware::read(&mut self.hardware, register)
    }

    /// Configure UART registers with state validation
    pub fn configure(&mut self) -> PortResult<()> {
        // State transition validation
        match self.state {
            HardwareState::Ready => {
                // Already configured, skip
                return Ok(());
            }
            HardwareState::Configuring => {
                return Err(InitError::ConfigurationFailed);
            }
            _ => {}
        }

        self.state = HardwareState::Configuring;

        let result = self.configure_internal();

        match result {
            Ok(()) => {
                self.state = HardwareState::Ready;
                Ok(())
            }
            Err(e) => {
                self.state = HardwareState::Error;
                Err(e)
            }
        }
    }

    /// Internal configuration logic
    fn configure_internal(&mut self) -> PortResult<()> {
        // SAFETY: Configuration sequence follows UART 16550 spec
        // 1. Disable interrupts
        // 2. Set baud rate via DLAB
        // 3. Configure line parameters
        // 4. Enable and clear FIFO
        // 5. Set modem control
        self.perform_op(&PortOp::Configure)?;

        // Verify configuration
        super::wait_short();

        let lsr = self.read_line_status()?;
        if lsr == 0 || lsr == 0xFF {
            return Err(InitError::ConfigurationFailed);
        }

        Ok(())
    }

    /// Write to scratch register with validation
    pub fn write_scratch(&mut self, value: u8) -> PortResult<()> {
        self.perform_op(&PortOp::ScratchWrite(value)).map(|_| ())
    }

    /// Read from scratch register with validation
    pub fn read_scratch(&mut self) -> PortResult<u8> {
        match self.perform_op(&PortOp::ScratchRead) {
            Ok(val) => Ok(val),
            Err(e) => Err(e),
        }
    }

    /// Read Line Status Register
    pub fn read_line_status(&mut self) -> PortResult<u8> {
        self.perform_op(&PortOp::LineStatusRead)
    }

    /// Read Modem Status Register
    pub fn read_modem_status(&mut self) -> PortResult<u8> {
        self.perform_op(&PortOp::ModemStatusRead)
    }

    /// Poll LSR and write byte when ready (using adaptive timeout)
    pub fn poll_and_write(&mut self, byte: u8) -> PortResult<()> {
        let config = self.adaptive_timeout.current_config();
        let result = self.poll_and_write_with_timeout(byte, config);

        // Update adaptive timeout based on result
        match result {
            Ok(()) => self.adaptive_timeout.record_success(),
            Err(InitError::Timeout) => self.adaptive_timeout.record_failure(),
            _ => {}
        }

        result
    }

    /// Poll LSR and write byte with specified timeout
    fn poll_and_write_with_timeout(&mut self, byte: u8, config: TimeoutConfig) -> PortResult<()> {
        let result = timeout::poll_with_timeout(config, || {
            let lsr = self.hw_read(Register::LineStatus);
            (lsr & LSR_TRANSMIT_EMPTY) != 0
        });

        match result {
            TimeoutResult::Ok(()) => {
                self.hw_write(Register::Data, byte);
                timeout::record_poll_success();
                Ok(())
            }
            TimeoutResult::Timeout { .. } => Err(InitError::Timeout),
        }
    }

    /// Comprehensive hardware validation
    pub fn comprehensive_validation(&mut self) -> PortResult<ValidationReport> {
        let mut report = ValidationReport::new();

        // Test scratch register with multiple patterns
        for &pattern in &[0x00, 0x55, 0xAA, 0xFF] {
            self.write_scratch(pattern)?;
            super::wait_short();
            let readback = self.read_scratch()?;
            report.record_scratch(pattern, readback, pattern == readback);
        }

        // Validate LSR
        let lsr = self.read_line_status()?;
        report.lsr_valid = lsr != 0xFF && (lsr & 0x60) != 0;

        // Test FIFO functionality
        report.fifo_functional = self.test_fifo();

        // Verify baud rate configuration
        report.baud_config_valid = self.verify_baud_rate();

        Ok(report)
    }

    /// Test FIFO functionality
    fn test_fifo(&mut self) -> bool {
        self.hw_write(Register::FifoControl, FIFO_ENABLE_CLEAR);
        super::wait_short();

        let iir = self.hw_read(Register::FifoControl);
        (iir & 0xC0) == 0xC0
    }

    /// Verify baud rate configuration
    fn verify_baud_rate(&mut self) -> bool {
        let original_lcr = self.hw_read(Register::LineControl);
        self.hw_write(Register::LineControl, original_lcr | DLAB_ENABLE);

        let dll = self.hw_read(Register::Data);
        let dlh = self.hw_read(Register::InterruptEnable);

        self.hw_write(Register::LineControl, original_lcr);

        let divisor = (u16::from(dlh) << 8) | u16::from(dll);
        divisor == BAUD_RATE_DIVISOR
    }

    /// Perform port operation with centralized unsafe access
    ///
    /// # Safety
    ///
    /// All port I/O is performed within this function under the following guarantees:
    /// - Port addresses are validated compile-time constants
    /// - Exclusive access via `SERIAL_PORTS` mutex
    /// - Operations follow UART 16550 specification
    /// - No memory safety violations possible with port I/O
    #[allow(clippy::unnecessary_wraps)]
    fn perform_op(&mut self, op: &PortOp) -> PortResult<u8> {
        match op {
            PortOp::Configure => {
                // Step 1: Disable all interrupts
                self.hw_write(Register::InterruptEnable, 0x00);

                // Step 2: Enable DLAB to set baud rate
                self.hw_write(Register::LineControl, DLAB_ENABLE);
                self.hw_write(Register::Data, (BAUD_RATE_DIVISOR & 0xFF) as u8);
                self.hw_write(
                    Register::InterruptEnable,
                    ((BAUD_RATE_DIVISOR >> 8) & 0xFF) as u8,
                );

                // Step 3: Set 8N1 and clear DLAB
                self.hw_write(Register::LineControl, CONFIG_8N1);

                // Step 4: Enable and clear FIFO
                self.hw_write(Register::FifoControl, FIFO_ENABLE_CLEAR);

                // Step 5: Set modem control (DTR, RTS, OUT2)
                self.hw_write(Register::ModemControl, MODEM_CTRL_ENABLE_IRQ_RTS_DSR);

                Ok(1)
            }
            PortOp::ScratchWrite(v) => {
                self.hw_write(Register::Scratch, *v);
                Ok(1)
            }
            PortOp::ScratchRead => Ok(self.hw_read(Register::Scratch)),
            PortOp::LineStatusRead => Ok(self.hw_read(Register::LineStatus)),
            PortOp::ModemStatusRead => Ok(self.hw_read(Register::ModemStatus)),
        }
    }

    /// Get adaptive timeout statistics
    pub const fn timeout_stats(&self) -> (u32, u32, u32) {
        self.adaptive_timeout.stats()
    }

    /// Reset adaptive timeout statistics
    pub const fn reset_timeout_stats(&mut self) {
        self.adaptive_timeout.reset();
    }
}

#[cfg(all(test, feature = "std-tests"))]
mod tests {
    use super::*;

    #[test]
    fn test_validation_report_new() {
        let report = ValidationReport::new();
        assert_eq!(report.scratch_count, 0);
        assert!(!report.lsr_valid);
        assert!(!report.fifo_functional);
        assert!(!report.baud_config_valid);
    }

    #[test]
    fn test_validation_report_record() {
        let mut report = ValidationReport::new();
        report.record_scratch(0xAA, 0xAA, true);
        assert_eq!(report.scratch_count, 1);
        assert_eq!(report.scratch_tests()[0].pattern, 0xAA);
        assert!(report.scratch_tests()[0].passed);
    }

    #[test]
    fn test_hardware_state_transitions() {
        assert_ne!(HardwareState::Uninitialized, HardwareState::Ready);
        assert_ne!(HardwareState::Configuring, HardwareState::Error);
    }
}
