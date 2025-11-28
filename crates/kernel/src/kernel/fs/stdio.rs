// kernel/src/kernel/fs/stdio.rs
//! Standard I/O devices (stdin, stdout, stderr) as FileDescriptor implementations
//!
//! This module provides capability-based standard I/O by wrapping the serial
//! console as FileDescriptor implementations. These can be stored in the
//! capability table as VfsFile resources.

use super::{FileDescriptor, FileError, FileResult, VfsFile, VfsFileType};
use alloc::sync::Arc;
use crate::kernel::core::traits::CharDevice;
use crate::kernel::driver::serial::SERIAL1;

/// Standard input (stdin) - reads from serial console
/// 
/// Currently returns WouldBlock since no blocking input is implemented.
pub struct Stdin;

impl FileDescriptor for Stdin {
    fn read(&mut self, _buf: &mut [u8]) -> FileResult<usize> {
        // TODO: Implement non-blocking read from serial
        // For now, return WouldBlock since we don't have buffered input
        Err(FileError::WouldBlock)
    }

    fn write(&mut self, _buf: &[u8]) -> FileResult<usize> {
        Err(FileError::InvalidArgument)
    }

    fn close(&mut self) -> FileResult<()> {
        // stdin cannot be closed
        Err(FileError::InvalidArgument)
    }
}

/// Standard output (stdout) - writes to serial console
pub struct Stdout;

impl FileDescriptor for Stdout {
    fn read(&mut self, _buf: &mut [u8]) -> FileResult<usize> {
        Err(FileError::InvalidArgument)
    }

    fn write(&mut self, buf: &[u8]) -> FileResult<usize> {
        if let Some(mut serial) = SERIAL1.try_lock() {
            for &byte in buf {
                let _ = serial.write_byte(byte);
            }
            Ok(buf.len())
        } else {
            Err(FileError::WouldBlock)
        }
    }

    fn close(&mut self) -> FileResult<()> {
        // stdout cannot be closed
        Err(FileError::InvalidArgument)
    }
}

/// Standard error (stderr) - writes to serial console
pub struct Stderr;

impl FileDescriptor for Stderr {
    fn read(&mut self, _buf: &mut [u8]) -> FileResult<usize> {
        Err(FileError::InvalidArgument)
    }

    fn write(&mut self, buf: &[u8]) -> FileResult<usize> {
        if let Some(mut serial) = SERIAL1.try_lock() {
            for &byte in buf {
                let _ = serial.write_byte(byte);
            }
            Ok(buf.len())
        } else {
            Err(FileError::WouldBlock)
        }
    }

    fn close(&mut self) -> FileResult<()> {
        // stderr cannot be closed
        Err(FileError::InvalidArgument)
    }
}

/// Reserved capability IDs for standard I/O
/// 
/// These IDs are reserved across all processes and correspond to:
/// - STDIN_CAP_ID: 0 (standard input)
/// - STDOUT_CAP_ID: 1 (standard output)  
/// - STDERR_CAP_ID: 2 (standard error)
///
/// Note: These are the same as traditional Unix FD numbers for compatibility.
pub const STDIN_CAP_ID: u64 = 0;
pub const STDOUT_CAP_ID: u64 = 1;
pub const STDERR_CAP_ID: u64 = 2;

/// First available capability ID for user allocations
/// 
/// User file capabilities start from ID 3, just like traditional Unix.
pub const FIRST_USER_CAP_ID: u64 = 3;

/// Create VfsFile wrappers for stdin, stdout, stderr
impl Stdin {
    /// Create a VfsFile-wrapped stdin
    #[must_use]
    pub fn as_vfs_file() -> Arc<VfsFile> {
        VfsFile::arc_with_type(Stdin, VfsFileType::CharDevice)
    }
}

impl Stdout {
    /// Create a VfsFile-wrapped stdout
    #[must_use]
    pub fn as_vfs_file() -> Arc<VfsFile> {
        VfsFile::arc_with_type(Stdout, VfsFileType::CharDevice)
    }
}

impl Stderr {
    /// Create a VfsFile-wrapped stderr
    #[must_use]
    pub fn as_vfs_file() -> Arc<VfsFile> {
        VfsFile::arc_with_type(Stderr, VfsFileType::CharDevice)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stdin_cannot_write() {
        let mut stdin = Stdin;
        assert!(matches!(stdin.write(b"test"), Err(FileError::InvalidArgument)));
    }

    #[test]
    fn test_stdout_cannot_read() {
        let mut stdout = Stdout;
        let mut buf = [0u8; 4];
        assert!(matches!(stdout.read(&mut buf), Err(FileError::InvalidArgument)));
    }

    #[test]
    fn test_reserved_ids() {
        assert_eq!(STDIN_CAP_ID, 0);
        assert_eq!(STDOUT_CAP_ID, 1);
        assert_eq!(STDERR_CAP_ID, 2);
        assert_eq!(FIRST_USER_CAP_ID, 3);
    }
}
