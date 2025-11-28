//! Syscall Error Types (User Space)
//!
//! Type-safe error handling for system calls.

/// Error category for grouping related errors
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorCategory {
    /// No error
    None = 0,
    /// Permission/capability errors
    Permission = 1,
    /// Resource errors (not found, busy, etc.)
    Resource = 2,
    /// Memory errors
    Memory = 3,
    /// I/O errors
    Io = 4,
    /// Invalid argument errors
    Argument = 5,
    /// System errors
    System = 6,
}

/// System call error type
///
/// A type-safe replacement for errno-style error codes.
#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SyscallError {
    // Permission errors (0x01xx)
    /// Operation not permitted
    NotPermitted = 0x0100,
    /// Capability does not have required rights
    InsufficientRights = 0x0101,
    /// Access denied
    AccessDenied = 0x0102,

    // Resource errors (0x02xx)
    /// Resource not found
    NotFound = 0x0200,
    /// Resource already exists
    AlreadyExists = 0x0201,
    /// Resource is busy
    Busy = 0x0202,
    /// No such process
    NoProcess = 0x0203,
    /// Invalid handle/capability
    InvalidHandle = 0x0204,
    /// Handle generation mismatch (stale handle)
    StaleHandle = 0x0205,
    /// Wrong handle type
    WrongHandleType = 0x0206,
    /// Capability table is full
    TableFull = 0x0207,

    // Memory errors (0x03xx)
    /// Out of memory
    OutOfMemory = 0x0300,
    /// Invalid address
    BadAddress = 0x0301,
    /// Address not mapped
    NotMapped = 0x0302,
    /// Address already mapped
    AlreadyMapped = 0x0303,

    // I/O errors (0x04xx)
    /// I/O error
    IoError = 0x0400,
    /// Operation would block
    WouldBlock = 0x0401,
    /// Connection refused
    ConnectionRefused = 0x0402,
    /// Connection reset
    ConnectionReset = 0x0403,
    /// Pipe broken
    BrokenPipe = 0x0404,

    // Argument errors (0x05xx)
    /// Invalid argument
    InvalidArgument = 0x0500,
    /// Argument out of range
    OutOfRange = 0x0501,
    /// Buffer too small
    BufferTooSmall = 0x0502,
    /// Invalid syscall number
    InvalidSyscall = 0x0503,

    // System errors (0x06xx)
    /// Internal kernel error
    Internal = 0x0600,
    /// Operation not supported
    NotSupported = 0x0601,
    /// Operation interrupted
    Interrupted = 0x0602,
    /// Timeout
    Timeout = 0x0603,
}

impl SyscallError {
    /// Get the error category
    #[must_use]
    pub const fn category(&self) -> ErrorCategory {
        match (*self as u16) >> 8 {
            0x01 => ErrorCategory::Permission,
            0x02 => ErrorCategory::Resource,
            0x03 => ErrorCategory::Memory,
            0x04 => ErrorCategory::Io,
            0x05 => ErrorCategory::Argument,
            0x06 => ErrorCategory::System,
            _ => ErrorCategory::None,
        }
    }

    /// Get the error code as u16
    #[must_use]
    pub const fn code(&self) -> u16 {
        *self as u16
    }

    /// Convert from raw u16 value
    #[must_use]
    pub const fn from_u16(value: u16) -> Option<Self> {
        match value {
            0x0100 => Some(Self::NotPermitted),
            0x0101 => Some(Self::InsufficientRights),
            0x0102 => Some(Self::AccessDenied),
            0x0200 => Some(Self::NotFound),
            0x0201 => Some(Self::AlreadyExists),
            0x0202 => Some(Self::Busy),
            0x0203 => Some(Self::NoProcess),
            0x0204 => Some(Self::InvalidHandle),
            0x0205 => Some(Self::StaleHandle),
            0x0206 => Some(Self::WrongHandleType),
            0x0207 => Some(Self::TableFull),
            0x0300 => Some(Self::OutOfMemory),
            0x0301 => Some(Self::BadAddress),
            0x0302 => Some(Self::NotMapped),
            0x0303 => Some(Self::AlreadyMapped),
            0x0400 => Some(Self::IoError),
            0x0401 => Some(Self::WouldBlock),
            0x0402 => Some(Self::ConnectionRefused),
            0x0403 => Some(Self::ConnectionReset),
            0x0404 => Some(Self::BrokenPipe),
            0x0500 => Some(Self::InvalidArgument),
            0x0501 => Some(Self::OutOfRange),
            0x0502 => Some(Self::BufferTooSmall),
            0x0503 => Some(Self::InvalidSyscall),
            0x0600 => Some(Self::Internal),
            0x0601 => Some(Self::NotSupported),
            0x0602 => Some(Self::Interrupted),
            0x0603 => Some(Self::Timeout),
            _ => None,
        }
    }

