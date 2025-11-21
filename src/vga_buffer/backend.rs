// src/vga_buffer/backend.rs

//! Low-level VGA buffer access abstractions.
//!
//! This module introduces the [`VgaBufferAccess`] trait so that higher-level
//! code (such as the VGA writer or future renderers) can target any backing
//! implementation—from the classic text-mode buffer at `0xB8000` to an
//! in-memory stub for testing.

use super::constants::{CELL_COUNT, VGA_BUFFER_ADDR, VGA_HEIGHT, VGA_WIDTH};
use super::VgaError;
use core::ptr::NonNull;

/// Abstraction over the VGA character buffer memory.
pub trait VgaBufferAccess {
    /// Total number of addressable character cells.
    fn capacity(&self) -> usize {
        CELL_COUNT
    }

    /// Read the encoded value at `index`.
    ///
    /// # Errors
    ///
    /// Returns [`VgaError::InvalidPosition`] when `index` is outside the
    /// buffer.
    fn read_cell(&self, index: usize) -> Result<u16, VgaError>;

    /// Write `value` to the cell at `index`.
    ///
    /// # Errors
    ///
    /// Returns [`VgaError::InvalidPosition`] if the index is invalid or any
    /// hardware write fails.
    fn write_cell(&mut self, index: usize, value: u16) -> Result<(), VgaError>;

    /// Copy `count` cells starting at `src` into the region beginning at `dst`.
    ///
    /// # Errors
    ///
    /// Returns [`VgaError::InvalidPosition`] if either range lies outside the
    /// buffer.
    fn copy_cells(&mut self, src: usize, dst: usize, count: usize) -> Result<(), VgaError>;

    /// Fill an entire row with `value`.
    ///
    /// # Errors
    ///
    /// Returns [`VgaError::InvalidPosition`] if the row exceeds the display.
    fn fill_row(&mut self, row: usize, value: u16) -> Result<(), VgaError>;
}

/// Concrete backend that talks to the legacy text-mode buffer at 0xB8000.
#[derive(Clone, Copy)]
pub struct TextModeBuffer {
    ptr: NonNull<u16>,
}

impl TextModeBuffer {
    /// Construct a new text-mode backend.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            // SAFETY: 0xB8000 is the canonical VGA text buffer address.
            ptr: unsafe { NonNull::new_unchecked(VGA_BUFFER_ADDR as *mut u16) },
        }
    }

    #[inline]
    const fn is_valid_index(index: usize) -> bool {
        index < CELL_COUNT
    }
}

impl VgaBufferAccess for TextModeBuffer {
    fn read_cell(&self, index: usize) -> Result<u16, VgaError> {
        if !Self::is_valid_index(index) {
            return Err(VgaError::InvalidPosition);
        }

        Ok(unsafe { core::ptr::read_volatile(self.ptr.as_ptr().add(index)) })
    }

    fn write_cell(&mut self, index: usize, value: u16) -> Result<(), VgaError> {
        if !Self::is_valid_index(index) {
            return Err(VgaError::InvalidPosition);
        }

        unsafe {
            core::ptr::write_volatile(self.ptr.as_ptr().add(index), value);
            core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
        }
        Ok(())
    }

    fn copy_cells(&mut self, src: usize, dst: usize, count: usize) -> Result<(), VgaError> {
        if count == 0 {
            return Ok(());
        }

        let src_end = src.checked_add(count).ok_or(VgaError::InvalidPosition)?;
        let dst_end = dst.checked_add(count).ok_or(VgaError::InvalidPosition)?;

        if !Self::is_valid_index(src) || !Self::is_valid_index(dst) {
            return Err(VgaError::InvalidPosition);
        }
        if src_end > CELL_COUNT || dst_end > CELL_COUNT {
            return Err(VgaError::InvalidPosition);
        }

        unsafe {
            core::ptr::copy(
                self.ptr.as_ptr().add(src),
                self.ptr.as_ptr().add(dst),
                count,
            );
        }
        Ok(())
    }

    fn fill_row(&mut self, row: usize, value: u16) -> Result<(), VgaError> {
        if row >= VGA_HEIGHT {
            return Err(VgaError::InvalidPosition);
        }

        let start = row
            .checked_mul(VGA_WIDTH)
            .ok_or(VgaError::InvalidPosition)?;

        for offset in 0..VGA_WIDTH {
            self.write_cell(start + offset, value)?;
        }
        Ok(())
    }
}

impl Default for TextModeBuffer {
    fn default() -> Self {
        Self::new()
    }
}

/// Simple stub implementation backed by regular memory for testing.
#[cfg_attr(target_arch = "x86_64", allow(dead_code))]
#[derive(Clone)]
pub struct StubBuffer {
    cells: [u16; CELL_COUNT],
    accessible: bool,
}

#[cfg_attr(target_arch = "x86_64", allow(dead_code))]
impl StubBuffer {
    /// Create a stub that reports itself as inaccessible.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            cells: [0; CELL_COUNT],
            accessible: false,
        }
    }

    /// Create a stub with a fixed accessibility flag.
    #[must_use]
    pub const fn with_accessible(accessible: bool) -> Self {
        Self {
            cells: [0; CELL_COUNT],
            accessible,
        }
    }
}

impl VgaBufferAccess for StubBuffer {
    fn read_cell(&self, index: usize) -> Result<u16, VgaError> {
        if !self.accessible {
            return Err(VgaError::BufferNotAccessible);
        }
        self.cells
            .get(index)
            .copied()
            .ok_or(VgaError::InvalidPosition)
    }

    fn write_cell(&mut self, index: usize, value: u16) -> Result<(), VgaError> {
        if !self.accessible {
            return Err(VgaError::BufferNotAccessible);
        }
        self
            .cells
            .get_mut(index)
            .map(|cell| {
                *cell = value;
            })
            .ok_or(VgaError::InvalidPosition)
    }

    fn copy_cells(&mut self, src: usize, dst: usize, count: usize) -> Result<(), VgaError> {
        if !self.accessible {
            return Err(VgaError::BufferNotAccessible);
        }
        if src.checked_add(count).is_none_or(|end| end > CELL_COUNT)
            || dst.checked_add(count).is_none_or(|end| end > CELL_COUNT)
        {
            return Err(VgaError::InvalidPosition);
        }

        // Manual copy to avoid pulling in heap allocation in no_std builds.
        if count == 0 {
            return Ok(());
        }
        let mut idx = 0;
        while idx < count {
            let value = self.cells[src + idx];
            self.cells[dst + idx] = value;
            idx += 1;
        }
        Ok(())
    }

    fn fill_row(&mut self, row: usize, value: u16) -> Result<(), VgaError> {
        if !self.accessible {
            return Err(VgaError::BufferNotAccessible);
        }
        if row >= VGA_HEIGHT {
            return Err(VgaError::InvalidPosition);
        }
        let start = row * VGA_WIDTH;
        for offset in 0..VGA_WIDTH {
            self.cells[start + offset] = value;
        }
        Ok(())
    }
}

#[cfg(target_arch = "x86_64")]
pub type DefaultVgaBuffer = TextModeBuffer;

#[cfg(not(target_arch = "x86_64"))]
pub type DefaultVgaBuffer = StubBuffer;
