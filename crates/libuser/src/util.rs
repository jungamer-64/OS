//! Utility functions for userland programs
//!
//! This module provides various helper functions that don't fit
//! into other modules.

use crate::syscall::SyscallResult;

/// Convert a syscall result to a boolean
///
/// # Examples
/// ```
/// use libuser::util::is_ok;
///
/// let result: Result<usize, SyscallError> = Ok(10);
/// assert!(is_ok(&result));
/// ```
pub fn is_ok<T>(result: &SyscallResult<T>) -> bool {
    result.is_ok()
}

/// Convert a syscall result to an Option
///
/// This is useful when you want to ignore errors.
///
/// # Examples
/// ```
/// use libuser::util::to_option;
///
/// let result: Result<usize, SyscallError> = Ok(10);
/// assert_eq!(to_option(result), Some(10));
/// ```
pub fn to_option<T>(result: SyscallResult<T>) -> Option<T> {
    result.ok()
}

/// Panic if a syscall result is an error
///
/// # Examples
/// ```should_panic
/// use libuser::util::unwrap_or_panic;
/// use libuser::syscall::SyscallError;
///
/// let result: Result<usize, SyscallError> = Err(SyscallError::new(-1));
/// unwrap_or_panic(result, "failed");  // Panics with message
/// ```
pub fn unwrap_or_panic<T>(result: SyscallResult<T>, msg: &str) -> T {
    match result {
        Ok(v) => v,
        Err(e) => {
            crate::io::eprintln("Error: ");
            crate::io::eprintln(msg);
            crate::io::eprintln(" - ");
            crate::io::eprintln(e.description());
            crate::process::exit(1);
        }
    }
}

/// Helper macro to unwrap or exit with error message
///
/// # Examples
/// ```
/// use libuser::unwrap_or_exit;
///
/// let result = some_syscall();
/// let value = unwrap_or_exit!(result, "syscall failed");
/// ```
#[macro_export]
macro_rules! unwrap_or_exit {
    ($result:expr, $msg:expr) => {
        match $result {
            Ok(v) => v,
            Err(e) => {
                $crate::io::eprint("Error: ");
                $crate::io::eprint($msg);
                $crate::io::eprint(" - ");
                $crate::io::eprintln(e.description());
                $crate::process::exit(1);
            }
        }
    };
}
