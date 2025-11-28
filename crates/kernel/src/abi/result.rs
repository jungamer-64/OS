// kernel/src/abi/result.rs
//! ABI-safe Result Type
//!
//! This module defines `AbiResult<T, E>`, a type that can safely cross
//! the user-kernel ABI boundary while preserving Rust's `Result` semantics.
//!
//! # Design Philosophy
//!
//! - **Zero-cost**: Same size as `max(size_of::<T>(), size_of::<E>()) + 8`
//! - **Safe**: Cannot be misinterpreted due to explicit tag
//! - **Ergonomic**: Converts to/from Rust `Result` seamlessly
//!
//! # Memory Layout
//!
//! ```text
//! +------------------+
//! | tag (1 byte)     |  0 = Ok, 1 = Err
//! | padding (7 bytes)|
//! | data (N bytes)   |  Either T or E, depending on tag
//! +------------------+
//! ```

use core::mem::ManuallyDrop;

use super::error::SyscallError;

/// ABI-safe Result type
///
/// This type provides a way to pass `Result<T, E>` across the ABI boundary
/// safely. Unlike Rust's `Result`, which has an undefined layout, `AbiResult`
/// has a stable, predictable memory layout.
///
/// # Type Parameters
///
/// - `T`: The success type. Must be `Copy` for safe ABI crossing.
/// - `E`: The error type. Defaults to `SyscallError`.
///
/// # Example
///
/// ```ignore
/// // Kernel side: return AbiResult
/// fn sys_read(cap: u64, buf: u64, len: u32) -> AbiResult<usize, SyscallError> {
///     match do_read(cap, buf, len) {
///         Ok(n) => AbiResult::ok(n),
///         Err(e) => AbiResult::err(e),
///     }
/// }
///
/// // User side: convert to Result
/// let result: Result<usize, SyscallError> = abi_result.into();
/// match result {
///     Ok(n) => println!("Read {} bytes", n),
///     Err(e) => println!("Error: {}", e),
/// }
/// ```
#[repr(C)]
pub struct AbiResult<T, E = SyscallError>
where
    T: Copy,
    E: Copy,
{
    /// Tag indicating Ok (0) or Err (1)
    tag: u8,
    /// Padding for alignment
    _pad: [u8; 7],
    /// Union containing either T or E
    data: AbiResultData<T, E>,
}

/// Internal union for storing either success or error value
#[repr(C)]
union AbiResultData<T: Copy, E: Copy> {
    ok: ManuallyDrop<T>,
    err: ManuallyDrop<E>,
}

// Safety: AbiResultData is Copy if both T and E are Copy
impl<T: Copy, E: Copy> Copy for AbiResultData<T, E> {}
impl<T: Copy, E: Copy> Clone for AbiResultData<T, E> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Copy, E: Copy> AbiResult<T, E> {
    /// Tag value for Ok variant
    pub const TAG_OK: u8 = 0;
    /// Tag value for Err variant
    pub const TAG_ERR: u8 = 1;

    /// Create a successful result
    #[must_use]
    #[inline]
    pub const fn ok(value: T) -> Self {
        Self {
            tag: Self::TAG_OK,
            _pad: [0; 7],
            data: AbiResultData {
                ok: ManuallyDrop::new(value),
            },
        }
    }

    /// Create an error result
    #[must_use]
    #[inline]
    pub const fn err(error: E) -> Self {
        Self {
            tag: Self::TAG_ERR,
            _pad: [0; 7],
            data: AbiResultData {
                err: ManuallyDrop::new(error),
            },
        }
    }

    /// Check if this is an Ok result
    #[must_use]
    #[inline]
    pub const fn is_ok(&self) -> bool {
        self.tag == Self::TAG_OK
    }

    /// Check if this is an Err result
    #[must_use]
    #[inline]
    pub const fn is_err(&self) -> bool {
        self.tag == Self::TAG_ERR
    }

    /// Get the Ok value, if present
    #[must_use]
    #[inline]
    pub fn ok_value(&self) -> Option<T> {
        if self.is_ok() {
            Some(unsafe { ManuallyDrop::into_inner(self.data.ok) })
        } else {
            None
        }
    }

    /// Get the Err value, if present
    #[must_use]
    #[inline]
    pub fn err_value(&self) -> Option<E> {
        if self.is_err() {
            Some(unsafe { ManuallyDrop::into_inner(self.data.err) })
        } else {
            None
        }
    }

    /// Convert to a Rust Result
    #[must_use]
    #[inline]
    pub fn into_result(self) -> Result<T, E> {
        if self.is_ok() {
            Ok(unsafe { ManuallyDrop::into_inner(self.data.ok) })
        } else {
            Err(unsafe { ManuallyDrop::into_inner(self.data.err) })
        }
    }
}

impl<T: Copy, E: Copy> From<Result<T, E>> for AbiResult<T, E> {
    #[inline]
    fn from(result: Result<T, E>) -> Self {
        match result {
            Ok(v) => Self::ok(v),
            Err(e) => Self::err(e),
        }
    }
}

impl<T: Copy, E: Copy> From<AbiResult<T, E>> for Result<T, E> {
    #[inline]
    fn from(abi: AbiResult<T, E>) -> Self {
        abi.into_result()
    }
}

impl<T: Copy, E: Copy> Copy for AbiResult<T, E> {}

