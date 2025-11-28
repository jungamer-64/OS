//! ABI-safe Result Types (User Space)
//!
//! Result types that can safely cross the ABI boundary.

use super::error::SyscallError;

/// Type alias for syscall results
pub type SyscallResult<T> = Result<T, SyscallError>;

/// ABI-safe Result for crossing kernel/user boundary
///
/// This is a tagged union representation that can be safely
/// passed across the syscall boundary.
///
/// # Memory Layout
/// ```text
/// [0..7]   tag: 0 = Ok, 1 = Err
/// [8..15]  value (for Ok) or error code (for Err)
/// ```
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct AbiResult<T: Copy, E: Copy> {
    tag: u64,
    value: AbiResultValue<T, E>,
}

#[repr(C)]
#[derive(Clone, Copy)]
union AbiResultValue<T: Copy, E: Copy> {
    ok: T,
    err: E,
}

impl<T: Copy, E: Copy> core::fmt::Debug for AbiResultValue<T, E> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "AbiResultValue {{ ... }}")
    }
}

impl<T: Copy, E: Copy> AbiResult<T, E> {
    /// Tag value for Ok variant
    pub const TAG_OK: u64 = 0;
    /// Tag value for Err variant
    pub const TAG_ERR: u64 = 1;

    /// Create a successful result
    #[must_use]
    pub const fn ok(value: T) -> Self {
        Self {
            tag: Self::TAG_OK,
            value: AbiResultValue { ok: value },
        }
    }

    /// Create an error result
    #[must_use]
    pub const fn err(error: E) -> Self {
        Self {
            tag: Self::TAG_ERR,
            value: AbiResultValue { err: error },
        }
    }

    /// Check if this is an Ok result
    #[must_use]
    pub const fn is_ok(&self) -> bool {
        self.tag == Self::TAG_OK
    }

    /// Check if this is an Err result
    #[must_use]
    pub const fn is_err(&self) -> bool {
        self.tag == Self::TAG_ERR
    }

    /// Get the Ok value (panics if Err)
    #[must_use]
    pub fn unwrap(self) -> T {
        if self.is_ok() {
            unsafe { self.value.ok }
        } else {
            panic!("called unwrap on an Err value");
        }
    }

    /// Get the Err value (panics if Ok)
    #[must_use]
    pub fn unwrap_err(self) -> E {
        if self.is_err() {
            unsafe { self.value.err }
        } else {
            panic!("called unwrap_err on an Ok value");
        }
    }

    /// Convert to standard Result
    #[must_use]
    pub fn into_result(self) -> Result<T, E> {
        if self.is_ok() {
            Ok(unsafe { self.value.ok })
        } else {
            Err(unsafe { self.value.err })
        }
    }
}

impl<T: Copy, E: Copy> From<Result<T, E>> for AbiResult<T, E> {
    fn from(result: Result<T, E>) -> Self {
        match result {
            Ok(v) => Self::ok(v),
            Err(e) => Self::err(e),
        }
    }
}

impl<T: Copy, E: Copy> From<AbiResult<T, E>> for Result<T, E> {
    fn from(abi_result: AbiResult<T, E>) -> Self {
        abi_result.into_result()
    }
}

/// ABI result with i64 value
pub type AbiResultI64 = AbiResult<i64, u16>;
/// ABI result with u64 value
pub type AbiResultU64 = AbiResult<u64, u16>;
/// ABI result with usize value
pub type AbiResultUsize = AbiResult<usize, u16>;
