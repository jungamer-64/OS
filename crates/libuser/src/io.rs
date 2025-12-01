//! High-level I/O API
//!
//! This module provides safe, high-level I/O functions for user programs.

use crate::syscall::{self, SyscallResult};

use core::fmt;

/// Standard file descriptors
/// Standard input file descriptor
pub const STDIN: u64 = 0;
/// Standard output file descriptor
pub const STDOUT: u64 = 1;
/// Standard error file descriptor
pub const STDERR: u64 = 2;

/// Write a byte slice to a file descriptor
pub fn write(fd: u64, buf: &[u8]) -> SyscallResult<usize> {
    syscall::write(fd, buf)
}

/// Read from a file descriptor into a buffer
pub fn read(fd: u64, buf: &mut [u8]) -> SyscallResult<usize> {
    syscall::read(fd, buf)
}

/// Pipe ends
pub struct PipeReader(u64);
pub struct PipeWriter(u64);

impl PipeReader {
    pub fn read(&mut self, buf: &mut [u8]) -> SyscallResult<usize> {
        read(self.0, buf)
    }
    
    pub fn as_raw_fd(&self) -> u64 {
        self.0
    }
}

impl PipeWriter {
    pub fn write(&mut self, buf: &[u8]) -> SyscallResult<usize> {
        write(self.0, buf)
    }
    
    pub fn as_raw_fd(&self) -> u64 {
        self.0
    }
}

/// Create a new pipe
pub fn pipe() -> SyscallResult<(PipeReader, PipeWriter)> {
    let mut fds = [0i32; 2];
    syscall::pipe(&mut fds)?;
    Ok((PipeReader(fds[0] as u64), PipeWriter(fds[1] as u64)))
}

/// Read a line from stdin
///
/// Reads characters until a newline ('\n') is encountered or the buffer is full.
/// The newline character is included in the buffer if there is space.
///
/// # Arguments
/// * `buf` - Buffer to read into
///
/// # Returns
/// Number of bytes read
pub fn read_line(buf: &mut [u8]) -> SyscallResult<usize> {
    let mut bytes_read = 0;
    let mut c = [0u8; 1];
    
    while bytes_read < buf.len() {
        match read(STDIN, &mut c) {
            Ok(0) => break, // EOF
            Ok(_) => {
                buf[bytes_read] = c[0];
                bytes_read += 1;
                
                if c[0] == b'\n' {
                    break;
                }
            }
            Err(e) => return Err(e),
        }
    }
    
    Ok(bytes_read)
}

/// Print a string to stdout
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

// --- Formatting Support ---

/// Stdout writer for fmt::Write
pub struct Stdout;

impl fmt::Write for Stdout {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        print(s);
        Ok(())
    }
}

/// Internal print function for macros
#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    let _ = Stdout.write_fmt(args);
}

/// Macro for formatted printing
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::io::_print(format_args!($($arg)*)));
}

/// Macro for formatted printing with newline
#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}

/// Macro for printing to stderr
#[macro_export]
macro_rules! eprint {
    ($($arg:tt)*) => {
        // Simple implementation for now, ideally should use a Stderr struct
        // But we can't easily reuse _print for stderr without passing fd
        // So let's just format to a buffer? No, no alloc.
        // For now, let's just use print! but to stderr?
        // Or implement _eprint
        $crate::io::_eprint(format_args!($($arg)*))
    };
}

/// Macro for printing to stderr with newline
#[macro_export]
macro_rules! eprintln {
    () => ($crate::eprint!("\n"));
    ($($arg:tt)*) => ($crate::eprint!("{}\n", format_args!($($arg)*)));
}

/// Stderr writer for fmt::Write
pub struct Stderr;

impl fmt::Write for Stderr {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        eprint(s);
        Ok(())
    }
}

/// Internal eprint function for macros
#[doc(hidden)]
pub fn _eprint(args: fmt::Arguments) {
    use core::fmt::Write;
    let _ = Stderr.write_fmt(args);
}

/// Internal println function for macros
#[doc(hidden)]
pub fn _println(args: fmt::Arguments) {
    _print(args);
    _print(format_args!("\n"));
}

/// Internal eprintln function for macros
#[doc(hidden)]
pub fn _eprintln(args: fmt::Arguments) {
    _eprint(args);
    _eprint(format_args!("\n"));
}
