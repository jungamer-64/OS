// src/serial/ports.rs

//! Serial port hardware access and operations
//!
//! # Safety Improvements
//! - Centralized unsafe operations with explicit documentation
//! - Timeout handling with proper error propagation
//! - Validated state transitions
//! - Hardware validation with detailed reporting

use super::constants::{port_addr, register_offset};
use super::error::InitError;
use super::timeout::{self, AdaptiveTimeout, TimeoutConfig, TimeoutResult};
use crate::constants::*;
use x86_64::instructions::port::Port;

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
pub struct SerialPorts {
    data: Port<u8>,
    interrupt_enable: Port<u8>,
    fifo: Port<u8>,
    line_control: Port<u8>,
    modem_control: Port<u8>,
    line_status: Port<u8>,
    modem_status: Port<u8>,
    scratch: Port<u8>,
    state: HardwareState,
    /// Adaptive timeout for write operations
    adaptive_timeout: AdaptiveTimeout,
}

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

    fn record_scratch(&mut self, pattern: u8, readback: u8, passed: bool) {
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
        self.scratch_tests().iter().all(|result| result.passed)
            && self.lsr_valid
            && self.fifo_functional
            && self.baud_config_valid
    }
}

impl SerialPorts {
    pub const fn new() -> Self {
        Self {
            data: Port::new(port_addr(register_offset::DATA)),
            interrupt_enable: Port::new(port_addr(register_offset::INTERRUPT_ENABLE)),
            fifo: Port::new(port_addr(register_offset::FIFO_CONTROL)),
            line_control: Port::new(port_addr(register_offset::LINE_CONTROL)),
            modem_control: Port::new(port_addr(register_offset::MODEM_CONTROL)),
            line_status: Port::new(port_addr(register_offset::LINE_STATUS)),
            modem_status: Port::new(port_addr(register_offset::MODEM_STATUS)),
            scratch: Port::new(port_addr(register_offset::SCRATCH)),
            state: HardwareState::Uninitialized,
            adaptive_timeout: AdaptiveTimeout::new(TimeoutConfig::default_timeout()),
        }
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
            Ok(_) => {
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
        self.perform_op(PortOp::Configure)?;

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
        self.perform_op(PortOp::ScratchWrite(value)).map(|_| ())
    }

    /// Read from scratch register with validation
    pub fn read_scratch(&mut self) -> PortResult<u8> {
        match self.perform_op(PortOp::ScratchRead) {
            Ok(val) => Ok(val),
            Err(e) => Err(e),
        }
    }

    /// Read Line Status Register
    pub fn read_line_status(&mut self) -> PortResult<u8> {
        self.perform_op(PortOp::LineStatusRead)
    }

    /// Read Modem Status Register
    pub fn read_modem_status(&mut self) -> PortResult<u8> {
        self.perform_op(PortOp::ModemStatusRead)
    }

    /// Poll LSR and write byte when ready (using adaptive timeout)
    pub fn poll_and_write(&mut self, byte: u8) -> PortResult<()> {
        let config = self.adaptive_timeout.current_config();
        let result = self.poll_and_write_with_timeout(byte, config);

        // Update adaptive timeout based on result
        match result {
            Ok(_) => self.adaptive_timeout.record_success(),
            Err(InitError::Timeout) => self.adaptive_timeout.record_failure(),
            _ => {}
        }

        result
    }

    /// Poll LSR and write byte with specified timeout
    fn poll_and_write_with_timeout(&mut self, byte: u8, config: TimeoutConfig) -> PortResult<()> {
        let result = timeout::poll_with_timeout(config, || {
            // SAFETY: Reading line status register
            let lsr = unsafe { self.line_status.read() };
            (lsr & LSR_TRANSMIT_EMPTY) != 0
        });

        match result {
            TimeoutResult::Ok(()) => {
                // SAFETY: Writing to data port when transmit buffer is empty
                unsafe {
                    self.data.write(byte);
                }
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
        report.fifo_functional = self.test_fifo()?;

        // Verify baud rate configuration
        report.baud_config_valid = self.verify_baud_rate()?;

        Ok(report)
    }

    /// Test FIFO functionality
    fn test_fifo(&mut self) -> PortResult<bool> {
        // SAFETY: Writing to FIFO control register to enable FIFO
        unsafe {
            self.fifo.write(FIFO_ENABLE_CLEAR);
        }
        super::wait_short();

        // SAFETY: Reading IIR to check FIFO status
        let iir = unsafe { self.fifo.read() };
        Ok((iir & 0xC0) == 0xC0)
    }

    /// Verify baud rate configuration
    fn verify_baud_rate(&mut self) -> PortResult<bool> {
        // SAFETY: Temporarily enable DLAB to read divisor latch
        unsafe {
            let original_lcr = self.line_control.read();
            self.line_control.write(original_lcr | DLAB_ENABLE);

            let dll = self.data.read();
            let dlh = self.interrupt_enable.read();

            self.line_control.write(original_lcr);

            let divisor = ((dlh as u16) << 8) | dll as u16;
            Ok(divisor == BAUD_RATE_DIVISOR)
        }
    }

    /// Perform port operation with centralized unsafe access
    ///
    /// # Safety
    ///
    /// All port I/O is performed within this function under the following guarantees:
    /// - Port addresses are validated compile-time constants
    /// - Exclusive access via SERIAL_PORTS mutex
    /// - Operations follow UART 16550 specification
    /// - No memory safety violations possible with port I/O
    fn perform_op(&mut self, op: PortOp) -> PortResult<u8> {
        // SAFETY: See function documentation
        unsafe {
            match op {
                PortOp::Configure => {
                    // Step 1: Disable all interrupts
                    self.interrupt_enable.write(0x00);

                    // Step 2: Enable DLAB to set baud rate
                    self.line_control.write(DLAB_ENABLE);
                    self.data.write((BAUD_RATE_DIVISOR & 0xFF) as u8);
                    self.interrupt_enable
                        .write(((BAUD_RATE_DIVISOR >> 8) & 0xFF) as u8);

                    // Step 3: Set 8N1 and clear DLAB
                    self.line_control.write(CONFIG_8N1);

                    // Step 4: Enable and clear FIFO
                    self.fifo.write(FIFO_ENABLE_CLEAR);

                    // Step 5: Set modem control (DTR, RTS, OUT2)
                    self.modem_control.write(MODEM_CTRL_ENABLE_IRQ_RTS_DSR);

                    Ok(1)
                }
                PortOp::ScratchWrite(v) => {
                    self.scratch.write(v);
                    Ok(1)
                }
                PortOp::ScratchRead => Ok(self.scratch.read()),
                PortOp::LineStatusRead => Ok(self.line_status.read()),
                PortOp::ModemStatusRead => Ok(self.modem_status.read()),
            }
        }
    }

    /// Get adaptive timeout statistics
    pub fn timeout_stats(&self) -> (u32, u32, u32) {
        self.adaptive_timeout.stats()
    }

    /// Reset adaptive timeout statistics
    pub fn reset_timeout_stats(&mut self) {
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
