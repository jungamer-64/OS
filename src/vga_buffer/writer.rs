// src/vga_buffer/writer.rs

//! VGA writer implementation with bounds-checked buffer access and
//! robust error propagation.

use super::backend::{DefaultVgaBuffer, VgaBufferAccess};
use super::color::ColorCode;
use super::constants::*;
use crate::diagnostics::DIAGNOSTICS;
use core::fmt::{self, Write};
use core::sync::atomic::{AtomicBool, Ordering};

const DIRTY_WORD_BITS: usize = core::mem::size_of::<u64>() * 8;
const DIRTY_WORD_COUNT: usize = CELL_COUNT.div_ceil(DIRTY_WORD_BITS);

/// Buffer accessibility tracking
pub(super) static BUFFER_ACCESSIBLE: AtomicBool = AtomicBool::new(false);

/// Errors that can occur when interacting with the VGA subsystem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VgaError {
    /// The VGA buffer is not currently accessible.
    BufferNotAccessible,
    /// The cursor position is outside the visible screen area.
    InvalidPosition,
    /// The underlying memory write failed.
    WriteFailure,
    /// The writer has not been successfully initialized yet.
    NotInitialized,
    /// The writer was used without the runtime lock being held.
    NotLocked,
    /// Lock order violation detected.
    LockOrderViolation,
}

impl VgaError {
    /// Convert the error into a human-readable static message.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::BufferNotAccessible => "buffer not accessible",
            Self::InvalidPosition => "invalid position",
            Self::WriteFailure => "write failure",
            Self::NotInitialized => "writer not initialized",
            Self::NotLocked => "writer not locked",
            Self::LockOrderViolation => "lock order violation",
        }
    }
}


/// Position in the VGA buffer with validation
#[derive(Debug, Clone, Copy)]
struct Position {
    row: usize,
    col: usize,
}

impl Position {
    /// Create a new position at the top-left corner
    const fn new() -> Self {
        Self { row: 0, col: 0 }
    }

    /// Calculate linear cell index in the VGA buffer with bounds checking.
    /// Returns `None` if the position is out of bounds.
    const fn cell_index(&self) -> Option<usize> {
        if self.row >= VGA_HEIGHT || self.col >= VGA_WIDTH {
            return None;
        }
        Some(self.row * VGA_WIDTH + self.col)
    }

    /// Move to next column, returns false if at row end
    const fn advance_col(&mut self) -> bool {
        if self.col + 1 < VGA_WIDTH {
            self.col += 1;
            true
        } else {
            false
        }
    }

    /// Move to new line
    const fn new_line(&mut self) {
        self.col = 0;
        self.row += 1;
    }

    /// Check if at bottom of screen
    const fn is_at_screen_bottom(&self) -> bool {
        self.row >= VGA_HEIGHT
    }

    /// Validate position is within bounds
    const fn is_valid(&self) -> bool {
        self.row < VGA_HEIGHT && self.col < VGA_WIDTH
    }
}

/// Runtime guard that tracks exclusive access beyond Rust's borrow checker.
pub struct RuntimeLockGuard {
    writer: *mut VgaWriter,
}

impl RuntimeLockGuard {
    #[allow(clippy::ref_as_ptr)]
    fn new(writer: &mut VgaWriter) -> Self {
        writer.runtime_locked = true;
        Self {
            writer: writer as *mut VgaWriter,
        }
    }
}

impl Drop for RuntimeLockGuard {
    fn drop(&mut self) {
        unsafe {
            (*self.writer).runtime_locked = false;
        }
    }
}

/// VGA Writer structure with bounds-checked buffer access
pub struct VgaWriter {
    position: Position,
    color_code: ColorCode,
    buffer: DefaultVgaBuffer,
    initialized: bool,
    runtime_locked: bool,
}

// SAFETY: We ensure exclusive access via Mutex and interrupt disabling
unsafe impl Send for VgaWriter {}
unsafe impl Sync for VgaWriter {}

impl VgaWriter {
    /// Create a new VGA writer
    #[must_use]
    pub const fn new() -> Self {
        Self {
            position: Position::new(),
            color_code: ColorCode::normal(),
            initialized: false,
            runtime_locked: false,
            buffer: DefaultVgaBuffer::new(),
        }
    }

