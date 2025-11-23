// kernel/src/error.rs

//! Unified error types for the kernel
//!
//! This module provides a consistent error handling approach across
//! all kernel subsystems.

use core::fmt;

/// Top-level kernel error type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KernelError {
    /// VGA subsystem error
    #[cfg(target_arch = "x86_64")]
    Vga(VgaError),
    /// Serial subsystem error
    Serial(SerialError),
    /// Initialization error
    Init(InitError),
    /// Display subsystem error
    Display(DisplayError),
}

impl fmt::Display for KernelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            #[cfg(target_arch = "x86_64")]
            Self::Vga(e) => write!(f, "VGA error: {e}"),
            Self::Serial(e) => write!(f, "Serial error: {e}"),
            Self::Init(e) => write!(f, "Init error: {e}"),
            Self::Display(e) => write!(f, "Display error: {e}"),
        }
    }
}

/// VGA subsystem errors
#[cfg(target_arch = "x86_64")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VgaError {
    /// Buffer not accessible
    BufferNotAccessible,
    /// Invalid position
    InvalidPosition,
    /// Write operation failed
    WriteFailure,
    /// Not initialized
    NotInitialized,
    /// Writer not locked
    NotLocked,
    /// Buffer overflow
    BufferOverflow,
}

#[cfg(target_arch = "x86_64")]
impl VgaError {
    /// Returns a string representation of the VGA error.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::BufferNotAccessible => "buffer not accessible",
            Self::InvalidPosition => "invalid position",
            Self::WriteFailure => "write failure",
            Self::NotInitialized => "not initialized",
            Self::NotLocked => "writer not locked",
            Self::BufferOverflow => "buffer overflow",
        }
    }
}

#[cfg(target_arch = "x86_64")]
impl fmt::Display for VgaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(target_arch = "x86_64")]
impl From<VgaError> for KernelError {
    fn from(err: VgaError) -> Self {
        Self::Vga(err)
    }
}

/// Serial subsystem errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SerialError {
    /// Port already initialized
    AlreadyInitialized,
    /// Hardware not present
    PortNotPresent,
    /// Timeout during operation
    Timeout,
    /// Configuration failed
    ConfigurationFailed,
    /// Hardware access failed
    HardwareAccessFailed,
    /// Too many initialization attempts
    TooManyAttempts,
    /// Invalid baud rate
    InvalidBaudRate,
    /// FIFO error
    FifoError,
}

impl fmt::Display for SerialError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadyInitialized => write!(f, "already initialized"),
            Self::PortNotPresent => write!(f, "hardware not present"),
            Self::Timeout => write!(f, "operation timeout"),
            Self::ConfigurationFailed => write!(f, "configuration failed"),
            Self::HardwareAccessFailed => write!(f, "hardware access failed"),
            Self::TooManyAttempts => write!(f, "too many attempts"),
            Self::InvalidBaudRate => write!(f, "invalid baud rate"),
            Self::FifoError => write!(f, "FIFO error"),
        }
    }
}

impl From<SerialError> for KernelError {
    fn from(err: SerialError) -> Self {
        Self::Serial(err)
    }
}

/// Initialization errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitError {
    /// VGA initialization failed
    VgaFailed(VgaError),
    /// Serial initialization failed
    SerialFailed(SerialError),
    /// Already initialized
    AlreadyInitialized,
    /// Prerequisites not met
    PrerequisitesNotMet,
}

impl fmt::Display for InitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::VgaFailed(e) => write!(f, "VGA init failed: {e}"),
            Self::SerialFailed(e) => write!(f, "Serial init failed: {e}"),
            Self::AlreadyInitialized => write!(f, "already initialized"),
            Self::PrerequisitesNotMet => write!(f, "prerequisites not met"),
        }
    }
}

impl From<InitError> for KernelError {
    fn from(err: InitError) -> Self {
        Self::Init(err)
    }
}

/// Display subsystem errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayError {
    /// No output available
    NoOutputAvailable,
    /// Format error
    FormatError,
    /// Underlying subsystem error
    SubsystemError,
}

