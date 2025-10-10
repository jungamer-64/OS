// src/constants.rs

//! Kernel constants and configuration values
//!
//! This module centralizes all constant values used throughout the kernel,
//! including feature descriptions, system information, hardware parameters,
//! and UI messages.
//!
//! # Design Philosophy
//!
//! - All magic numbers are defined as named constants
//! - Hardware-specific values are documented with rationale
//! - Configuration values are validated at compile time where possible
//! - Related constants are grouped together
//!
//! # Safety
//!
//! All constants in this module are safe to use. Hardware I/O port
//! addresses are well-known standard values for PC/AT architecture.

/// List of major kernel features and improvements
///
/// These features are displayed during boot to inform the user
/// about the kernel's capabilities.
pub const FEATURES: &[&str] = &[
    "Replaced static mut with Mutex (SAFE!)",
    "Interrupt-safe locking (no deadlock!)",
    "Implemented fmt::Write trait",
    "Optimized scroll with copy_nonoverlapping",
    "Modular code structure (vga_buffer, serial)",
    "Serial FIFO transmit check with timeout",
    "VGA color support (16 colors)",
    "VGA auto-scroll with bounds checking",
    "CPU hlt instruction for power saving",
    "Detailed panic handler with state dump",
    "Hardware detection before use",
    "Idempotent initialization",
    "Comprehensive error handling",
    "Buffer overflow protection",
];

/// System component information
///
/// Each tuple contains a (label, value) pair describing
/// a kernel component or configuration.
pub const SYSTEM_INFO: &[(&str, &str)] = &[
    ("Bootloader", "0.9.33"),
    ("Serial", "COM1 (0x3F8) with FIFO check"),
    ("VGA Mode", "Text 80x25 (0xB8000)"),
    ("Safety", "Mutex + Interrupt disabling"),
];

/// Usage hints displayed to serial output
///
/// These messages provide guidance on interacting with
/// the kernel when running under QEMU or similar emulators.
pub const SERIAL_HINTS: &[&str] = &[
    "Kernel running. System in low-power hlt loop.",
    "Press Ctrl+A, X to exit QEMU.",
    "Serial output: COM1 at 38400 baud.",
];

// ============================================================================
// Initialization Messages
// ============================================================================

/// Serial banner lines emitted after a successful initialization.
pub const SERIAL_INIT_SUCCESS_LINES: &[&str] = &[
    "========================================",
    "=== Rust OS Kernel Started ===",
    "========================================",
    "",
    "[OK] Serial port initialized successfully",
    "     - Baud rate: 38400",
    "     - Configuration: 8N1",
    "     - FIFO: Enabled and verified",
    "     - Hardware detection: Passed",
    "",
];

/// Serial notice when initialization discovers the port was already configured.
pub const SERIAL_ALREADY_INITIALIZED_LINES: &[&str] = &[
    "[INFO] Serial port already initialized",
    "       Skipping hardware setup",
];

/// Static listing of safety features for serial output.
pub const SERIAL_SAFETY_FEATURE_LINES: &[&str] = &[
    "[SAFETY] Kernel safety features:",
    "     - Mutex-protected I/O (interrupt-safe)",
    "     - Boundary checking on all buffer writes",
    "     - Hardware validation before use",
    "     - Deadlock prevention via interrupt disabling",
    "     - Timeout protection on hardware operations",
    "     - Idempotent initialization",
    "",
];

/// Log messages emitted before entering the idle loop.
pub const SERIAL_IDLE_LOOP_LINES: &[&str] = &[
    "[INFO] Entering low-power idle loop",
    "       CPU will execute hlt instruction",
    "       System ready for interrupts",
    "",
];

/// Messages displayed when non-critical initialization fails.
pub const SERIAL_NON_CRITICAL_CONTINUATION_LINES: &[&str] =
    &["       Continuing with available subsystems", ""];

// ============================================================================
// Hardware Constants - Serial Port (UART 16550)
// ============================================================================

