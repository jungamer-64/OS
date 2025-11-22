//! Display color definitions and color code management

/// Standard 16-color palette (VGA compatible)
#[allow(dead_code)]
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
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
    pub const fn new(fg: Color, bg: Color) -> Self {
        Self((bg as u8) << 4 | (fg as u8))
    }

    /// Get the raw byte value
    pub const fn as_u8(self) -> u8 {
        self.0
    }

    /// Default color scheme (light gray on black)
    pub const fn normal() -> Self {
        Self::new(Color::LightGray, Color::Black)
    }

    /// Info color scheme (light cyan on black)
    pub const fn info() -> Self {
        Self::new(Color::LightCyan, Color::Black)
    }

    /// Success color scheme (light green on black)
    pub const fn success() -> Self {
        Self::new(Color::LightGreen, Color::Black)
    }

    /// Warning color scheme (yellow on black)
    pub const fn warning() -> Self {
        Self::new(Color::Yellow, Color::Black)
    }

    /// Error color scheme (light red on black)
    pub const fn error() -> Self {
        Self::new(Color::LightRed, Color::Black)
    }

    /// Panic color scheme (white on red)
    pub const fn panic() -> Self {
        Self::new(Color::White, Color::Red)
    }
}

#[cfg(all(test, feature = "std-tests"))]
mod tests {
    use super::*;

    #[test]
    fn test_color_code_encoding() {
        let color = ColorCode::new(Color::White, Color::Red);
        assert_eq!(color.as_u8(), 0x4F);
    }
}
