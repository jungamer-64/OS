// rany_os_abi/src/error.rs
//! Type-safe System Call Errors
//!
//! This module defines a comprehensive error type for system calls,
//! completely replacing the traditional errno-based error handling.
//!
//! # Design Philosophy
//!
//! - **No errno**: Errors are strongly typed, not magic numbers
//! - **Pattern matching**: All errors must be handled explicitly
//! - **Rich information**: Errors carry context, not just codes
//! - **ABI-safe**: Can be safely passed across the user-kernel boundary
//!
//! # Example
//!
//! ```ignore
//! match result {
//!     Ok(bytes) => println!("Read {} bytes", bytes),
//!     Err(SyscallError::NotFound) => println!("File not found"),
//!     Err(SyscallError::PermissionDenied) => println!("Access denied"),
//!     Err(e) => println!("Unexpected error: {:?}", e),
//! }
//! ```

/// System call error type
///
/// A comprehensive, type-safe error enum that replaces traditional
/// errno-based error handling. Each variant carries semantic meaning
/// that the compiler can verify.
///
/// # ABI Representation
///
/// The error is represented as a u32 for efficient ABI crossing.
/// The discriminant values are stable and must not be changed.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SyscallError {
    // === General Errors (0x00xx) ===
    /// Operation completed successfully (not an error)
    /// This variant exists for `AbiResult` compatibility but should
    /// never be used in an `Err` context.
    Success = 0x0000,

    /// Invalid argument provided
    InvalidArgument = 0x0001,

    /// Out of memory
    OutOfMemory = 0x0002,

    /// Permission denied
    PermissionDenied = 0x0003,

    /// Resource not found
    NotFound = 0x0004,

    /// Resource is busy
    Busy = 0x0005,

    /// Operation was interrupted
    Interrupted = 0x0006,

    /// Resource already exists
    AlreadyExists = 0x0007,

    /// Invalid state for this operation
    InvalidState = 0x0008,

    /// Operation timed out
    Timeout = 0x0009,

    /// Operation would block (for non-blocking I/O)
    WouldBlock = 0x000A,

    /// Too many open resources
    TooManyOpen = 0x000B,

    /// Name too long
    NameTooLong = 0x000C,

    /// Not a directory
    NotADirectory = 0x000D,

    /// Is a directory
    IsADirectory = 0x000E,

    /// Directory not empty
    DirectoryNotEmpty = 0x000F,

    // === I/O Errors (0x01xx) ===
    /// Generic I/O error
    IoError = 0x0100,

    /// End of file reached
    EndOfFile = 0x0101,

    /// Broken pipe
    BrokenPipe = 0x0102,

    /// Connection reset by peer
    ConnectionReset = 0x0103,

    /// Connection refused
    ConnectionRefused = 0x0104,

    /// Connection aborted
    ConnectionAborted = 0x0105,

    /// Not connected
    NotConnected = 0x0106,

    /// Address already in use
    AddressInUse = 0x0107,

    /// Address not available
    AddressNotAvailable = 0x0108,

    /// Network unreachable
    NetworkUnreachable = 0x0109,

    /// Host unreachable
    HostUnreachable = 0x010A,

    // === Capability Errors (0x02xx) ===
    /// Invalid capability handle
    InvalidCapability = 0x0200,

    /// Insufficient rights on capability
    InsufficientRights = 0x0201,

    /// Wrong capability type for this operation
    WrongCapabilityType = 0x0202,

    /// Capability has been revoked
    CapabilityRevoked = 0x0203,

    /// Capability table is full
    CapabilityTableFull = 0x0204,

    /// Cannot transfer capability to target
    CannotTransfer = 0x0205,

    // === io_uring Errors (0x03xx) ===
    /// Submission queue is full
    QueueFull = 0x0300,

    /// Completion queue overflow
    QueueOverflow = 0x0301,

    /// Buffer not registered
    BufferNotRegistered = 0x0302,

    /// Invalid buffer index
    InvalidBufferIndex = 0x0303,

    /// Invalid operation code
    InvalidOpCode = 0x0304,

    /// Operation cancelled
    OperationCancelled = 0x0305,

    /// Ring not setup
    RingNotSetup = 0x0306,

    // === Memory Errors (0x04xx) ===
    /// Invalid memory address
    InvalidAddress = 0x0400,

    /// Address not aligned
    NotAligned = 0x0401,

    /// Memory mapping failed
    MmapFailed = 0x0402,

    /// Access to unmapped memory
    UnmappedMemory = 0x0403,

    /// Stack overflow
    StackOverflow = 0x0404,

    // === Process Errors (0x05xx) ===
    /// No such process
    NoSuchProcess = 0x0500,

    /// Process limit reached
    ProcessLimitReached = 0x0501,

    /// Invalid executable format
    InvalidExecutable = 0x0502,

    // === Filesystem Errors (0x06xx) ===
    /// Filesystem full
    FilesystemFull = 0x0600,

    /// Read-only filesystem
    ReadOnlyFilesystem = 0x0601,

    /// Cross-device link
    CrossDeviceLink = 0x0602,

    /// Invalid seek position
    InvalidSeek = 0x0603,

    // === System Errors (0xFFxx) ===
    /// Operation not implemented
    NotImplemented = 0xFF00,

    /// Internal kernel error (should never happen)
    InternalError = 0xFF01,

    /// Unknown error code
    Unknown = 0xFFFF,
}

