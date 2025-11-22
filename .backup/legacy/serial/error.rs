// src/serial/error.rs

//! Error types for serial port operations

/// Serial port initialization result
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitError {
    /// Port already initialized (not an error, just informational)
    AlreadyInitialized,
    /// Hardware not present or not responding
    PortNotPresent,
    /// Hardware timeout during initialization
    Timeout,
    /// Configuration failed
    ConfigurationFailed,
    /// Hardware access failed（将来の詳細エラー処理で使用予定）
    HardwareAccessFailed,
    /// Too many initialization attempts
    TooManyAttempts,
}

impl core::fmt::Display for InitError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::AlreadyInitialized => write!(f, "Serial port already initialized"),
            Self::PortNotPresent => write!(f, "Serial port hardware not present"),
            Self::Timeout => write!(f, "Serial port initialization timeout"),
            Self::ConfigurationFailed => write!(f, "Serial port configuration failed"),
            Self::HardwareAccessFailed => write!(f, "Serial port hardware access failed"),
            Self::TooManyAttempts => {
                write!(f, "Too many serial port initialization attempts")
            }
        }
    }
}

#[cfg(all(test, feature = "std-tests"))]
mod tests {
    use super::*;

    #[test]
    fn test_init_error_display() {
        assert_eq!(
            format!("{}", InitError::PortNotPresent),
            "Serial port hardware not present"
        );
    }
}