/// COM1 base I/O port address
///
/// Standard PC/AT I/O port for COM1 (first serial port).
/// This is a de-facto standard that has been consistent since the IBM PC/AT.
///
/// Memory-mapped I/O alternative: Some systems use memory-mapped UART,
/// but standard PC systems always use I/O ports.
pub const SERIAL_IO_PORT: u16 = 0x3F8;

/// Baud rate divisor for 38400 baud
///
/// Calculation: 115200 / 38400 = 3
/// Base frequency: 115200 Hz (standard UART oscillator / 16)
/// Target baud: 38400 (good balance of speed and reliability)
///
/// Common alternatives:
/// - 115200 baud: divisor = 1 (fastest, may have errors on poor hardware)
/// - 57600 baud: divisor = 2
/// - 38400 baud: divisor = 3 (chosen for reliability)
/// - 19200 baud: divisor = 6
/// - 9600 baud: divisor = 12 (very reliable, slower)
pub const BAUD_RATE_DIVISOR: u16 = 3;

/// Timeout iterations for serial port operations
///
/// This value is a conservative estimate for hardware response time.
/// At 38400 baud with FIFO enabled, the transmit buffer should empty
/// in microseconds, but we allow up to ~10ms for safety margin.
///
/// Value chosen based on:
/// - Typical CPU clock speeds (1+ GHz)
/// - UART FIFO depth (16 bytes)
/// - Transmission time at 38400 baud
///
/// On modern CPUs, this provides plenty of time for hardware response
/// while preventing indefinite hangs.
pub const TIMEOUT_ITERATIONS: u32 = 10_000_000;

/// FIFO control register value: enable and clear FIFOs
///
/// Bit layout:
/// - Bit 0: Enable FIFO (1 = enabled)
/// - Bit 1: Clear receive FIFO (1 = clear)
/// - Bit 2: Clear transmit FIFO (1 = clear)
/// - Bit 3: DMA mode select (0 = mode 0)
/// - Bit 6-7: Interrupt trigger level (11 = 14 bytes)
///
/// Value 0xC7 = 0b11000111:
/// - Enables FIFO
/// - Clears both FIFOs
/// - Sets 14-byte trigger level
pub const FIFO_ENABLE_CLEAR: u8 = 0xC7;

/// Modem control register value
///
/// Bit layout:
/// - Bit 0: DTR (Data Terminal Ready) - 1 = active
/// - Bit 1: RTS (Request To Send) - 1 = active
/// - Bit 2: OUT1 - auxiliary output (0 = inactive)
/// - Bit 3: OUT2 - enables interrupts when set (1 = enable IRQ)
/// - Bit 4: Loopback mode (0 = normal operation)
///
/// Value 0x0B = 0b00001011:
/// - DTR active (bit 0)
/// - RTS active (bit 1)
/// - OUT2 active for IRQ support (bit 3)
pub const MODEM_CTRL_ENABLE_IRQ_RTS_DSR: u8 = 0x0B;

/// Divisor Latch Access Bit - enables baud rate configuration
///
/// When DLAB is set in Line Control Register:
/// - Register 0 (Data) becomes Divisor Latch Low
/// - Register 1 (IER) becomes Divisor Latch High
///
/// This allows setting the baud rate divisor.
pub const DLAB_ENABLE: u8 = 0x80;

/// Line control: 8 data bits, No parity, 1 stop bit (8N1)
///
/// Bit layout:
/// - Bit 0-1: Word length (11 = 8 bits)
/// - Bit 2: Stop bits (0 = 1 stop bit)
/// - Bit 3-5: Parity (000 = no parity)
/// - Bit 6: Break control (0 = no break)
/// - Bit 7: DLAB (0 = normal operation)
///
/// Value 0x03 = 0b00000011:
/// - 8 data bits
/// - 1 stop bit
/// - No parity
pub const CONFIG_8N1: u8 = 0x03;

/// Line Status Register: Transmit Holding Register Empty bit
///
/// When this bit is set (1), the transmit buffer is empty and
/// ready to accept a new byte.
///
/// Bit 5 of LSR = THRE (Transmitter Holding Register Empty)
pub const LSR_TRANSMIT_EMPTY: u8 = 0x20;