    /// Convert to Linux-compatible errno (negative)
    #[must_use]
    pub const fn to_errno(&self) -> i64 {
        match self {
            Self::NotPermitted => -1,      // EPERM
            Self::InsufficientRights => -1, // EPERM
            Self::AccessDenied => -13,     // EACCES
            Self::NotFound => -2,          // ENOENT
            Self::AlreadyExists => -17,    // EEXIST
            Self::Busy => -16,             // EBUSY
            Self::NoProcess => -3,         // ESRCH
            Self::InvalidHandle => -9,     // EBADF
            Self::StaleHandle => -9,       // EBADF
            Self::WrongHandleType => -9,   // EBADF
            Self::TableFull => -24,        // EMFILE
            Self::OutOfMemory => -12,      // ENOMEM
            Self::BadAddress => -14,       // EFAULT
            Self::NotMapped => -14,        // EFAULT
            Self::AlreadyMapped => -17,    // EEXIST
            Self::IoError => -5,           // EIO
            Self::WouldBlock => -11,       // EAGAIN
            Self::ConnectionRefused => -111, // ECONNREFUSED
            Self::ConnectionReset => -104, // ECONNRESET
            Self::BrokenPipe => -32,       // EPIPE
            Self::InvalidArgument => -22,  // EINVAL
            Self::OutOfRange => -34,       // ERANGE
            Self::BufferTooSmall => -34,   // ERANGE
            Self::InvalidSyscall => -38,   // ENOSYS
            Self::Internal => -5,          // EIO
            Self::NotSupported => -38,     // ENOSYS
            Self::Interrupted => -4,       // EINTR
            Self::Timeout => -110,         // ETIMEDOUT
        }
    }
}

impl core::fmt::Display for SyscallError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NotPermitted => write!(f, "operation not permitted"),
            Self::InsufficientRights => write!(f, "insufficient capability rights"),
            Self::AccessDenied => write!(f, "access denied"),
            Self::NotFound => write!(f, "resource not found"),
            Self::AlreadyExists => write!(f, "resource already exists"),
            Self::Busy => write!(f, "resource busy"),
            Self::NoProcess => write!(f, "no such process"),
            Self::InvalidHandle => write!(f, "invalid handle"),
            Self::StaleHandle => write!(f, "stale handle (generation mismatch)"),
            Self::WrongHandleType => write!(f, "wrong handle type"),
            Self::TableFull => write!(f, "capability table full"),
            Self::OutOfMemory => write!(f, "out of memory"),
            Self::BadAddress => write!(f, "bad address"),
            Self::NotMapped => write!(f, "address not mapped"),
            Self::AlreadyMapped => write!(f, "address already mapped"),
            Self::IoError => write!(f, "I/O error"),
            Self::WouldBlock => write!(f, "operation would block"),
            Self::ConnectionRefused => write!(f, "connection refused"),
            Self::ConnectionReset => write!(f, "connection reset"),
            Self::BrokenPipe => write!(f, "broken pipe"),
            Self::InvalidArgument => write!(f, "invalid argument"),
            Self::OutOfRange => write!(f, "value out of range"),
            Self::BufferTooSmall => write!(f, "buffer too small"),
            Self::InvalidSyscall => write!(f, "invalid syscall"),
            Self::Internal => write!(f, "internal error"),
            Self::NotSupported => write!(f, "not supported"),
            Self::Interrupted => write!(f, "interrupted"),
            Self::Timeout => write!(f, "timeout"),
        }
    }
}
