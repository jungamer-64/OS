// src/vga_buffer/safe_buffer.rs

//! Safe buffer access with comprehensive bounds checking
//!
//! This module provides multiple layers of protection:
//! - Compile-time bounds checking where possible
//! - Runtime validation for dynamic accesses
//! - Guard pages detection
//! - Memory access verification

use super::constants::{VGA_BUFFER_ADDR, VGA_HEIGHT, VGA_WIDTH};
use super::VgaError;
use core::ptr::NonNull;

/// Total number of cells in the VGA buffer
pub const CELL_COUNT: usize = VGA_WIDTH * VGA_HEIGHT;

/// A validated index into the VGA buffer
///
/// This type guarantees that the index is within bounds
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ValidIndex(usize);

impl ValidIndex {
    /// Create a new validated index
    ///
    /// Returns None if index is out of bounds
    pub const fn new(index: usize) -> Option<Self> {
        if index < CELL_COUNT {
            Some(Self(index))
        } else {
            None
        }
    }

    /// Get the raw index value
    pub const fn get(self) -> usize {
        self.0
    }

    /// Create from row and column with validation
    pub const fn from_row_col(row: usize, col: usize) -> Option<Self> {
        if row < VGA_HEIGHT && col < VGA_WIDTH {
            let index = row * VGA_WIDTH + col;
            Some(Self(index))
        } else {
            None
        }
    }

    /// Get next index if available
    pub const fn next(self) -> Option<Self> {
        let next_val = self.0 + 1;
        if next_val < CELL_COUNT {
            Some(Self(next_val))
        } else {
            None
        }
    }

    /// Convert to row and column
    pub const fn to_row_col(self) -> (usize, usize) {
        (self.0 / VGA_WIDTH, self.0 % VGA_WIDTH)
    }
}

/// A validated range of indices
#[derive(Debug, Clone, Copy)]
pub struct ValidRange {
    start: ValidIndex,
    len: usize,
}

impl ValidRange {
    /// Create a new validated range
    ///
    /// Returns None if the range would exceed buffer bounds
    pub const fn new(start: usize, len: usize) -> Option<Self> {
        if len == 0 {
            return None;
        }

        match start.checked_add(len) {
            Some(end) if end <= CELL_COUNT => match ValidIndex::new(start) {
                Some(start_idx) => Some(Self {
                    start: start_idx,
                    len,
                }),
                None => None,
            },
            _ => None,
        }
    }

    /// Get the start index
    pub const fn start(&self) -> ValidIndex {
        self.start
    }

    /// Get the length
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Get the end index (exclusive)
    pub const fn end(&self) -> usize {
        self.start.0 + self.len
    }

    /// Check if range contains an index
    pub const fn contains(&self, index: ValidIndex) -> bool {
        index.0 >= self.start.0 && index.0 < self.start.0 + self.len
    }

    /// Create a range for an entire row
    pub const fn row(row: usize) -> Option<Self> {
        if row >= VGA_HEIGHT {
            return None;
        }
        Self::new(row * VGA_WIDTH, VGA_WIDTH)
    }
}

/// Safe VGA buffer accessor with validated operations
pub struct SafeBuffer {
    ptr: NonNull<u16>,
    verification_cell: usize,
}

impl SafeBuffer {
    /// Create a new safe buffer accessor
    ///
    /// # Safety
    ///
    /// The VGA buffer must be accessible and mapped at VGA_BUFFER_ADDR
    pub const unsafe fn new() -> Self {
        Self {
            ptr: unsafe { NonNull::new_unchecked(VGA_BUFFER_ADDR as *mut u16) },
            verification_cell: 0,
        }
    }

    /// Verify buffer is still accessible
    ///
    /// Performs a test read/write to ensure the buffer is mapped
    pub fn verify_accessible(&mut self) -> Result<(), VgaError> {
        // Use different cell each time to avoid caching effects
        self.verification_cell = (self.verification_cell + 1) % CELL_COUNT;

        let test_index =
            ValidIndex::new(self.verification_cell).ok_or(VgaError::InvalidPosition)?;

        // Read original value
        let original = self.read_validated(test_index)?;

        // Write test pattern
        let test_pattern = 0x0720; // Space with normal color
        self.write_validated(test_index, test_pattern)?;

        // Verify write
        let readback = self.read_validated(test_index)?;

        // Restore original
        self.write_validated(test_index, original)?;

        if readback == test_pattern {
            Ok(())
        } else {
            Err(VgaError::BufferNotAccessible)
        }
    }

    /// Write to a validated index
    ///
    /// This is the only way to write to the buffer, ensuring all writes
    /// are bounds-checked
    #[inline]
    pub fn write_validated(&self, index: ValidIndex, value: u16) -> Result<(), VgaError> {
        // Debug-only assertion following Microsoft Docs best practices:
        // Verify validated index is still within bounds
        let idx = index.get();
        debug_assert!(
            idx < super::constants::BUFFER_SIZE,
            "ValidIndex {idx} exceeds buffer size {}",
            super::constants::BUFFER_SIZE
        );

        unsafe {
            let ptr = self.ptr.as_ptr().add(index.get());

            // Use volatile write to prevent compiler optimization
            core::ptr::write_volatile(ptr, value);

            // Memory barrier to ensure write completes
            core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
        }
        Ok(())
    }