impl SyscallError {
    /// Convert from raw u32 value
    #[must_use]
    pub const fn from_u32(value: u32) -> Self {
        match value {
            0x0000 => Self::Success,
            0x0001 => Self::InvalidArgument,
            0x0002 => Self::OutOfMemory,
            0x0003 => Self::PermissionDenied,
            0x0004 => Self::NotFound,
            0x0005 => Self::Busy,
            0x0006 => Self::Interrupted,
            0x0007 => Self::AlreadyExists,
            0x0008 => Self::InvalidState,
            0x0009 => Self::Timeout,
            0x000A => Self::WouldBlock,
            0x000B => Self::TooManyOpen,
            0x000C => Self::NameTooLong,
            0x000D => Self::NotADirectory,
            0x000E => Self::IsADirectory,
            0x000F => Self::DirectoryNotEmpty,
            0x0100 => Self::IoError,
            0x0101 => Self::EndOfFile,
            0x0102 => Self::BrokenPipe,
            0x0103 => Self::ConnectionReset,
            0x0104 => Self::ConnectionRefused,
            0x0105 => Self::ConnectionAborted,
            0x0106 => Self::NotConnected,
            0x0107 => Self::AddressInUse,
            0x0108 => Self::AddressNotAvailable,
            0x0109 => Self::NetworkUnreachable,
            0x010A => Self::HostUnreachable,
            0x0200 => Self::InvalidCapability,
            0x0201 => Self::InsufficientRights,
            0x0202 => Self::WrongCapabilityType,
            0x0203 => Self::CapabilityRevoked,
            0x0204 => Self::CapabilityTableFull,
            0x0205 => Self::CannotTransfer,
            0x0300 => Self::QueueFull,
            0x0301 => Self::QueueOverflow,
            0x0302 => Self::BufferNotRegistered,
            0x0303 => Self::InvalidBufferIndex,
            0x0304 => Self::InvalidOpCode,
            0x0305 => Self::OperationCancelled,
            0x0306 => Self::RingNotSetup,
            0x0400 => Self::InvalidAddress,
            0x0401 => Self::NotAligned,
            0x0402 => Self::MmapFailed,
            0x0403 => Self::UnmappedMemory,
            0x0404 => Self::StackOverflow,
            0x0500 => Self::NoSuchProcess,
            0x0501 => Self::ProcessLimitReached,
            0x0502 => Self::InvalidExecutable,
            0x0600 => Self::FilesystemFull,
            0x0601 => Self::ReadOnlyFilesystem,
            0x0602 => Self::CrossDeviceLink,
            0x0603 => Self::InvalidSeek,
            0xFF00 => Self::NotImplemented,
            0xFF01 => Self::InternalError,
            _ => Self::Unknown,
        }
    }

    /// Get the raw u32 value
    #[must_use]
    pub const fn to_u32(self) -> u32 {
        self as u32
    }

    /// Get the error category
    #[must_use]
    pub const fn category(&self) -> ErrorCategory {
        match (*self as u32) >> 8 {
            0x00 => ErrorCategory::General,
            0x01 => ErrorCategory::Io,
            0x02 => ErrorCategory::Capability,
            0x03 => ErrorCategory::IoUring,
            0x04 => ErrorCategory::Memory,
            0x05 => ErrorCategory::Process,
            0x06 => ErrorCategory::Filesystem,
            0xFF => ErrorCategory::System,
            _ => ErrorCategory::Unknown,
        }
    }

    /// Check if this is a retriable error
    #[must_use]
    pub const fn is_retriable(&self) -> bool {
        matches!(
            self,
            Self::WouldBlock
                | Self::Interrupted
                | Self::Busy
                | Self::Timeout
                | Self::QueueFull
        )
    }

