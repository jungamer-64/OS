//! High-level I/O API
//!
//! This module provides safe, high-level I/O functions for user programs.

use crate::syscall::{self, SyscallResult};

/// Standard file descriptors
pub const STDIN: u64 = 0;
pub const STDOUT: u64 = 1;
pub const STDERR: u64 = 2;

/// Write a byte slice to a file descriptor
///
/// # Arguments
/// * `fd` - File descriptor
/// * `buf` - Data to write
///
/// # Returns
/// Number of bytes written
///
/// # Errors
/// * `EBADF` - Invalid file descriptor
/// * `EFAULT` - Invalid buffer pointer
/// * `EIO` - I/O error
pub fn write(fd: u64, buf: &[u8]) -> SyscallResult<usize> {
    syscall::write(fd, buf)
}

/// Read from a file descriptor into a buffer
///
/// # Arguments
/// * `fd` - File descriptor
/// * `buf` - Buffer to read into
///
/// # Returns
/// Number of bytes read (0 = EOF)
///
/// # Errors
/// * `EBADF` - Invalid file descriptor
/// * `EFAULT` - Invalid buffer pointer
/// * `EIO` - I/O error
pub fn read(fd: u64, buf: &mut [u8]) -> SyscallResult<usize> {
    syscall::read(fd, buf)
}

/// Print a string to stdout
///
/// This function ignores errors. Use `write()` directly if you need
/// error handling.
pub fn print(s: &str) {
    let _ = write(STDOUT, s.as_bytes());
}

/// Print a string to stdout with a newline
pub fn println(s: &str) {
    print(s);
    print("\n");
}

/// Print a string to stderr
pub fn eprint(s: &str) {
    let _ = write(STDERR, s.as_bytes());
}

/// Print a string to stderr with a newline
pub fn eprintln(s: &str) {
    eprint(s);
    eprint("\n");
}

/// Macro for formatted printing (simplified版)
///
/// Note: This macro doesn't support formatting in no_std environment
/// without alloc. Use the `print()` function for string literals.
#[macro_export]
macro_rules! print {
    ($s:expr) => {
        $crate::io::print($s)
    };
}

/// Macro for formatted printing with newline
#[macro_export]
macro_rules! println {
    () => {
        $crate::io::println("")
    };
    ($s:expr) => {
        $crate::io::println($s)
    };
}

/// Macro for printing to stderr
#[macro_export]
macro_rules! eprint {
    ($s:expr) => {
        $crate::io::eprint($s)
    };
}

/// Macro for printing to stderr with newline
#[macro_export]
macro_rules! eprintln {
    () => {
        $crate::io::eprintln("")
    };
    ($s:expr) => {
        $crate::io::eprintln($s)
    };
}