    /// Mark the writer as locked for the duration of the guard.
    pub(crate) fn runtime_guard(&mut self) -> RuntimeLockGuard {
        RuntimeLockGuard::new(self)
    }

    /// Ensure the writer has acquired the runtime lock.
    ///
    /// # Errors
    ///
    /// Returns `VgaError::NotLocked` if runtime lock not acquired.
    fn ensure_runtime_lock(&self) -> Result<(), VgaError> {
        if !self.runtime_locked {
            DIAGNOSTICS.record_vga_write(false);
            return Err(VgaError::NotLocked);
        }
        Ok(())
    }

    /// Ensure the writer is ready for operations.
    ///
    /// # Errors
    ///
    /// Returns `VgaError::NotLocked` if runtime lock not acquired.
    /// Returns `VgaError::NotInitialized` if writer not initialized.
    /// Returns `VgaError::BufferNotAccessible` if VGA buffer is inaccessible.
    fn ensure_ready(&self) -> Result<(), VgaError> {
        self.ensure_runtime_lock()?;
        if !self.initialized {
            DIAGNOSTICS.record_vga_write(false);
            return Err(VgaError::NotInitialized);
        }
        if !Self::is_accessible() {
            DIAGNOSTICS.record_vga_write(false);
            return Err(VgaError::BufferNotAccessible);
        }
        Ok(())
    }

    /// Test if VGA buffer is accessible.
    fn test_accessibility(&mut self) -> Result<bool, VgaError> {
        const MAX_ATTEMPTS: usize = 3;
        let original = self.buffer.read_cell(0)?;
        let test_pattern: u16 = 0x0F20; // White space

        for _ in 0..MAX_ATTEMPTS {
            self.buffer.write_cell(0, test_pattern)?;
            let readback = self.buffer.read_cell(0)?;
            self.buffer.write_cell(0, original)?;

            if readback == test_pattern {
                return Ok(true);
            }
        }

        // Ensure original value restored even if checks failed.
        let _ = self.buffer.write_cell(0, original);
        Ok(false)
    }

    /// Verify buffer is accessible (cached result)
    fn is_accessible() -> bool {
        BUFFER_ACCESSIBLE.load(Ordering::Acquire)
    }

    /// Initialize and test buffer accessibility.
    pub fn init_accessibility(&mut self) -> Result<(), VgaError> {
        let accessible = self.test_accessibility()?;
        BUFFER_ACCESSIBLE.store(accessible, Ordering::Release);
        self.initialized = accessible;

        if accessible {
            Ok(())
        } else {
            Err(VgaError::BufferNotAccessible)
        }
    }

    /// Set text color
    pub fn set_color(&mut self, color: ColorCode) -> Result<(), VgaError> {
        self.ensure_runtime_lock()?;
        if self.color_code != color {
            DIAGNOSTICS.record_vga_color_change();
        }
        self.color_code = color;
        Ok(())
    }

    /// Clear the entire screen with bounds checking
    pub fn clear(&mut self) -> Result<(), VgaError> {
        self.ensure_ready()?;

        for row in 0..VGA_HEIGHT {
            self.clear_row(row)?;
        }

        self.position = Position::new();
        self.color_code = ColorCode::normal();
        Ok(())
    }

    /// Clear a specific row with blank characters
    fn clear_row(&mut self, row: usize) -> Result<(), VgaError> {
        self.ensure_ready()?;
        let blank = Self::encode_char(b' ', self.color_code);
        let result = self.buffer.fill_row(row, blank);
        if result.is_err() {
            DIAGNOSTICS.record_vga_write(false);
        }
        result
    }

    /// Encode a character with color into a 16-bit value
    const fn encode_char(ch: u8, color: ColorCode) -> u16 {
        (color.as_u8() as u16) << 8 | ch as u16
    }

