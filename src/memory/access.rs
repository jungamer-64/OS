// src/memory/access.rs

//! Generic memory access traits and helpers.
//!
//! These abstractions allow higher-level components to operate on memory
//! without being tied to a particular backing store. Drivers can accept any
//! implementer of [`MemoryAccess`] which makes it trivial to swap between
//! volatile buffers, device memory, or plain slices during testing.

use super::safety::{BufferError, SafeBuffer};

/// Trait describing read/write capabilities over a contiguous memory region.
pub trait MemoryAccess<T: Copy> {
    /// Total number of elements that can be addressed.
    fn capacity(&self) -> usize;

    /// Read the element at `index`.
    ///
    /// # Errors
    ///
    /// Returns [`BufferError::OutOfBounds`] if `index` is outside the valid
    /// range for this accessor.
    fn read(&self, index: usize) -> Result<T, BufferError>;

    /// Write `value` to the element at `index`.
    ///
    /// # Errors
    ///
    /// Returns [`BufferError::OutOfBounds`] if `index` exceeds the available
    /// capacity.
    fn write(&mut self, index: usize, value: T) -> Result<(), BufferError>;
}

/// Extension helpers implemented for every [`MemoryAccess`] provider.
pub trait MemoryAccessExt<T: Copy>: MemoryAccess<T> {
    /// Fill the entire region with `value`.
    ///
    /// # Errors
    ///
    /// Propagates any error reported by [`MemoryAccess::write`].
    fn fill_all(&mut self, value: T) -> Result<(), BufferError> {
        for idx in 0..self.capacity() {
            self.write(idx, value)?;
        }
        Ok(())
    }

    /// Write the contents of `data` starting at `start`.
    ///
    /// # Errors
    ///
    /// Returns [`BufferError::Overflow`] if the range would overflow `usize`
    /// calculations or [`BufferError::OutOfBounds`] when the write would extend
    /// beyond the accessor's capacity.
    fn write_slice(&mut self, start: usize, data: &[T]) -> Result<(), BufferError> {
        validate_range(start, data.len(), self.capacity())?;
        for (offset, value) in data.iter().copied().enumerate() {
            self.write(start + offset, value)?;
        }
        Ok(())
    }

    /// Read a range of elements into `out` starting at `start`.
    ///
    /// # Errors
    ///
    /// Returns [`BufferError::Overflow`] or [`BufferError::OutOfBounds`] when
    /// the requested slice doesn't fit inside the accessor.
    fn read_slice(&self, start: usize, out: &mut [T]) -> Result<(), BufferError> {
        validate_range(start, out.len(), self.capacity())?;
        for (offset, slot) in out.iter_mut().enumerate() {
            *slot = self.read(start + offset)?;
        }
        Ok(())
    }

    /// Copy `len` elements from `src` into `self`, starting at index 0.
    ///
    /// # Errors
    ///
    /// Returns [`BufferError::InsufficientSpace`] if either accessor cannot fit
    /// `len` elements, or propagates read/write errors from the underlying
    /// accessors.
    fn copy_from_access<A>(&mut self, src: &A, len: usize) -> Result<(), BufferError>
    where
        A: MemoryAccess<T>,
    {
        if len > src.capacity() {
            return Err(BufferError::InsufficientSpace {
                required: len,
                available: src.capacity(),
            });
        }

        if len > self.capacity() {
            return Err(BufferError::InsufficientSpace {
                required: len,
                available: self.capacity(),
            });
        }

        for idx in 0..len {
            let value = src.read(idx)?;
            self.write(idx, value)?;
        }
        Ok(())
    }

    /// Copy `len` elements from `self` into `dest`, starting at index 0.
    ///
    /// # Errors
    ///
    /// Returns [`BufferError::InsufficientSpace`] if `len` is larger than the
    /// capacity of either accessor, or any error surfaced by the read/write
    /// operations.
    fn copy_into_access<A>(&self, dest: &mut A, len: usize) -> Result<(), BufferError>
    where
        A: MemoryAccess<T>,
    {
        if len > self.capacity() {
            return Err(BufferError::InsufficientSpace {
                required: len,
                available: self.capacity(),
            });
        }

        if len > dest.capacity() {
            return Err(BufferError::InsufficientSpace {
                required: len,
                available: dest.capacity(),
            });
        }

        for idx in 0..len {
            let value = self.read(idx)?;
            dest.write(idx, value)?;
        }

        Ok(())
    }
}