impl<T: Copy, E: Copy> Clone for AbiResult<T, E> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Copy + core::fmt::Debug, E: Copy + core::fmt::Debug> core::fmt::Debug for AbiResult<T, E> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.is_ok() {
            f.debug_tuple("AbiResult::Ok")
                .field(unsafe { &*self.data.ok })
                .finish()
        } else {
            f.debug_tuple("AbiResult::Err")
                .field(unsafe { &*self.data.err })
                .finish()
        }
    }
}

impl<T: Copy + PartialEq, E: Copy + PartialEq> PartialEq for AbiResult<T, E> {
    fn eq(&self, other: &Self) -> bool {
        if self.tag != other.tag {
            return false;
        }
        if self.is_ok() {
            unsafe { *self.data.ok == *other.data.ok }
        } else {
            unsafe { *self.data.err == *other.data.err }
        }
    }
}

impl<T: Copy + Eq, E: Copy + Eq> Eq for AbiResult<T, E> {}

// === Specialized types for common cases ===

/// ABI result with i32 success value
pub type AbiResultI32 = AbiResult<i32, SyscallError>;

/// ABI result with i64 success value
pub type AbiResultI64 = AbiResult<i64, SyscallError>;

/// ABI result with u64 success value
pub type AbiResultU64 = AbiResult<u64, SyscallError>;

/// ABI result with usize success value
pub type AbiResultUsize = AbiResult<usize, SyscallError>;

/// ABI result with no success value (just success/error)
pub type AbiResultUnit = AbiResult<(), SyscallError>;

// === Compact result for CQE ===

/// Compact ABI result for Completion Queue Entries
///
/// This is a space-optimized result type specifically designed for CQE.
/// It packs the result into a single i64:
/// - If positive: success value
/// - If negative: negated error code (similar to traditional errno)
///
/// This is a bridge type for the transition period. New code should
/// prefer `AbiResult` where possible.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompactResult(i64);

impl CompactResult {
    /// Create a success result
    #[must_use]
    #[inline]
    pub const fn ok(value: i64) -> Self {
        debug_assert!(value >= 0, "Success value must be non-negative");
        Self(value)
    }

    /// Create an error result
    #[must_use]
    #[inline]
    pub const fn err(error: SyscallError) -> Self {
        Self(-(error.to_u32() as i64))
    }

    /// Check if this is a success
    #[must_use]
    #[inline]
    pub const fn is_ok(&self) -> bool {
        self.0 >= 0
    }

    /// Check if this is an error
    #[must_use]
    #[inline]
    pub const fn is_err(&self) -> bool {
        self.0 < 0
    }

    /// Get the raw i64 value
    #[must_use]
    #[inline]
    pub const fn raw(&self) -> i64 {
        self.0
    }

    /// Convert to Result
    #[must_use]
    #[inline]
    pub fn into_result(self) -> Result<i64, SyscallError> {
        if self.is_ok() {
            Ok(self.0)
        } else {
            Err(SyscallError::from_u32((-self.0) as u32))
        }
    }
}

impl From<Result<i64, SyscallError>> for CompactResult {
    #[inline]
    fn from(result: Result<i64, SyscallError>) -> Self {
        match result {
            Ok(v) => Self::ok(v),
            Err(e) => Self::err(e),
        }
    }
}

impl From<CompactResult> for Result<i64, SyscallError> {
    #[inline]
    fn from(compact: CompactResult) -> Self {
        compact.into_result()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_abi_result_ok() {
        let result: AbiResult<i32, SyscallError> = AbiResult::ok(42);
        assert!(result.is_ok());
        assert!(!result.is_err());
        assert_eq!(result.ok_value(), Some(42));
        assert_eq!(result.err_value(), None);
    }

    #[test]
    fn test_abi_result_err() {
        let result: AbiResult<i32, SyscallError> = AbiResult::err(SyscallError::NotFound);
        assert!(!result.is_ok());
        assert!(result.is_err());
        assert_eq!(result.ok_value(), None);
        assert_eq!(result.err_value(), Some(SyscallError::NotFound));
    }

    #[test]
    fn test_abi_result_conversion() {
        let rust_result: Result<i32, SyscallError> = Ok(42);
        let abi_result: AbiResult<i32, SyscallError> = rust_result.into();
        let back: Result<i32, SyscallError> = abi_result.into();
        assert_eq!(back, Ok(42));

        let rust_result: Result<i32, SyscallError> = Err(SyscallError::NotFound);
        let abi_result: AbiResult<i32, SyscallError> = rust_result.into();
        let back: Result<i32, SyscallError> = abi_result.into();
        assert_eq!(back, Err(SyscallError::NotFound));
    }

    #[test]
    fn test_compact_result() {
        let ok = CompactResult::ok(42);
        assert!(ok.is_ok());
        assert_eq!(ok.into_result(), Ok(42));

        let err = CompactResult::err(SyscallError::NotFound);
        assert!(err.is_err());
        assert_eq!(err.into_result(), Err(SyscallError::NotFound));
    }

    #[test]
    fn test_abi_result_size() {
        // AbiResult<i32, SyscallError> should be 8 (header) + max(4, 4) = 12, aligned to 8 = 16
        // Actually: tag(1) + pad(7) + data(4) = 12, but repr(C) may add padding
        let size = core::mem::size_of::<AbiResult<i32, SyscallError>>();
        assert!(size <= 16, "AbiResult should be compact: {}", size);
    }

    #[test]
    fn test_compact_result_size() {
        assert_eq!(core::mem::size_of::<CompactResult>(), 8);
    }
}