    /// Scroll the screen up by one line with validated memory operations.
    fn scroll(&mut self) -> Result<(), VgaError> {
        self.ensure_ready()?;
        DIAGNOSTICS.record_vga_scroll();

        let src_index = VGA_WIDTH;
        let dst_index = 0;
        let copy_cells = VGA_WIDTH
            .checked_mul(VGA_HEIGHT - 1)
            .ok_or(VgaError::InvalidPosition)?;

        if let Err(err) = self.buffer.copy_cells(src_index, dst_index, copy_cells) {
            DIAGNOSTICS.record_vga_write(false);
            return Err(err);
        }

        if let Err(err) = self
            .buffer
            .fill_row(VGA_HEIGHT - 1, Self::encode_char(b' ', self.color_code))
        {
            DIAGNOSTICS.record_vga_write(false);
            return Err(err);
        }

        self.position.row = VGA_HEIGHT - 1;
        self.position.col = 0;
        Ok(())
    }

    /// Move to a new line, scrolling if necessary
    fn new_line(&mut self) -> Result<(), VgaError> {
        self.position.new_line();
        if self.position.is_at_screen_bottom() {
            self.scroll()?;
        }
        Ok(())
    }

    /// Write a single byte to the screen, ignoring any failure.
    #[allow(dead_code)]
    fn write_byte(&mut self, byte: u8) {
        let _ = self.write_byte_checked(byte);
    }

    /// Write a single byte with detailed error reporting.
    #[allow(dead_code)]
    pub fn write_byte_checked(&mut self, byte: u8) -> Result<(), VgaError> {
        self.write_byte_internal(byte)
    }

    #[inline]
    #[allow(clippy::branches_sharing_code)]
    fn write_byte_internal(&mut self, byte: u8) -> Result<(), VgaError> {
        self.ensure_ready()?;

        if byte == b'\n' {
            if let Err(err) = self.new_line() {
                DIAGNOSTICS.record_vga_write(false);
                return Err(err);
            }
            DIAGNOSTICS.record_vga_write(true);
            Ok(())
        } else {
            if !self.position.is_valid() {
                DIAGNOSTICS.record_vga_write(false);
                return Err(VgaError::InvalidPosition);
            }

            let index = self
                .position
                .cell_index()
                .ok_or(VgaError::InvalidPosition)?;
            let encoded = Self::encode_char(byte, self.color_code);

            self.buffer.write_cell(index, encoded)?;

            if !self.position.advance_col() {
                if let Err(err) = self.new_line() {
                    DIAGNOSTICS.record_vga_write(false);
                    return Err(err);
                }
            }

            DIAGNOSTICS.record_vga_write(true);
            Ok(())
        }
    }

    #[inline]
    fn write_ascii(&mut self, s: &str) -> Result<(), VgaError> {
        for byte in s.bytes() {
            let display_byte = if Self::is_printable(byte) {
                byte
            } else {
                REPLACEMENT_CHAR
            };

            self.write_byte_checked(display_byte)?;
        }
        Ok(())
    }

    /// Write a string with temporary color
    pub fn write_colored(&mut self, s: &str, color: ColorCode) -> Result<(), VgaError> {
        let previous = self.color_code;
        self.set_color(color)?;
        let result = self.write_ascii(s);
        self.set_color(previous)?;
        result
    }

    /// Check if a byte is printable ASCII or newline
    fn is_printable(byte: u8) -> bool {
        (PRINTABLE_ASCII_START..=PRINTABLE_ASCII_END).contains(&byte) || byte == b'\n'
    }
}

impl Write for VgaWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_ascii(s).map_err(|_| fmt::Error)
    }
}

/// Double-buffered VGA writer to minimize display tearing.
#[allow(dead_code)]
pub struct DoubleBufferedWriter {
    front: DefaultVgaBuffer,
    back: [u16; CELL_COUNT],
    dirty: [u64; DIRTY_WORD_COUNT],
}

impl DoubleBufferedWriter {
    /// Create a new empty double buffer
    #[must_use]
    pub const fn new() -> Self {
        Self {
            front: DefaultVgaBuffer::new(),
            back: [0; CELL_COUNT],
            dirty: [0; DIRTY_WORD_COUNT],
        }
    }

    /// Stage a cell write in the back buffer.
    /// Write a cell to the back buffer
    ///
    /// # Errors
    ///
    /// Returns `VgaError::InvalidPosition` if index is out of bounds
    #[allow(dead_code)]
    pub fn write_cell(&mut self, index: usize, value: u16) -> Result<(), VgaError> {
        if index >= CELL_COUNT {
            return Err(VgaError::InvalidPosition);
        }

        self.back[index] = value;
        Self::mark_dirty(&mut self.dirty, index);
        Ok(())
    }