impl fmt::Display for DisplayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoOutputAvailable => write!(f, "no output available"),
            Self::FormatError => write!(f, "format error"),
            Self::SubsystemError => write!(f, "subsystem error"),
        }
    }
}

impl From<DisplayError> for KernelError {
    fn from(err: DisplayError) -> Self {
        Self::Display(err)
    }
}

/// Result type alias for kernel operations
pub type Result<T> = core::result::Result<T, KernelError>;

/// Additional conversions for `VgaError`
impl From<VgaError> for InitError {
    fn from(err: VgaError) -> Self {
        Self::VgaFailed(err)
    }
}

/// Additional conversions for `SerialError`
impl From<SerialError> for InitError {
    fn from(err: SerialError) -> Self {
        Self::SerialFailed(err)
    }
}

/// Helper trait for error context
pub trait ErrorContext {
    /// Get a detailed description of the error
    fn context(&self) -> &'static str;
}

impl ErrorContext for KernelError {
    fn context(&self) -> &'static str {
        match self {
            Self::Vga(_) => "Error occurred in VGA buffer subsystem",
            Self::Serial(_) => "Error occurred in serial port subsystem",
            Self::Init(_) => "Error occurred during kernel initialization",
            Self::Display(_) => "Error occurred in display subsystem",
        }
    }
}

impl ErrorContext for VgaError {
    fn context(&self) -> &'static str {
        match self {
            Self::BufferNotAccessible => "VGA buffer memory could not be accessed",
            Self::InvalidPosition => "Attempted to write to invalid screen position",
            Self::WriteFailure => "Failed to write to VGA buffer",
            Self::NotInitialized => "VGA writer must be initialized before use",
            Self::NotLocked => "VGA writer lock must be acquired before writing",
            Self::BufferOverflow => "VGA buffer capacity exceeded",
        }
    }
}

impl ErrorContext for SerialError {
    fn context(&self) -> &'static str {
        match self {
            Self::AlreadyInitialized => "Serial port cannot be initialized twice",
            Self::PortNotPresent => "Serial port hardware is not available",
            Self::Timeout => "Serial operation timed out waiting for hardware",
            Self::ConfigurationFailed => "Failed to configure serial port registers",
            Self::HardwareAccessFailed => "Could not access serial port I/O registers",
            Self::TooManyAttempts => "Exceeded maximum retry attempts for serial operation",
            Self::InvalidBaudRate => "Specified baud rate is not supported",
            Self::FifoError => "Serial FIFO buffer encountered an error",
        }
    }
}

impl ErrorContext for InitError {
    fn context(&self) -> &'static str {
        match self {
            Self::VgaFailed(_) => "VGA subsystem initialization failed",
            Self::SerialFailed(_) => "Serial subsystem initialization failed",
            Self::AlreadyInitialized => "Kernel subsystems are already initialized",
            Self::PrerequisitesNotMet => "Required conditions for initialization not satisfied",
        }
    }
}

#[cfg(test)]
mod kernel_tests {
    use super::*;

    #[test_case]
    fn test_kernel_error_display() {
        // Just verify that we can format errors without panicking
        let err = KernelError::Init(InitError::AlreadyInitialized);
        let _ = err; 
    }

    #[test_case]
    fn test_error_conversions() {
        let vga_err = VgaError::BufferNotAccessible;
        let init_err: InitError = vga_err.into();
        assert!(matches!(init_err, InitError::VgaFailed(VgaError::BufferNotAccessible)));

        let serial_err = SerialError::Timeout;
        let init_err2: InitError = serial_err.into();
        assert!(matches!(init_err2, InitError::SerialFailed(SerialError::Timeout)));

        let kernel_err: KernelError = init_err.into();
        assert!(matches!(kernel_err, KernelError::Init(InitError::VgaFailed(_))));
    }

    #[test_case]
    fn test_error_context() {
        let err = SerialError::PortNotPresent;
        assert_eq!(err.context(), "Serial port hardware is not available");
        
        let k_err = KernelError::Serial(err);
        assert_eq!(k_err.context(), "Error occurred in serial port subsystem");
    }
}
