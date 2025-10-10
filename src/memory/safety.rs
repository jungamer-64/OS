// src/memory/safety.rs

//! Memory safety utilities and validation
//!
//! Provides utilities for safe memory operations:
//! - Bounds checking
//! - Alignment validation
//! - Overflow detection
//! - Safe pointer arithmetic

use core::mem;
use core::ptr::NonNull;

/// Memory region descriptor with validation
#[derive(Debug, Clone, Copy)]
pub struct MemoryRegion {
    start: usize,
    size: usize,
}

impl MemoryRegion {
    /// Create a new memory region with validation
    ///
    /// Returns None if the region would overflow or is invalid
    pub const fn new(start: usize, size: usize) -> Option<Self> {
        if size == 0 {
            return None;
        }

        match start.checked_add(size) {
            Some(_) => Some(Self { start, size }),
            None => None,
        }
    }

    /// Create from start and end addresses
    pub const fn from_range(start: usize, end: usize) -> Option<Self> {
        if end <= start {
            return None;
        }

        let size = end - start;
        Self::new(start, size)
    }

    /// Get start address
    pub const fn start(&self) -> usize {
        self.start
    }

    /// Get size in bytes
    pub const fn size(&self) -> usize {
        self.size
    }

    /// Get end address (exclusive)
    pub const fn end(&self) -> usize {
        self.start + self.size
    }

    /// Check if address is within region
    pub const fn contains(&self, addr: usize) -> bool {
        addr >= self.start && addr < self.end()
    }

    /// Check if another region overlaps with this one
    pub const fn overlaps(&self, other: &Self) -> bool {
        self.start < other.end() && other.start < self.end()
    }

    /// Check if region is properly aligned
    pub const fn is_aligned(&self, alignment: usize) -> bool {
        self.start % alignment == 0 && self.size % alignment == 0
    }

    /// Get subregion with bounds checking
    pub const fn subregion(&self, offset: usize, size: usize) -> Option<Self> {
        if offset >= self.size {
            return None;
        }

        let remaining = self.size - offset;
        if size > remaining {
            return None;
        }

        Self::new(self.start + offset, size)
    }
}

/// Safe buffer accessor with compile-time size checking
#[derive(Debug)]
pub struct SafeBuffer<T> {
    ptr: NonNull<T>,
    len: usize,
    region: MemoryRegion,
}

impl<T> SafeBuffer<T> {
    /// Create a new safe buffer
    ///
    /// # Safety
    ///
    /// - ptr must point to valid memory for at least `len` elements
    /// - Memory must remain valid for the lifetime of this buffer
    /// - No other mutable references to this memory must exist
    pub unsafe fn new(ptr: NonNull<T>, len: usize) -> Option<Self> {
        let size = len.checked_mul(mem::size_of::<T>())?;
        let region = MemoryRegion::new(ptr.as_ptr() as usize, size)?;

        Some(Self { ptr, len, region })
    }

    /// Get buffer length
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Check if buffer is empty
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Get memory region
    pub const fn region(&self) -> &MemoryRegion {
        &self.region
    }

    /// Read value at index with bounds checking
    pub fn read(&self, index: usize) -> Result<T, BufferError>
    where
        T: Copy,
    {
        if index >= self.len {
            return Err(BufferError::OutOfBounds {
                index,
                len: self.len,
            });
        }

        unsafe {
            let ptr = self.ptr.as_ptr().add(index);
            Ok(core::ptr::read_volatile(ptr))
        }
    }

    /// Write value at index with bounds checking
    pub fn write(&mut self, index: usize, value: T) -> Result<(), BufferError> {
        if index >= self.len {
            return Err(BufferError::OutOfBounds {
                index,
                len: self.len,
            });
        }

        unsafe {
            let ptr = self.ptr.as_ptr().add(index);
            core::ptr::write_volatile(ptr, value);
        }

        Ok(())
    }

    /// Fill buffer with value
    pub fn fill(&mut self, value: T) -> Result<(), BufferError>
    where
        T: Copy,
    {
        for i in 0..self.len {
            self.write(i, value)?;
        }
        Ok(())
    }

    /// Copy data from slice with bounds checking
    pub fn copy_from_slice(&mut self, src: &[T]) -> Result<(), BufferError>
    where
        T: Copy,
    {
        if src.len() > self.len {
            return Err(BufferError::InsufficientSpace {
                required: src.len(),
                available: self.len,
            });
        }

        unsafe {
            core::ptr::copy_nonoverlapping(src.as_ptr(), self.ptr.as_ptr(), src.len());
        }

        Ok(())
    }

    /// Get subslice with bounds checking
    pub fn subslice(&self, start: usize, len: usize) -> Result<&[T], BufferError> {
        if start >= self.len {
            return Err(BufferError::OutOfBounds {
                index: start,
                len: self.len,
            });
        }

        let end = start.checked_add(len).ok_or(BufferError::Overflow)?;

        if end > self.len {
            return Err(BufferError::OutOfBounds {
                index: end,
                len: self.len,
            });
        }

        unsafe {
            let ptr = self.ptr.as_ptr().add(start);
            Ok(core::slice::from_raw_parts(ptr, len))
        }
    }
}