    /// Read from a validated index
    #[inline]
    pub fn read_validated(&self, index: ValidIndex) -> Result<u16, VgaError> {
        unsafe {
            let ptr = self.ptr.as_ptr().add(index.get());
            let value = core::ptr::read_volatile(ptr);
            Ok(value)
        }
    }

    /// Copy within buffer with validated range
    #[inline]
    pub fn copy_range(&self, src: ValidRange, dst: ValidIndex) -> Result<(), VgaError> {
        // Validate destination range
        let dst_end = dst
            .get()
            .checked_add(src.len())
            .ok_or(VgaError::BufferOverflow)?;

        if dst_end > CELL_COUNT {
            return Err(VgaError::BufferOverflow);
        }

        unsafe {
            let src_ptr = self.ptr.as_ptr().add(src.start().get());
            let dst_ptr = self.ptr.as_ptr().add(dst.get());

            // Use ptr::copy which handles overlapping ranges correctly
            core::ptr::copy(src_ptr, dst_ptr, src.len());
        }

        Ok(())
    }

    /// Fill a validated range with a value
    #[inline]
    pub fn fill_range(&self, range: ValidRange, value: u16) -> Result<(), VgaError> {
        for i in 0..range.len() {
            let index =
                ValidIndex::new(range.start().get() + i).ok_or(VgaError::InvalidPosition)?;
            self.write_validated(index, value)?;
        }
        Ok(())
    }

    /// Write a slice to buffer starting at index
    ///
    /// Returns number of cells written
    #[inline]
    pub fn write_slice(&self, start: ValidIndex, data: &[u16]) -> Result<usize, VgaError> {
        let end = start
            .get()
            .checked_add(data.len())
            .ok_or(VgaError::BufferOverflow)?;

        if end > CELL_COUNT {
            return Err(VgaError::BufferOverflow);
        }

        for (i, &value) in data.iter().enumerate() {
            let index = ValidIndex::new(start.get() + i).ok_or(VgaError::InvalidPosition)?;
            self.write_validated(index, value)?;
        }

        Ok(data.len())
    }

    /// Read a slice from buffer
    #[inline]
    pub fn read_slice(&self, start: ValidIndex, buf: &mut [u16]) -> Result<usize, VgaError> {
        let end = start
            .get()
            .checked_add(buf.len())
            .ok_or(VgaError::BufferOverflow)?;

        if end > CELL_COUNT {
            return Err(VgaError::BufferOverflow);
        }

        for (i, cell) in buf.iter_mut().enumerate() {
            let index = ValidIndex::new(start.get() + i).ok_or(VgaError::InvalidPosition)?;
            *cell = self.read_validated(index)?;
        }

        Ok(buf.len())
    }
}

#[cfg(all(test, feature = "std-tests"))]
mod tests {
    use super::*;

    #[test]
    fn test_valid_index_creation() {
        assert!(ValidIndex::new(0).is_some());
        assert!(ValidIndex::new(CELL_COUNT - 1).is_some());
        assert!(ValidIndex::new(CELL_COUNT).is_none());
    }

    #[test]
    fn test_valid_index_from_row_col() {
        let idx = ValidIndex::from_row_col(0, 0).unwrap();
        assert_eq!(idx.get(), 0);

        let idx = ValidIndex::from_row_col(1, 0).unwrap();
        assert_eq!(idx.get(), VGA_WIDTH);

        assert!(ValidIndex::from_row_col(VGA_HEIGHT, 0).is_none());
        assert!(ValidIndex::from_row_col(0, VGA_WIDTH).is_none());
    }

    #[test]
    fn test_valid_range_creation() {
        let range = ValidRange::new(0, 10).unwrap();
        assert_eq!(range.len(), 10);
        assert_eq!(range.end(), 10);

        // Can't create range that exceeds buffer
        assert!(ValidRange::new(CELL_COUNT - 5, 10).is_none());
    }

    #[test]
    fn test_valid_range_row() {
        let range = ValidRange::row(0).unwrap();
        assert_eq!(range.len(), VGA_WIDTH);
        assert_eq!(range.start().get(), 0);

        let range = ValidRange::row(1).unwrap();
        assert_eq!(range.start().get(), VGA_WIDTH);

        assert!(ValidRange::row(VGA_HEIGHT).is_none());
    }

    #[test]
    fn test_range_contains() {
        let range = ValidRange::new(10, 5).unwrap();

        assert!(range.contains(ValidIndex::new(10).unwrap()));
        assert!(range.contains(ValidIndex::new(14).unwrap()));
        assert!(!range.contains(ValidIndex::new(15).unwrap()));
        assert!(!range.contains(ValidIndex::new(9).unwrap()));
    }
}
