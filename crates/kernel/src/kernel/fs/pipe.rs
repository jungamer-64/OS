use super::{FileDescriptor, FileResult, FileError};
use alloc::collections::VecDeque;
use alloc::sync::Arc;
use spin::Mutex;

/// A simple pipe implementation
pub struct Pipe {
    buffer: VecDeque<u8>,
    write_closed: bool,
}

impl Pipe {
    /// Creates a new empty pipe.
    pub fn new() -> Self {
        Self {
            buffer: VecDeque::new(),
            write_closed: false,
        }
    }
    
    /// Reads data from the pipe into the provided buffer.
    pub fn read(&mut self, buf: &mut [u8]) -> FileResult<usize> {
        if self.buffer.is_empty() {
            if self.write_closed {
                return Ok(0); // EOF
            }
            return Err(FileError::WouldBlock);
        }
        
        let mut read_count = 0;
        for b in buf.iter_mut() {
            if let Some(byte) = self.buffer.pop_front() {
                *b = byte;
                read_count += 1;
            } else {
                break;
            }
        }
        Ok(read_count)
    }
    
    /// Writes data from the provided buffer into the pipe.
    pub fn write(&mut self, buf: &[u8]) -> FileResult<usize> {
        if self.write_closed {
            return Err(FileError::BrokenPipe);
        }
        
        for &byte in buf {
            self.buffer.push_back(byte);
        }
        Ok(buf.len())
    }
    
    /// Closes the write end of the pipe.
    pub fn close_write(&mut self) {
        self.write_closed = true;
    }
}

/// Read end of a pipe.
pub struct PipeReader {
    /// Shared pipe instance
    pub pipe: Arc<Mutex<Pipe>>,
}

impl FileDescriptor for PipeReader {
    fn read(&mut self, buf: &mut [u8]) -> FileResult<usize> {
        let mut pipe = self.pipe.lock();
        pipe.read(buf)
    }
    
    fn write(&mut self, _buf: &[u8]) -> FileResult<usize> {
        Err(FileError::InvalidInput)
    }
    
    fn close(&mut self) -> FileResult<()> {
        Ok(())
    }
}

/// Write end of a pipe.
pub struct PipeWriter {
    /// Shared pipe instance
    pub pipe: Arc<Mutex<Pipe>>,
}

impl FileDescriptor for PipeWriter {
    fn read(&mut self, _buf: &mut [u8]) -> FileResult<usize> {
        Err(FileError::InvalidInput)
    }
    
    fn write(&mut self, buf: &[u8]) -> FileResult<usize> {
        // Check if reader is gone?
        // If Arc strong count is 1 (only this writer), then reader is gone.
        if Arc::strong_count(&self.pipe) == 1 {
            return Err(FileError::BrokenPipe);
        }
        
        let mut pipe = self.pipe.lock();
        pipe.write(buf)
    }
    
    fn close(&mut self) -> FileResult<()> {
        let mut pipe = self.pipe.lock();
        pipe.close_write();
        Ok(())
    }
}