/// Buffer operation errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferError {
    /// Index out of bounds
    OutOfBounds { index: usize, len: usize },
    /// Insufficient space for operation
    InsufficientSpace { required: usize, available: usize },
    /// Arithmetic overflow
    Overflow,
    /// Alignment error
    Misaligned { addr: usize, required: usize },
}

impl core::fmt::Display for BufferError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            BufferError::OutOfBounds { index, len } => {
                write!(f, "index {} out of bounds (len: {})", index, len)
            }
            BufferError::InsufficientSpace {
                required,
                available,
            } => {
                write!(
                    f,
                    "insufficient space (required: {}, available: {})",
                    required, available
                )
            }
            BufferError::Overflow => write!(f, "arithmetic overflow"),
            BufferError::Misaligned { addr, required } => {
                write!(
                    f,
                    "misaligned address {:#x} (required alignment: {})",
                    addr, required
                )
            }
        }
    }
}

/// Safe pointer arithmetic utilities
pub mod ptr_math {
    use super::BufferError;

    /// Add offset to pointer with overflow checking
    pub fn checked_add<T>(ptr: *const T, offset: usize) -> Result<*const T, BufferError> {
        let ptr_val = ptr as usize;
        let offset_bytes = offset
            .checked_mul(core::mem::size_of::<T>())
            .ok_or(BufferError::Overflow)?;

        let result = ptr_val
            .checked_add(offset_bytes)
            .ok_or(BufferError::Overflow)?;

        Ok(result as *const T)
    }

    /// Calculate distance between pointers
    pub fn ptr_distance<T>(start: *const T, end: *const T) -> Result<usize, BufferError> {
        let start_val = start as usize;
        let end_val = end as usize;

        if end_val < start_val {
            return Err(BufferError::Overflow);
        }

        let byte_distance = end_val - start_val;
        let elem_size = core::mem::size_of::<T>();

        if elem_size == 0 {
            return Err(BufferError::Overflow);
        }

        if byte_distance % elem_size != 0 {
            return Err(BufferError::Misaligned {
                addr: end_val,
                required: elem_size,
            });
        }

        Ok(byte_distance / elem_size)
    }

    /// Check if pointer is aligned
    pub fn is_aligned<T>(ptr: *const T, alignment: usize) -> bool {
        (ptr as usize) % alignment == 0
    }
}

#[cfg(all(test, feature = "std-tests"))]
mod tests {
    use super::*;

    #[test]
    fn test_memory_region_creation() {
        let region = MemoryRegion::new(0x1000, 0x100).unwrap();
        assert_eq!(region.start(), 0x1000);
        assert_eq!(region.size(), 0x100);
        assert_eq!(region.end(), 0x1100);
    }

    #[test]
    fn test_memory_region_overflow() {
        // Should fail on overflow
        assert!(MemoryRegion::new(usize::MAX - 10, 20).is_none());
    }

    #[test]
    fn test_memory_region_contains() {
        let region = MemoryRegion::new(0x1000, 0x100).unwrap();
        assert!(region.contains(0x1000));
        assert!(region.contains(0x10FF));
        assert!(!region.contains(0x1100));
        assert!(!region.contains(0x0FFF));
    }

    #[test]
    fn test_memory_region_overlaps() {
        let region1 = MemoryRegion::new(0x1000, 0x100).unwrap();
        let region2 = MemoryRegion::new(0x1050, 0x100).unwrap();
        let region3 = MemoryRegion::new(0x2000, 0x100).unwrap();

        assert!(region1.overlaps(&region2));
        assert!(region2.overlaps(&region1));
        assert!(!region1.overlaps(&region3));
    }

    #[test]
    fn test_memory_region_alignment() {
        let aligned = MemoryRegion::new(0x1000, 0x100).unwrap();
        assert!(aligned.is_aligned(16));
        assert!(aligned.is_aligned(256));

        let misaligned = MemoryRegion::new(0x1001, 0x100).unwrap();
        assert!(!misaligned.is_aligned(16));
    }

    #[test]
    fn test_memory_region_subregion() {
        let region = MemoryRegion::new(0x1000, 0x100).unwrap();

        let sub = region.subregion(0x10, 0x20).unwrap();
        assert_eq!(sub.start(), 0x1010);
        assert_eq!(sub.size(), 0x20);

        // Should fail - exceeds parent
        assert!(region.subregion(0x10, 0x200).is_none());
    }

    #[test]
    fn test_ptr_math_checked_add() {
        let ptr = 0x1000 as *const u8;
        let result = ptr_math::checked_add(ptr, 10).unwrap();
        assert_eq!(result as usize, 0x100A);

        // Should overflow
        let large_ptr = (usize::MAX - 10) as *const u8;
        assert!(ptr_math::checked_add(large_ptr, 20).is_err());
    }

    #[test]
    fn test_ptr_math_distance() {
        let start = 0x1000 as *const u32;
        let end = 0x1010 as *const u32;

        let distance = ptr_math::ptr_distance(start, end).unwrap();
        assert_eq!(distance, 4); // 16 bytes / 4 bytes per u32
    }
}
