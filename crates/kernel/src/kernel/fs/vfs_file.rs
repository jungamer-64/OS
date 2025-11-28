// kernel/src/kernel/fs/vfs_file.rs
//! VFS File wrapper for Capability-based resource management
//!
//! This module provides the `VfsFile` type that wraps `FileDescriptor`
//! implementations and integrates with the capability system.
//!
//! # Design
//!
//! `VfsFile` serves as the bridge between the capability system and
//! the filesystem layer:
//!
//! ```text
//! CapabilityEntry
//!     │
//!     └─► resource: Arc<VfsFile>
//!              │
//!              └─► inner: Mutex<Box<dyn FileDescriptor>>
//! ```
//!
//! This design:
//! - Allows `FileDescriptor` objects to be stored in `CapabilityEntry`
//! - Provides interior mutability for read/write operations
//! - Enables automatic cleanup when capability is closed

use alloc::boxed::Box;
use alloc::sync::Arc;
use spin::Mutex;
use core::any::Any;

use super::{FileDescriptor, FileError};

/// VFS File wrapper for capability integration
///
/// This type wraps a `FileDescriptor` and provides the necessary traits
/// for storage in a `CapabilityEntry`.
///
/// # Thread Safety
///
/// `VfsFile` uses interior mutability via `Mutex` to allow safe concurrent
/// access from multiple threads. The `FileDescriptor` trait requires
/// `Send + Sync`, so wrapped types must be thread-safe.
///
/// # Example
///
/// ```ignore
/// // Create a VfsFile from a PipeReader
/// let reader = PipeReader::new(pipe);
/// let vfs_file = VfsFile::new(reader);
///
/// // Store in capability table
/// let handle = cap_table.insert::<FileResource, _>(
///     Arc::new(vfs_file),
///     Rights::READ
/// )?;
///
/// // Later, retrieve and use
/// let entry = cap_table.get(&handle)?;
/// let vfs = entry.downcast::<VfsFile>().unwrap();
/// let bytes = vfs.read(&mut buffer)?;
/// ```
pub struct VfsFile {
    /// The underlying file descriptor
    inner: Mutex<Box<dyn FileDescriptor>>,
    /// File type for debugging/introspection
    file_type: VfsFileType,
}

/// Types of VFS files for debugging and introspection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VfsFileType {
    /// Regular file (from filesystem)
    Regular,
    /// Directory
    Directory,
    /// Pipe read end
    PipeRead,
    /// Pipe write end
    PipeWrite,
    /// Character device (e.g., serial)
    CharDevice,
    /// Block device
    BlockDevice,
    /// Socket
    Socket,
    /// Anonymous/unknown
    Anonymous,
}

impl VfsFile {
    /// Create a new VfsFile from a FileDescriptor
    #[must_use]
    pub fn new<F: FileDescriptor + 'static>(fd: F) -> Self {
        Self {
            inner: Mutex::new(Box::new(fd)),
            file_type: VfsFileType::Anonymous,
        }
    }

    /// Create a new VfsFile with an explicit type
    #[must_use]
    pub fn with_type<F: FileDescriptor + 'static>(fd: F, file_type: VfsFileType) -> Self {
        Self {
            inner: Mutex::new(Box::new(fd)),
            file_type,
        }
    }

    /// Create a VfsFile from a boxed FileDescriptor
    #[must_use]
    pub fn from_boxed(fd: Box<dyn FileDescriptor>, file_type: VfsFileType) -> Self {
        Self {
            inner: Mutex::new(fd),
            file_type,
        }
    }

    /// Get the file type
    #[must_use]
    pub fn file_type(&self) -> VfsFileType {
        self.file_type
    }

    /// Read from the file
    ///
    /// # Errors
    ///
    /// Returns `FileError` if the read operation fails.
    pub fn read(&self, buf: &mut [u8]) -> Result<usize, FileError> {
        let mut inner = self.inner.lock();
        inner.read(buf)
    }

    /// Write to the file
    ///
    /// # Errors
    ///
    /// Returns `FileError` if the write operation fails.
    pub fn write(&self, buf: &[u8]) -> Result<usize, FileError> {
        let mut inner = self.inner.lock();
        inner.write(buf)
    }

    /// Close the file
    ///
    /// This is called automatically when the capability is removed,
    /// but can also be called explicitly.
    ///
    /// # Errors
    ///
    /// Returns `FileError` if the close operation fails.
    pub fn close(&self) -> Result<(), FileError> {
        let mut inner = self.inner.lock();
        inner.close()
    }
}

// VfsFile is Send + Sync because:
// - inner: Mutex<Box<dyn FileDescriptor>> where FileDescriptor: Send + Sync
// - file_type: VfsFileType is Copy
unsafe impl Send for VfsFile {}
unsafe impl Sync for VfsFile {}

impl Drop for VfsFile {
    fn drop(&mut self) {
        // Attempt to close the file descriptor on drop
        // Ignore errors since we can't propagate them from drop
        let _ = self.close();
    }
}

impl core::fmt::Debug for VfsFile {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("VfsFile")
            .field("file_type", &self.file_type)
            .finish()
    }
}

/// Helper to create an Arc<VfsFile> directly
impl VfsFile {
    /// Create an Arc-wrapped VfsFile
    #[must_use]
    pub fn arc<F: FileDescriptor + 'static>(fd: F) -> Arc<Self> {
        Arc::new(Self::new(fd))
    }

    /// Create an Arc-wrapped VfsFile with explicit type
    #[must_use]
    pub fn arc_with_type<F: FileDescriptor + 'static>(fd: F, file_type: VfsFileType) -> Arc<Self> {
        Arc::new(Self::with_type(fd, file_type))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    // Mock file descriptor for testing
    struct MockFd {
        data: alloc::vec::Vec<u8>,
        pos: usize,
    }

    impl MockFd {
        fn new(data: &[u8]) -> Self {
            Self {
                data: data.to_vec(),
                pos: 0,
            }
        }
    }

    impl FileDescriptor for MockFd {
        fn read(&mut self, buf: &mut [u8]) -> Result<usize, FileError> {
            let remaining = self.data.len() - self.pos;
            if remaining == 0 {
                return Err(FileError::BrokenPipe);
            }
            let to_read = buf.len().min(remaining);
            buf[..to_read].copy_from_slice(&self.data[self.pos..self.pos + to_read]);
            self.pos += to_read;
            Ok(to_read)
        }

        fn write(&mut self, buf: &[u8]) -> Result<usize, FileError> {
            self.data.extend_from_slice(buf);
            Ok(buf.len())
        }
    }

    #[test]
    fn test_vfs_file_read_write() {
        let mock = MockFd::new(b"hello");
        let vfs = VfsFile::new(mock);
        
        let mut buf = [0u8; 5];
        let n = vfs.read(&mut buf).unwrap();
        assert_eq!(n, 5);
        assert_eq!(&buf, b"hello");
        
        let n = vfs.write(b" world").unwrap();
        assert_eq!(n, 6);
    }

    #[test]
    fn test_vfs_file_type() {
        let mock = MockFd::new(b"");
        let vfs = VfsFile::with_type(mock, VfsFileType::PipeRead);
        assert_eq!(vfs.file_type(), VfsFileType::PipeRead);
    }
}