    /// Get a human-readable description
    #[must_use]
    pub const fn description(&self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::InvalidArgument => "invalid argument",
            Self::OutOfMemory => "out of memory",
            Self::PermissionDenied => "permission denied",
            Self::NotFound => "not found",
            Self::Busy => "resource busy",
            Self::Interrupted => "operation interrupted",
            Self::AlreadyExists => "already exists",
            Self::InvalidState => "invalid state",
            Self::Timeout => "operation timed out",
            Self::WouldBlock => "operation would block",
            Self::TooManyOpen => "too many open resources",
            Self::NameTooLong => "name too long",
            Self::NotADirectory => "not a directory",
            Self::IsADirectory => "is a directory",
            Self::DirectoryNotEmpty => "directory not empty",
            Self::IoError => "I/O error",
            Self::EndOfFile => "end of file",
            Self::BrokenPipe => "broken pipe",
            Self::ConnectionReset => "connection reset",
            Self::ConnectionRefused => "connection refused",
            Self::ConnectionAborted => "connection aborted",
            Self::NotConnected => "not connected",
            Self::AddressInUse => "address in use",
            Self::AddressNotAvailable => "address not available",
            Self::NetworkUnreachable => "network unreachable",
            Self::HostUnreachable => "host unreachable",
            Self::InvalidCapability => "invalid capability",
            Self::InsufficientRights => "insufficient rights",
            Self::WrongCapabilityType => "wrong capability type",
            Self::CapabilityRevoked => "capability revoked",
            Self::CapabilityTableFull => "capability table full",
            Self::CannotTransfer => "cannot transfer capability",
            Self::QueueFull => "queue full",
            Self::QueueOverflow => "queue overflow",
            Self::BufferNotRegistered => "buffer not registered",
            Self::InvalidBufferIndex => "invalid buffer index",
            Self::InvalidOpCode => "invalid operation code",
            Self::OperationCancelled => "operation cancelled",
            Self::RingNotSetup => "ring not setup",
            Self::InvalidAddress => "invalid address",
            Self::NotAligned => "address not aligned",
            Self::MmapFailed => "memory mapping failed",
            Self::UnmappedMemory => "unmapped memory access",
            Self::StackOverflow => "stack overflow",
            Self::NoSuchProcess => "no such process",
            Self::ProcessLimitReached => "process limit reached",
            Self::InvalidExecutable => "invalid executable",
            Self::FilesystemFull => "filesystem full",
            Self::ReadOnlyFilesystem => "read-only filesystem",
            Self::CrossDeviceLink => "cross-device link",
            Self::InvalidSeek => "invalid seek position",
            Self::NotImplemented => "not implemented",
            Self::InternalError => "internal error",
            Self::Unknown => "unknown error",
        }
    }
}

impl core::fmt::Display for SyscallError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.description())
    }
}

/// Error category for grouping related errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCategory {
    /// General errors
    General,
    /// I/O errors
    Io,
    /// Capability errors
    Capability,
    /// io_uring errors
    IoUring,
    /// Memory errors
    Memory,
    /// Process errors
    Process,
    /// Filesystem errors
    Filesystem,
    /// System errors
    System,
    /// Unknown category
    Unknown,
}

/// Type alias for syscall results
pub type SyscallResult<T> = Result<T, SyscallError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_roundtrip() {
        let errors = [
            SyscallError::InvalidArgument,
            SyscallError::NotFound,
            SyscallError::InvalidCapability,
            SyscallError::QueueFull,
            SyscallError::NotImplemented,
        ];

        for err in errors {
            let raw = err.to_u32();
            let restored = SyscallError::from_u32(raw);
            assert_eq!(err, restored);
        }
    }

    #[test]
    fn test_error_categories() {
        assert_eq!(
            SyscallError::InvalidArgument.category(),
            ErrorCategory::General
        );
        assert_eq!(SyscallError::IoError.category(), ErrorCategory::Io);
        assert_eq!(
            SyscallError::InvalidCapability.category(),
            ErrorCategory::Capability
        );
        assert_eq!(SyscallError::QueueFull.category(), ErrorCategory::IoUring);
        assert_eq!(
            SyscallError::InvalidAddress.category(),
            ErrorCategory::Memory
        );
        assert_eq!(
            SyscallError::NoSuchProcess.category(),
            ErrorCategory::Process
        );
        assert_eq!(
            SyscallError::FilesystemFull.category(),
            ErrorCategory::Filesystem
        );
        assert_eq!(
            SyscallError::NotImplemented.category(),
            ErrorCategory::System
        );
    }

    #[test]
    fn test_retriable_errors() {
        assert!(SyscallError::WouldBlock.is_retriable());
        assert!(SyscallError::Interrupted.is_retriable());
        assert!(SyscallError::Busy.is_retriable());
        assert!(!SyscallError::NotFound.is_retriable());
        assert!(!SyscallError::PermissionDenied.is_retriable());
    }

    #[test]
    fn test_error_size() {
        // Error should fit in u32 for efficient ABI crossing
        assert_eq!(
            core::mem::size_of::<SyscallError>(),
            core::mem::size_of::<u32>()
        );
    }
}