impl<T: Copy, A: MemoryAccess<T> + ?Sized> MemoryAccessExt<T> for A {}

/// Memory access implementation backed by a mutable slice.
pub struct SliceMemoryAccess<'a, T: Copy> {
    data: &'a mut [T],
}

impl<'a, T: Copy> SliceMemoryAccess<'a, T> {
    /// Create a new accessor from a slice.
    #[inline]
    pub const fn new(data: &'a mut [T]) -> Self {
        Self { data }
    }

    /// Borrow the underlying slice immutably.
    #[inline]
    #[must_use]
    pub const fn as_slice(&self) -> &[T] {
        self.data
    }

    /// Borrow the underlying slice mutably.
    #[inline]
    #[must_use]
    pub const fn as_mut_slice(&mut self) -> &mut [T] {
        self.data
    }
}

impl<T: Copy> MemoryAccess<T> for SliceMemoryAccess<'_, T> {
    fn capacity(&self) -> usize {
        self.data.len()
    }

    fn read(&self, index: usize) -> Result<T, BufferError> {
        self.data.get(index).copied().ok_or(BufferError::OutOfBounds {
            index,
            len: self.data.len(),
        })
    }

    fn write(&mut self, index: usize, value: T) -> Result<(), BufferError> {
        if let Some(slot) = self.data.get_mut(index) {
            *slot = value;
            Ok(())
        } else {
            Err(BufferError::OutOfBounds {
                index,
                len: self.data.len(),
            })
        }
    }
}

#[allow(clippy::use_self)]
impl<T: Copy> MemoryAccess<T> for SafeBuffer<T> {
    fn capacity(&self) -> usize {
        self.len()
    }

    fn read(&self, index: usize) -> Result<T, BufferError> {
        SafeBuffer::read(self, index)
    }

    fn write(&mut self, index: usize, value: T) -> Result<(), BufferError> {
        SafeBuffer::write(self, index, value)
    }
}

fn validate_range(start: usize, len: usize, capacity: usize) -> Result<(), BufferError> {
    if len == 0 {
        return Ok(());
    }

    let end = start
        .checked_add(len)
        .ok_or(BufferError::Overflow)?;

    if end > capacity {
        return Err(BufferError::OutOfBounds {
            index: end - 1,
            len: capacity,
        });
    }

    Ok(())
}

#[cfg(all(test, feature = "std-tests"))]
mod tests {
    use super::*;
    use core::ptr::NonNull;

    #[test]
    fn slice_memory_read_write() {
        let mut backing = [0u32; 4];
        let mut mem = SliceMemoryAccess::new(&mut backing);

        mem.write(0, 0xDEAD_BEEF).unwrap();
        mem.write_slice(1, &[1, 2, 3]).unwrap();

        let mut tmp = [0u32; 4];
        let len = tmp.len();
        {
            let mut tmp_access = SliceMemoryAccess::new(&mut tmp);
            mem.copy_into_access(&mut tmp_access, len).unwrap();
        }

        assert_eq!(tmp, [0xDEAD_BEEF, 1, 2, 3]);
    }

    #[test]
    fn safe_buffer_to_slice_transfer() {
        static mut RAW: [u32; 4] = [0; 4];
        let mut safe = unsafe {
            SafeBuffer::new(NonNull::new_unchecked(RAW.as_mut_ptr()), RAW.len()).unwrap()
        };

        safe.fill(0xABCD_EF01).unwrap();

        let mut slice = [0u32; 4];
        let slice_len = slice.len();
        {
            let mut slice_access = SliceMemoryAccess::new(&mut slice);
            safe.copy_into_access(&mut slice_access, slice_len).unwrap();
        }

        assert_eq!(slice, [0xABCD_EF01; 4]);
    }
}
