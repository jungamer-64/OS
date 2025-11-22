// src/framebuffer/font.rs

//! Font rendering for framebuffer display
//!
//! This module provides bitmap font support for rendering text on the framebuffer.
//! Currently supports a built-in 8x16 ASCII font.

use super::{FramebufferError, FramebufferInfo, RgbColor};

/// Built-in 8x16 bitmap font data (ASCII characters 32-126)
///
/// Each character is 16 bytes (8 pixels wide, 16 pixels tall)
/// Bits are stored row by row, MSB to LSB
static FONT_DATA: &[u8] = include_bytes!("../../assets/font/basic_8x16.bin");

/// Font metrics
pub const FONT_WIDTH: usize = 8;
pub const FONT_HEIGHT: usize = 16;
const CHARS_COUNT: usize = 95; // ASCII 32-126
const BYTES_PER_CHAR: usize = FONT_HEIGHT;

/// Get font bitmap for a character
///
/// Returns None if the character is not in the supported range
#[must_use]
pub fn get_char_bitmap(c: char) -> Option<&'static [u8]> {
    let code = c as usize;
    
    // Only ASCII printable characters (32-126)
    if code < 32 || code > 126 {
        // Return space character for unsupported chars
        return get_char_bitmap(' ');
    }

    let index = code - 32;
    if index >= CHARS_COUNT {
        return None;
    }

    let start = index * BYTES_PER_CHAR;
    let end = start + BYTES_PER_CHAR;

    if end > FONT_DATA.len() {
        return None;
    }

    Some(&FONT_DATA[start..end])
}

/// Draw a character on the framebuffer
///
/// # Arguments
///
/// * `fb` - Framebuffer to draw on
/// * `x` - X coordinate (left edge)
/// * `y` - Y coordinate (top edge)
/// * `c` - Character to draw
/// * `fg_color` - Foreground color
/// * `bg_color` - Background color
///
/// # Errors
///
/// Returns error if coordinates are out of bounds or font data is invalid
pub fn draw_char(
    fb: &mut FramebufferInfo,
    x: usize,
    y: usize,
    c: char,
    fg_color: RgbColor,
    bg_color: RgbColor,
) -> Result<(), FramebufferError> {
    let bitmap = get_char_bitmap(c).ok_or(FramebufferError::InvalidFont)?;

    for (row, &byte) in bitmap.iter().enumerate() {
        for col in 0..FONT_WIDTH {
            let bit = (byte >> (7 - col)) & 1;
            let color = if bit == 1 { fg_color } else { bg_color };
            fb.write_pixel(x + col, y + row, color)?;
        }
    }

    Ok(())
}

/// Draw a string on the framebuffer
///
/// # Arguments
///
/// * `fb` - Framebuffer to draw on
/// * `x` - X coordinate (left edge)
/// * `y` - Y coordinate (top edge)
/// * `s` - String to draw
/// * `fg_color` - Foreground color
/// * `bg_color` - Background color
///
/// # Returns
///
/// Returns the x coordinate after the last character
///
/// # Errors
///
/// Returns error if coordinates are out of bounds
pub fn draw_string(
    fb: &mut FramebufferInfo,
    mut x: usize,
    y: usize,
    s: &str,
    fg_color: RgbColor,
    bg_color: RgbColor,
) -> Result<usize, FramebufferError> {
    for c in s.chars() {
        draw_char(fb, x, y, c, fg_color, bg_color)?;
        x += FONT_WIDTH;
    }
    Ok(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn test_get_char_bitmap() {
        // Test space character
        let space = get_char_bitmap(' ');
        assert!(space.is_some());
        assert_eq!(space.unwrap().len(), BYTES_PER_CHAR);

        // Test printable character
        let a = get_char_bitmap('A');
        assert!(a.is_some());
        assert_eq!(a.unwrap().len(), BYTES_PER_CHAR);

        // Test that unsupported chars fallback to space
        let unsupported = get_char_bitmap('😀');
        assert!(unsupported.is_some()); // Returns space
    }

    #[test_case]
    fn test_font_dimensions() {
        assert_eq!(FONT_WIDTH, 8);
        assert_eq!(FONT_HEIGHT, 16);
    }
}