/// Scratch register test pattern - primary
///
/// Used for hardware detection. The scratch register should
/// reliably store and return this value if hardware is present.
///
/// Pattern chosen to have good bit distribution (alternating pattern).
pub const SCRATCH_TEST_PRIMARY: u8 = 0xAA;

/// Scratch register test pattern - secondary
///
/// Inverse of primary pattern for additional validation.
pub const SCRATCH_TEST_SECONDARY: u8 = 0x55;

// ============================================================================
// Compile-Time Validation
// ============================================================================

// Ensure baud rate divisor is non-zero (would cause divide-by-zero in UART)
const _: () = assert!(BAUD_RATE_DIVISOR > 0, "Baud rate divisor must be non-zero");

// Ensure timeout is reasonable (not too small to be useless, not too large to hang)
const _: () = assert!(
    TIMEOUT_ITERATIONS >= 1000 && TIMEOUT_ITERATIONS <= 100_000_000,
    "Timeout iterations must be between 1000 and 100M"
);

// Ensure serial port address is in valid I/O port range
const _: () = assert!(
    SERIAL_IO_PORT >= 0x100 && SERIAL_IO_PORT < 0xFFFF,
    "Serial port address must be in valid I/O range"
);

// Ensure test patterns are different (otherwise test is meaningless)
const _: () = assert!(
    SCRATCH_TEST_PRIMARY != SCRATCH_TEST_SECONDARY,
    "Scratch test patterns must be different"
);

// ============================================================================
// Type-Safe Configuration
// ============================================================================

/// Serial port configuration structure
///
/// Encapsulates all serial port configuration in a type-safe manner.
/// Can be used to validate configuration before applying to hardware.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct SerialConfig {
    /// Base I/O port address
    pub port: u16,
    /// Baud rate divisor
    pub divisor: u16,
    /// Data bits (5-8)
    pub data_bits: u8,
    /// Parity setting
    pub parity: Parity,
    /// Stop bits
    pub stop_bits: StopBits,
}

/// Parity configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum Parity {
    /// No parity bit
    None,
    /// Odd parity
    Odd,
    /// Even parity
    Even,
}

/// Stop bits configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum StopBits {
    /// One stop bit
    One,
    /// Two stop bits (or 1.5 for 5-bit data)
    Two,
}

impl SerialConfig {
    /// Default configuration: 38400 baud, 8N1
    #[must_use]
    #[allow(dead_code)]
    pub const fn default() -> Self {
        Self {
            port: SERIAL_IO_PORT,
            divisor: BAUD_RATE_DIVISOR,
            data_bits: 8,
            parity: Parity::None,
            stop_bits: StopBits::One,
        }
    }

    /// Validate configuration parameters
    #[must_use]
    #[allow(dead_code)]
    pub const fn is_valid(&self) -> bool {
        // Port must be in valid range
        if self.port < 0x100 || self.port == u16::MAX {
            return false;
        }

        // Divisor must be non-zero
        if self.divisor == 0 {
            return false;
        }

        // Data bits must be 5-8
        if self.data_bits < 5 || self.data_bits > 8 {
            return false;
        }

        true
    }
}

#[cfg(all(test, feature = "std-tests"))]
mod tests {
    use super::*;

    #[test]
    fn test_serial_config_validation() {
        let config = SerialConfig::default();
        assert!(config.is_valid());
    }

    #[test]
    fn test_serial_config_invalid_port() {
        let mut config = SerialConfig::default();
        config.port = 0x50; // Too low
        assert!(!config.is_valid());
    }

    #[test]
    fn test_serial_config_zero_divisor() {
        let mut config = SerialConfig::default();
        config.divisor = 0;
        assert!(!config.is_valid());
    }

    #[test]
    fn test_baud_rate_calculation() {
        // Verify baud rate calculation is correct
        let expected_baud = 115_200 / u32::from(BAUD_RATE_DIVISOR);
        assert_eq!(expected_baud, 38400);
    }
}
