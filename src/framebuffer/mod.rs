// src/framebuffer/mod.rs

//! Framebuffer display driver for UEFI graphics mode
//!
//! This module provides pixel-based framebuffer rendering support for UEFI systems.
//! It complements the legacy VGA text mode driver and provides a fallback-compatible
//! display system.
//!
//! # Features
//!
//! - RGB/BGR pixel format support
//! - Software font rendering (PSF2 format)
//! - Automatic scrolling
//! - Color conversion from VGA palette
//! - Safe buffer access with bounds checking

pub mod font;
pub mod writer;

// Note: Bootloader 0.11 imports will be added when Cargo.toml is upgraded
// For now, we'll define placeholder types compatible with bootloader 0.11
use core::fmt;

/// Pixel format supported by the framebuffer
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    /// RGB format (Red, Green, Blue)
    Rgb,
    /// BGR format (Blue, Green, Red)
    Bgr,
    /// Single-component grayscale or indexed (unsupported for color)
    U8,
}

// Conversion will be implemented when bootloader 0.11 is integrated
// impl From<BootPixelFormat> for PixelFormat { ... }

/// RGB color representation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RgbColor {
    /// Red component (0-255)
    pub r: u8,
    /// Green component (0-255)
    pub g: u8,
    /// Blue component (0-255)
    pub b: u8,
}

impl RgbColor {
    /// Create a new RGB color
    #[must_use]
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Black color
    #[must_use]
    pub const fn black() -> Self {
        Self::new(0, 0, 0)
    }

    /// White color
    #[must_use]
    pub const fn white() -> Self {
        Self::new(255, 255, 255)
    }

    /// Convert to bytes based on pixel format
    #[must_use]
    pub const fn to_bytes(self, format: PixelFormat) -> [u8; 3] {
        match format {
            PixelFormat::Rgb => [self.r, self.g, self.b],
            PixelFormat::Bgr => [self.b, self.g, self.r],
            PixelFormat::U8 => {
                // Grayscale: simple average
                let gray = ((self.r as u16 + self.g as u16 + self.b as u16) / 3) as u8;
                [gray, gray, gray]
            }
        }
    }
}

/// Framebuffer information and configuration
pub struct FramebufferInfo {
    /// Framebuffer base address
    buffer: &'static mut [u8],
    /// Width in pixels
    width: usize,
    /// Height in pixels
    height: usize,
    /// Bytes per pixel (3 or 4)
    bytes_per_pixel: usize,
    /// Stride (bytes per scanline)
    stride: usize,
    /// Pixel format
    pixel_format: PixelFormat,
}

impl FramebufferInfo {
    /// Create a new framebuffer info manually
    ///
    /// # Safety
    ///
    /// The caller must ensure that the framebuffer memory is valid and
    /// exclusively accessible.
    #[allow(dead_code)]
    pub unsafe fn new(
        buffer: &'static mut [u8],
        width: usize,
        height: usize,
        bytes_per_pixel: usize,
        stride: usize,
        pixel_format: PixelFormat,
    ) -> Self {
        Self {
            buffer,
            width,
            height,
            bytes_per_pixel,
            stride,
            pixel_format,
        }
    }

    /// Get framebuffer dimensions
    #[must_use]
    pub const fn dimensions(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    /// Get pixel format
    #[must_use]
    pub const fn pixel_format(&self) -> PixelFormat {
        self.pixel_format
    }

    /// Write a pixel at the given coordinates
    ///
    /// # Errors
    ///
    /// Returns error if coordinates are out of bounds
    pub fn write_pixel(&mut self, x: usize, y: usize, color: RgbColor) -> Result<(), FramebufferError> {
        if x >= self.width || y >= self.height {
            return Err(FramebufferError::OutOfBounds);
        }

        let offset = y * self.stride + x * self.bytes_per_pixel;
        let bytes = color.to_bytes(self.pixel_format);

        if offset + self.bytes_per_pixel > self.buffer.len() {
            return Err(FramebufferError::BufferTooSmall);
        }

        // Write RGB/BGR bytes
        for (i, &byte) in bytes.iter().enumerate() {
            self.buffer[offset + i] = byte;
        }

        Ok(())
    }

    /// Fill a rectangle with a color
    pub fn fill_rect(
        &mut self,
        x: usize,
        y: usize,
        width: usize,
        height: usize,
        color: RgbColor,
    ) -> Result<(), FramebufferError> {
        for dy in 0..height {
            for dx in 0..width {
                self.write_pixel(x + dx, y + dy, color)?;
            }
        }
        Ok(())
    }

    /// Clear the entire framebuffer
    pub fn clear(&mut self, color: RgbColor) -> Result<(), FramebufferError> {
        self.fill_rect(0, 0, self.width, self.height, color)
    }

    /// Scroll the framebuffer up by one line (font height)
    ///
    /// # Arguments
    ///
    /// * `font_height` - Height of one line in pixels
    /// * `bg_color` - Background color for the new line
    pub fn scroll_up(&mut self, font_height: usize, bg_color: RgbColor) -> Result<(), FramebufferError> {
        // Move all rows up by font_height pixels
        let bytes_to_move = (self.height - font_height) * self.stride;
        let src_offset = font_height * self.stride;

        // Use copy_within to move the buffer contents
        self.buffer.copy_within(src_offset..src_offset + bytes_to_move, 0);

        // Clear the bottom line
        let bottom_y = self.height - font_height;
        self.fill_rect(0, bottom_y, self.width, font_height, bg_color)?;

        Ok(())
    }
}

/// Errors that can occur during framebuffer operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FramebufferError {
    /// Coordinates are out of bounds
    OutOfBounds,
    /// Buffer is too small for the operation
    BufferTooSmall,
    /// Framebuffer is not available
    Unavailable,
    /// Invalid font data
    InvalidFont,
}

impl FramebufferError {
    /// Convert the error to a descriptive string
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::OutOfBounds => "coordinates out of bounds",
            Self::BufferTooSmall => "framebuffer buffer too small",
            Self::Unavailable => "framebuffer not available",
            Self::InvalidFont => "invalid font data",
        }
    }
}

impl fmt::Display for FramebufferError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Convert VGA ColorCode to RGB
///
/// Uses the standard VGA color palette
#[must_use]
pub const fn colorcode_to_rgb(color: u8) -> RgbColor {
    match color {
        0 => RgbColor::new(0, 0, 0),         // Black
        1 => RgbColor::new(0, 0, 170),       // Blue
        2 => RgbColor::new(0, 170, 0),       // Green
        3 => RgbColor::new(0, 170, 170),     // Cyan
        4 => RgbColor::new(170, 0, 0),       // Red
        5 => RgbColor::new(170, 0, 170),     // Magenta
        6 => RgbColor::new(170, 85, 0),      // Brown
        7 => RgbColor::new(170, 170, 170),   // Light Gray
        8 => RgbColor::new(85, 85, 85),      // Dark Gray
        9 => RgbColor::new(85, 85, 255),     // Light Blue
        10 => RgbColor::new(85, 255, 85),    // Light Green
        11 => RgbColor::new(85, 255, 255),   // Light Cyan
        12 => RgbColor::new(255, 85, 85),    // Light Red
        13 => RgbColor::new(255, 85, 255),   // Light Magenta
        14 => RgbColor::new(255, 255, 85),   // Yellow
        15 => RgbColor::new(255, 255, 255),  // White
        _ => RgbColor::new(170, 170, 170),   // Default to light gray
    }
}
