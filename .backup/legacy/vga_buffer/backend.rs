// src/vga_buffer/backend.rs

//! Low-level VGA buffer access abstractions.
//!
//! This module introduces the [`VgaBufferAccess`] trait so that higher-level
//! code (such as the VGA writer or future renderers) can target any backing
//! implementation—from the classic text-mode buffer at `0xB8000` to an
//! in-memory stub for testing.

use super::constants::{CELL_COUNT, VGA_HEIGHT, VGA_WIDTH};
use super::VgaError;

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

#[cfg(target_arch = "x86_64")]
pub type DefaultVgaBuffer = crate::arch::VgaBackend;

#[cfg(not(target_arch = "x86_64"))]
pub type DefaultVgaBuffer = StubBuffer;

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
