// src/vga_buffer/color.rs

//! VGA color definitions and color code management

/// VGA color codes (4-bit color palette)
#[allow(dead_code)]
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VgaColor {
    Black = 0,
    Blue = 1,
    Green = 2,
    Cyan = 3,
    Red = 4,
    Magenta = 5,
    Brown = 6,
    LightGray = 7,
    DarkGray = 8,
    LightBlue = 9,
    LightGreen = 10,
    LightCyan = 11,
    LightRed = 12,
    Pink = 13,
    Yellow = 14,
    White = 15,
}

/// Color code combining foreground and background colors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorCode(u8);

impl ColorCode {
    /// Create a new color code from foreground and background colors
    pub const fn new(fg: VgaColor, bg: VgaColor) -> Self {
        Self((bg as u8) << 4 | (fg as u8))
    }

    /// Get the raw byte value
    pub const fn as_u8(self) -> u8 {
        self.0
    }

    /// Default color scheme (light gray on black)
    pub const fn normal() -> Self {
        Self::new(VgaColor::LightGray, VgaColor::Black)
    }

    /// Info color scheme (light cyan on black)
    pub const fn info() -> Self {
        Self::new(VgaColor::LightCyan, VgaColor::Black)
    }

    /// Success color scheme (light green on black)
    pub const fn success() -> Self {
        Self::new(VgaColor::LightGreen, VgaColor::Black)
    }

    /// Warning color scheme (yellow on black)
    pub const fn warning() -> Self {
        Self::new(VgaColor::Yellow, VgaColor::Black)
    }

    /// Error color scheme (light red on black)
    pub const fn error() -> Self {
        Self::new(VgaColor::LightRed, VgaColor::Black)
    }

    /// Panic color scheme (white on red)
    pub const fn panic() -> Self {
        Self::new(VgaColor::White, VgaColor::Red)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_code_encoding() {
        let color = ColorCode::new(VgaColor::White, VgaColor::Red);
        assert_eq!(color.as_u8(), 0x4F);
    }
}
