// src/display/backend.rs

//! Display hardware abstraction layer.
//!
//! This module bridges the high-level display code with the actual
//! hardware implementation (VGA text buffer today, but extensible to
//! other targets). By hiding the concrete hardware behind a trait we
//! can swap implementations, provide stubs for tests, and report
//! descriptive errors without sprinkling low-level details throughout
//! the codebase.

use crate::display::color::ColorCode;
#[cfg(target_arch = "x86_64")]
use crate::vga_buffer::{self, VgaError};
use crate::framebuffer::FramebufferError;
use core::fmt;

/// Errors that can originate from display backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayError {
    /// Underlying VGA (or similar) hardware reported an error.
    #[cfg(target_arch = "x86_64")]
    Hardware(VgaError),
    /// Framebuffer hardware reported an error.
    Framebuffer(FramebufferError),
    /// No display hardware is currently available.
    Unavailable,
}

impl DisplayError {
    /// Convert the error into a descriptive static string.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            #[cfg(target_arch = "x86_64")]
            Self::Hardware(err) => err.as_str(),
            Self::Framebuffer(err) => err.as_str(),
            Self::Unavailable => "display hardware unavailable",
        }
    }
}

impl fmt::Display for DisplayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(target_arch = "x86_64")]
impl From<VgaError> for DisplayError {
    fn from(value: VgaError) -> Self {
        Self::Hardware(value)
    }
}

impl From<FramebufferError> for DisplayError {
    fn from(value: FramebufferError) -> Self {
        Self::Framebuffer(value)
    }
}

/// Unified trait implemented by all display backends.
pub trait DisplayHardware {
    /// Returns true when the backend has access to a physical display.
    fn is_available(&self) -> bool;

    /// Write colored text to the display.
    ///
    /// # Errors
    ///
    /// Returns [`DisplayError::Unavailable`] if the display cannot be used or
    /// propagates the underlying hardware failure.
    fn write_colored(&mut self, text: &str, color: ColorCode) -> Result<(), DisplayError>;

    /// Clear the display contents.
    ///
    /// # Errors
    ///
    /// Returns [`DisplayError::Unavailable`] if the backend cannot perform the
    /// operation.
    fn clear(&mut self) -> Result<(), DisplayError> {
        Err(DisplayError::Unavailable)
    }

    /// Update the active text color (if supported).
    ///
    /// # Errors
    ///
    /// Returns [`DisplayError::Unavailable`] when the backend does not expose
    /// a programmable color setting.
    fn set_color(&mut self, _color: ColorCode) -> Result<(), DisplayError> {
        Err(DisplayError::Unavailable)
    }
}

/// VGA-backed display implementation used on x86 hardware.
pub struct VgaDisplay;

impl VgaDisplay {
    /// Construct a new VGA display backend.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Default for VgaDisplay {
    fn default() -> Self {
        Self::new()
    }
}

impl DisplayHardware for VgaDisplay {
    fn is_available(&self) -> bool {
        vga_buffer::is_accessible()
    }

    fn write_colored(&mut self, text: &str, color: ColorCode) -> Result<(), DisplayError> {
        vga_buffer::print_colored(text, color).map_err(DisplayError::from)
    }

    fn clear(&mut self) -> Result<(), DisplayError> {
        vga_buffer::clear().map_err(DisplayError::from)
    }

    fn set_color(&mut self, color: ColorCode) -> Result<(), DisplayError> {
        vga_buffer::set_color(color).map_err(DisplayError::from)
    }
}

/// Stub backend used on platforms without VGA access.
#[derive(Clone, Copy, Debug, Default)]
pub struct StubDisplay {
    accessible: bool,
}

impl StubDisplay {
    /// Create a new stub backend that reports as unavailable.
    #[must_use]
    pub const fn new() -> Self {
        Self { accessible: false }
    }

    /// Create a stub backend with a predetermined availability flag.
    #[must_use]
    pub const fn with_accessible(accessible: bool) -> Self {
        Self { accessible }
    }
}

impl DisplayHardware for StubDisplay {
    fn is_available(&self) -> bool {
        self.accessible
    }

    fn write_colored(&mut self, _text: &str, _color: ColorCode) -> Result<(), DisplayError> {
        Err(DisplayError::Unavailable)
    }
}

/// Framebuffer-backed display implementation for UEFI graphics mode.
///
/// Note: This backend requires bootloader 0.11 to obtain framebuffer info.
/// Until then, it will report as unavailable.
pub struct FramebufferDisplay {
    // Placeholder: will store FramebufferInfo and FramebufferWriter
    // when integrated with bootloader 0.11
    available: bool,
}

impl FramebufferDisplay {
    /// Create a new framebuffer display backend.
    ///
    /// Note: Will report unavailable until bootloader 0.11 integration
    #[must_use]
    pub const fn new() -> Self {
        Self { available: false }
    }

    /// Create framebuffer display from bootloader FrameBuffer
    ///
    /// This will be implemented in Phase 3 when bootloader 0.11 is integrated.
    ///
    /// # Safety
    ///
    /// Caller must ensure framebuffer memory is valid and exclusive.
    #[allow(dead_code)]
    pub unsafe fn from_bootloader(_fb: ()) -> Self {
        // Placeholder for bootloader 0.11 integration
        // Will create FramebufferInfo and FramebufferWriter here
        Self { available: false }
    }
}

impl Default for FramebufferDisplay {
    fn default() -> Self {
        Self::new()
    }
}

impl DisplayHardware for FramebufferDisplay {
    fn is_available(&self) -> bool {
        self.available
    }

    fn write_colored(&mut self, _text: &str, _color: ColorCode) -> Result<(), DisplayError> {
        // Will be implemented with actual framebuffer writer in Phase 3
        Err(DisplayError::Unavailable)
    }

    fn clear(&mut self) -> Result<(), DisplayError> {
        // Will be implemented with actual framebuffer writer in Phase 3
        Err(DisplayError::Unavailable)
    }

    fn set_color(&mut self, _color: ColorCode) -> Result<(), DisplayError> {
        // Will be implemented with actual framebuffer writer in Phase 3
        Err(DisplayError::Unavailable)
    }
}

#[cfg(target_arch = "x86_64")]
pub type DefaultDisplayBackend = VgaDisplay;

#[cfg(not(target_arch = "x86_64"))]
pub type DefaultDisplayBackend = StubDisplay;

/// Helper that constructs the default backend for the current platform.
#[must_use]
pub const fn default_display_backend() -> DefaultDisplayBackend {
    DefaultDisplayBackend::new()
}

#[cfg(test)]
mod kernel_tests {
    use super::*;

    #[test_case]
    fn test_stub_display() {
        let stub = StubDisplay::new();
        assert!(!stub.is_available());
        
        let mut stub = StubDisplay::with_accessible(true);
        assert!(stub.is_available());
        
        // Stub always returns Unavailable error for writes
        assert_eq!(
            stub.write_colored("test", ColorCode::normal()),
            Err(DisplayError::Unavailable)
        );
    }

    #[test_case]
    fn test_display_error_str() {
        let err = DisplayError::Unavailable;
        assert_eq!(err.as_str(), "display hardware unavailable");
    }
}
