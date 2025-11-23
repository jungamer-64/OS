//! File System and IPC module

/// Pipe implementation for inter-process communication.
pub mod pipe;

/// Result type for file operations
pub type FileResult<T> = Result<T, FileError>;

/// File operation errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileError {
    /// Operation not implemented
    NotImplemented,
    /// Broken pipe (writer closed)
    BrokenPipe,
    /// Operation would block
    WouldBlock,
    /// Invalid input provided
    InvalidInput,
    /// Other error
    Other,
}

/// File Descriptor Trait
/// 
/// Represents an open file, pipe, or other resource that can be read/written.
pub trait FileDescriptor: Send + Sync {
    /// Read bytes from the file into the buffer
    /// Returns the number of bytes read
    fn read(&mut self, buf: &mut [u8]) -> FileResult<usize>;
    
    /// Write bytes to the file from the buffer
    /// Returns the number of bytes written
    fn write(&mut self, buf: &[u8]) -> FileResult<usize>;
    
    /// Close the file descriptor
    fn close(&mut self) -> FileResult<()>;
    
    /// Poll for readiness (optional, for non-blocking I/O)
    fn poll(&self) -> bool {
        true
    }
}
