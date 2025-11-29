// libuser/src/abi/io_uring_common.rs
//! Common io_uring-style ABI definitions
//!
//! Shared data structures for asynchronous I/O between user space and kernel space,
//! used by both V1 (legacy) and V2 (capability-based) io_uring protocols.

use core::sync::atomic::{AtomicU32, Ordering};

/// Ring buffer size (must be power of 2)
/// 
/// 256 entries provides good batching while keeping memory usage reasonable.
/// Each SQ entry is 64 bytes, CQ entry is 16 bytes.
/// Total: 256 * (64 + 16) = 20KB per ring pair
pub const RING_SIZE: u32 = 256;

/// Ring mask for efficient modulo operation
pub const RING_MASK: u32 = RING_SIZE - 1;

/// I/O operation codes
///
/// These correspond to the operations that can be submitted via the ring buffer.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpCode {
    /// No operation (for padding/alignment)
    Nop = 0,
    /// Read from file descriptor
    Read = 1,
    /// Write to file descriptor  
    Write = 2,
    /// Open file
    Open = 3,
    /// Close file descriptor
    Close = 4,
    /// Synchronous file data
    Fsync = 5,
    /// Poll for events
    Poll = 6,
    /// Cancel a pending request
    Cancel = 7,
    /// Link timeout to operation
    LinkTimeout = 8,
    /// Connect socket
    Connect = 9,
    /// Accept connection
    Accept = 10,
    /// Send data
    Send = 11,
    /// Receive data
    Recv = 12,
    /// Memory map
    Mmap = 13,
    /// Memory unmap
    Munmap = 14,
    /// Exit process (immediate, doesn't use ring)
    Exit = 255,
}

impl OpCode {
    /// Convert from raw u8 value
    #[must_use]
    pub const fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Nop),
            1 => Some(Self::Read),
            2 => Some(Self::Write),
            3 => Some(Self::Open),
            4 => Some(Self::Close),
            5 => Some(Self::Fsync),
            6 => Some(Self::Poll),
            7 => Some(Self::Cancel),
            8 => Some(Self::LinkTimeout),
            9 => Some(Self::Connect),
            10 => Some(Self::Accept),
            11 => Some(Self::Send),
            12 => Some(Self::Recv),
            13 => Some(Self::Mmap),
            14 => Some(Self::Munmap),
            255 => Some(Self::Exit),
            _ => None,
        }
    }
}

/// Submission entry flags
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IoUringFlags(pub u8);

impl IoUringFlags {
    /// No special flags
    pub const NONE: Self = Self(0);
    
    /// Link this request with the next one
    /// If this request fails, the linked request is cancelled
    pub const LINK: Self = Self(1 << 0);
    
    /// Hardlink - always submit next request regardless of this one's result
    pub const HARDLINK: Self = Self(1 << 1);
    
    /// Force async execution (don't try synchronous fast path)
    pub const ASYNC: Self = Self(1 << 2);
    
    /// Buffer is registered (zero-copy optimization)
    pub const FIXED_BUFFER: Self = Self(1 << 3);
    
    /// File descriptor is registered
    pub const FIXED_FILE: Self = Self(1 << 4);
    
    /// Drain - wait for all prior requests to complete
    pub const DRAIN: Self = Self(1 << 5);
}

impl core::ops::BitOr for IoUringFlags {
    type Output = Self;
    
    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl core::ops::BitAnd for IoUringFlags {
    type Output = Self;
    
    fn bitand(self, rhs: Self) -> Self::Output {
        Self(self.0 & rhs.0)
    }
}