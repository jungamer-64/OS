use core::ptr::NonNull;
use crate::vga_buffer::{VgaBufferAccess, VgaError};
use crate::vga_buffer::constants::{VGA_BUFFER_ADDR, CELL_COUNT, VGA_HEIGHT, VGA_WIDTH};

/// Concrete backend that talks to the legacy PC/AT text-mode buffer at 0xB8000.
/// This is specific to x86/x86_64 PC-compatible systems.
#[derive(Clone, Copy)]
pub struct TextModeBuffer {
    ptr: NonNull<u16>,
}

impl TextModeBuffer {
    /// Construct a new text-mode backend.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            // SAFETY: 0xB8000 is the canonical VGA text buffer address for PC/AT systems.
            ptr: unsafe { NonNull::new_unchecked(VGA_BUFFER_ADDR as *mut u16) },
        }
    }

    #[inline]
    const fn is_valid_index(index: usize) -> bool {
        index < CELL_COUNT
    }
}

unsafe impl Send for TextModeBuffer {}
unsafe impl Sync for TextModeBuffer {}

impl Default for TextModeBuffer {
    fn default() -> Self {
        Self::new()
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