    /// Present all pending changes to the front buffer and clear dirty state.
    #[allow(dead_code)]
    /// Swap dirty buffers to screen
    ///
    /// # Errors
    ///
    /// Returns `VgaError` if buffer operations fail
    pub fn swap_buffers(&mut self) -> Result<usize, VgaError> {
        if !BUFFER_ACCESSIBLE.load(Ordering::Acquire) {
            return Err(VgaError::BufferNotAccessible);
        }

        let mut updated = 0usize;

        for (chunk_idx, chunk_bits) in self.dirty.iter_mut().enumerate() {
            let mut bits = *chunk_bits;
            while bits != 0 {
                let bit = bits.trailing_zeros() as usize;
                let cell_index = chunk_idx * DIRTY_WORD_BITS + bit;
                if cell_index >= CELL_COUNT {
                    break;
                }

                self.front
                    .write_cell(cell_index, self.back[cell_index])?;
                updated += 1;
                bits &= bits - 1;
            }

            *chunk_bits = 0;
        }

        Ok(updated)
    }

    /// Mark every cell as dirty so that the next swap refreshes the full frame.
    #[allow(dead_code)]
    pub fn mark_all_dirty(&mut self) {
        let remainder_bits = CELL_COUNT % DIRTY_WORD_BITS;
        for (chunk_idx, chunk_bits) in self.dirty.iter_mut().enumerate() {
            *chunk_bits = if chunk_idx == DIRTY_WORD_COUNT - 1 && remainder_bits != 0 {
                (1u64 << remainder_bits) - 1
            } else {
                u64::MAX
            };
        }
    }

    /// Replace the back buffer with an entire pre-rendered frame.
    #[allow(dead_code)]
    pub fn stage_frame(&mut self, frame: &[u16; CELL_COUNT]) {
        self.back.copy_from_slice(frame);
        self.mark_all_dirty();
    }

    const fn mark_dirty(dirty: &mut [u64; DIRTY_WORD_COUNT], index: usize) {
        let chunk = index / DIRTY_WORD_BITS;
        let bit = index % DIRTY_WORD_BITS;
        dirty[chunk] |= 1u64 << bit;
    }
}

impl Default for DoubleBufferedWriter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(all(test, feature = "std-tests"))]
mod tests {
    use super::*;

    #[test]
    fn test_position_validation() {
        let mut pos = Position::new();
        assert!(pos.is_valid());

        pos.row = VGA_HEIGHT;
        assert!(!pos.is_valid());
    }

    #[test]
    fn test_position_byte_offset() {
        let pos = Position { row: 1, col: 2 };
        assert_eq!(pos.cell_index(), Some(1 * 80 + 2));

        let invalid_pos = Position {
            row: VGA_HEIGHT,
            col: 0,
        };
        assert_eq!(invalid_pos.cell_index(), None);
    }

    #[test]
    fn test_advance_col_behavior() {
        let mut end_of_row = Position {
            row: 0,
            col: VGA_WIDTH - 1,
        };
        assert!(!end_of_row.advance_col());
        assert_eq!(end_of_row.col, VGA_WIDTH - 1);

        let mut mid_row = Position { row: 0, col: 0 };
        assert!(mid_row.advance_col());
        assert_eq!(mid_row.col, 1);
    }

    #[test]
    fn test_char_encoding() {
        use super::super::color::VgaColor;
        let encoded = VgaWriter::encode_char(b'A', ColorCode::normal());
        assert_eq!(encoded & 0xFF, b'A' as u16);
        assert_eq!(encoded >> 8, ColorCode::normal().as_u8() as u16);
    }

    #[test]
    fn test_printable_detection() {
        assert!(VgaWriter::is_printable(b' '));
        assert!(VgaWriter::is_printable(b'A'));
        assert!(VgaWriter::is_printable(b'\n'));
        assert!(!VgaWriter::is_printable(0x00));
        assert!(!VgaWriter::is_printable(0x7F));
    }
}
