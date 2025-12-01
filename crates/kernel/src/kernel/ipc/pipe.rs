//! Pipe implementation based on Channel
//!
//! This module provides a `Pipe` implementation that uses `Channel<u8>`
//! for the underlying storage. It implements `FileDescriptor` for compatibility
//! with the VFS.

use super::channel::{Channel, ChannelError};
use crate::kernel::fs::{FileDescriptor, FileError, FileResult};

/// Pipe built on top of Channel<u8>
pub struct Pipe {
    channel: Channel<u8>,
}

impl Pipe {
    /// Create a new pipe with given capacity
    pub fn new(capacity: usize) -> (PipeReader, PipeWriter) {
        let channel = Channel::new(capacity);
        (
            PipeReader {
                channel: channel.clone(),
            },
            PipeWriter { channel },
        )
    }
}

/// Reader end of the pipe
pub struct PipeReader {
    channel: Channel<u8>,
}

impl FileDescriptor for PipeReader {
    fn read(&mut self, buf: &mut [u8]) -> FileResult<usize> {
        let mut read_count = 0;
        for byte in buf.iter_mut() {
            match self.channel.recv() {
                Ok(b) => {
                    *byte = b;
                    read_count += 1;
                }
                Err(ChannelError::Empty) => {
                    // If we read something, return it. If nothing, block (not implemented) or return 0?
                    // For non-blocking, return what we have.
                    break;
                }
                Err(ChannelError::Closed) => {
                    // EOF
                    break;
                }
                Err(_) => return Err(FileError::IoError),
            }
        }
        Ok(read_count)
    }

    fn write(&mut self, _buf: &[u8]) -> FileResult<usize> {
        Err(FileError::AccessDenied)
    }

    fn close(&mut self) -> FileResult<()> {
        // Reader close doesn't necessarily close the channel for writing,
        // but usually we might want to signal?
        // For now just drop.
        Ok(())
    }
}

/// Writer end of the pipe
pub struct PipeWriter {
    channel: Channel<u8>,
}

impl PipeWriter {
    /// Close the writer end (signals EOF to reader)
    pub fn close_writer(&self) {
        self.channel.close();
    }
    
    /// Write bytes helper
    pub fn write_bytes(&mut self, buf: &[u8]) -> FileResult<usize> {
        self.write(buf)
    }
}

impl FileDescriptor for PipeWriter {
    fn read(&mut self, _buf: &mut [u8]) -> FileResult<usize> {
        Err(FileError::AccessDenied)
    }

    fn write(&mut self, buf: &[u8]) -> FileResult<usize> {
        if self.channel.is_closed() {
            return Err(FileError::BrokenPipe);
        }

        let mut written_count = 0;
        for &byte in buf {
            match self.channel.send(byte) {
                Ok(_) => {
                    written_count += 1;
                }
                Err(ChannelError::Full) => {
                    // Buffer full, return what we wrote
                    break;
                }
                Err(ChannelError::Closed) => return Err(FileError::BrokenPipe),
                Err(_) => return Err(FileError::IoError),
            }
        }
        
        if written_count == 0 && !buf.is_empty() && self.channel.is_full() {
             // If we couldn't write anything because full, return 0 (EAGAIN equivalent)
             // or block? For now non-blocking.
             return Ok(0);
        }

        Ok(written_count)
    }

    fn close(&mut self) -> FileResult<()> {
        self.channel.close();
        Ok(())
    }
}

impl PipeReader {
    /// Read bytes helper
    pub fn read_bytes(&mut self, buf: &mut [u8]) -> FileResult<usize> {
        self.read(buf)
    }
}
