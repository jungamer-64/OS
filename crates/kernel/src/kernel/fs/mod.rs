// kernel/src/kernel/fs/mod.rs
//! Filesystem abstraction layer

// pub mod initrd;  // TODO: Implement
// pub mod vfs;     // TODO: Implement

/// File operation errors
#[derive(Debug, Clone, Copy)]
pub enum FileError {
    /// End of file / broken pipe
    BrokenPipe,
    /// Operation would block
    WouldBlock,
    /// Input/output error
    IoError,
    /// Invalid argument
    InvalidArgument,
}

/// File descriptor trait (stub for now)
pub trait FileDescriptor: Send + Sync {
    /// Read from file descriptor
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, FileError>;
    
    /// Write to file descriptor
    fn write(&mut self, buf: &[u8]) -> Result<usize, FileError>;
    
    /// Close file descriptor
    fn close(&mut self) -> Result<(), FileError> {
        Ok(()) // Default implementation does nothing
    }
}

/// Trait for filesystem implementations
pub trait FileSystem: Send + Sync {
    /// Read file contents
    fn read_file(&self, path: &str) -> Option<&[u8]>;
    
    /// Check if file exists
    fn exists(&self, path: &str) -> bool {
        self.read_file(path).is_some()
    }
}
