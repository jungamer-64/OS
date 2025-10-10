// src/error.rs

//! Unified error types for the kernel
//!
//! This module provides a consistent error handling approach across
//! all kernel subsystems.

use core::fmt;

/// Top-level kernel error type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KernelError {
    /// VGA subsystem error
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
            KernelError::Vga(e) => write!(f, "VGA error: {}", e),
            KernelError::Serial(e) => write!(f, "Serial error: {}", e),
            KernelError::Init(e) => write!(f, "Init error: {}", e),
            KernelError::Display(e) => write!(f, "Display error: {}", e),
        }
    }
}

/// VGA subsystem errors
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

impl VgaError {
    pub const fn as_str(&self) -> &'static str {
        match self {
            VgaError::BufferNotAccessible => "buffer not accessible",
            VgaError::InvalidPosition => "invalid position",
            VgaError::WriteFailure => "write failure",
            VgaError::NotInitialized => "not initialized",
            VgaError::NotLocked => "writer not locked",
            VgaError::BufferOverflow => "buffer overflow",
        }
    }
}

impl fmt::Display for VgaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<VgaError> for KernelError {
    fn from(err: VgaError) -> Self {
        KernelError::Vga(err)
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
            SerialError::AlreadyInitialized => write!(f, "already initialized"),
            SerialError::PortNotPresent => write!(f, "hardware not present"),
            SerialError::Timeout => write!(f, "operation timeout"),
            SerialError::ConfigurationFailed => write!(f, "configuration failed"),
            SerialError::HardwareAccessFailed => write!(f, "hardware access failed"),
            SerialError::TooManyAttempts => write!(f, "too many attempts"),
            SerialError::InvalidBaudRate => write!(f, "invalid baud rate"),
            SerialError::FifoError => write!(f, "FIFO error"),
        }
    }
}

impl From<SerialError> for KernelError {
    fn from(err: SerialError) -> Self {
        KernelError::Serial(err)
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
            InitError::VgaFailed(e) => write!(f, "VGA init failed: {}", e),
            InitError::SerialFailed(e) => write!(f, "Serial init failed: {}", e),
            InitError::AlreadyInitialized => write!(f, "already initialized"),
            InitError::PrerequisitesNotMet => write!(f, "prerequisites not met"),
        }
    }
}

impl From<InitError> for KernelError {
    fn from(err: InitError) -> Self {
        KernelError::Init(err)
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
            DisplayError::NoOutputAvailable => write!(f, "no output available"),
            DisplayError::FormatError => write!(f, "format error"),
            DisplayError::SubsystemError => write!(f, "subsystem error"),
        }
    }
}

impl From<DisplayError> for KernelError {
    fn from(err: DisplayError) -> Self {
        KernelError::Display(err)
    }
}

/// Result type alias for kernel operations
pub type Result<T> = core::result::Result<T, KernelError>;

/// Additional conversions for VgaError
impl From<VgaError> for InitError {
    fn from(err: VgaError) -> Self {
        InitError::VgaFailed(err)
    }
}

/// Additional conversions for SerialError
impl From<SerialError> for InitError {
    fn from(err: SerialError) -> Self {
        InitError::SerialFailed(err)
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
            KernelError::Vga(_) => "Error occurred in VGA buffer subsystem",
            KernelError::Serial(_) => "Error occurred in serial port subsystem",
            KernelError::Init(_) => "Error occurred during kernel initialization",
            KernelError::Display(_) => "Error occurred in display subsystem",
        }
    }
}

impl ErrorContext for VgaError {
    fn context(&self) -> &'static str {
        match self {
            VgaError::BufferNotAccessible => "VGA buffer memory could not be accessed",
            VgaError::InvalidPosition => "Attempted to write to invalid screen position",
            VgaError::WriteFailure => "Failed to write to VGA buffer",
            VgaError::NotInitialized => "VGA writer must be initialized before use",
            VgaError::NotLocked => "VGA writer lock must be acquired before writing",
            VgaError::BufferOverflow => "VGA buffer capacity exceeded",
        }
    }
}

impl ErrorContext for SerialError {
    fn context(&self) -> &'static str {
        match self {
            SerialError::AlreadyInitialized => "Serial port cannot be initialized twice",
            SerialError::PortNotPresent => "Serial port hardware is not available",
            SerialError::Timeout => "Serial operation timed out waiting for hardware",
            SerialError::ConfigurationFailed => "Failed to configure serial port registers",
            SerialError::HardwareAccessFailed => "Could not access serial port I/O registers",
            SerialError::TooManyAttempts => "Exceeded maximum retry attempts for serial operation",
            SerialError::InvalidBaudRate => "Specified baud rate is not supported",
            SerialError::FifoError => "Serial FIFO buffer encountered an error",
        }
    }
}

impl ErrorContext for InitError {
    fn context(&self) -> &'static str {
        match self {
            InitError::VgaFailed(_) => "VGA subsystem initialization failed",
            InitError::SerialFailed(_) => "Serial subsystem initialization failed",
            InitError::AlreadyInitialized => "Kernel subsystems are already initialized",
            InitError::PrerequisitesNotMet => {
                "Required conditions for initialization not satisfied"
            }
        }
    }
}

impl ErrorContext for DisplayError {
    fn context(&self) -> &'static str {
        match self {
            DisplayError::NoOutputAvailable => "No display output methods are available",
            DisplayError::FormatError => "Failed to format output string",
            DisplayError::SubsystemError => "Underlying display subsystem error",
        }
    }
}
