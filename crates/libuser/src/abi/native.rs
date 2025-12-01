//! Rust-Native ABI Definitions (User Space)
//!
//! This module defines Rust-native ABI types for user-space programs.
//! These types mirror the kernel's `abi::native` module.

use core::marker::PhantomData;

/// Syscall number enumeration
///
/// Type-safe syscall numbers that replace raw integers.
#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SyscallNumber {
    // Ring setup (0x00xx)
    /// Setup io_uring instance
    IoUringSetup = 0x0001,
    /// Enter io_uring (submit and/or wait)
    IoUringEnter = 0x0002,
    /// Register buffers/files with io_uring
    IoUringRegister = 0x0003,

    // Capability management (0x01xx)
    /// Open a resource and get a capability
    CapOpen = 0x0100,
    /// Close/drop a capability
    CapClose = 0x0101,
    /// Duplicate a capability (with optional rights restriction)
    CapDup = 0x0102,
    /// Transfer a capability to another process
    CapTransfer = 0x0103,
    /// Query capability rights
    CapQuery = 0x0104,
    /// Restrict capability rights
    CapRestrict = 0x0105,

    // Memory management (0x02xx)
    /// Map memory
    Mmap = 0x0200,
    /// Unmap memory
    Munmap = 0x0201,
    /// Protect memory region
    Mprotect = 0x0202,

    // Process management (0x03xx)
    /// Exit current process
    Exit = 0x0300,
    /// Execute program
    Exec = 0x0302,
    /// Wait for child process
    Wait = 0x0303,
    /// Get process ID
    GetPid = 0x0304,
    /// Yield to scheduler
    Yield = 0x0305,

    // Debug (0xFFxx)
    /// Debug print
    DebugPrint = 0xFF00,
    /// Debug log level
    DebugSetLevel = 0xFF01,
}

impl SyscallNumber {
    /// Convert from raw u16 value
    #[must_use]
    pub const fn from_u16(value: u16) -> Option<Self> {
        match value {
            0x0001 => Some(Self::IoUringSetup),
            0x0002 => Some(Self::IoUringEnter),
            0x0003 => Some(Self::IoUringRegister),
            0x0100 => Some(Self::CapOpen),
            0x0101 => Some(Self::CapClose),
            0x0102 => Some(Self::CapDup),
            0x0103 => Some(Self::CapTransfer),
            0x0104 => Some(Self::CapQuery),
            0x0105 => Some(Self::CapRestrict),
            0x0200 => Some(Self::Mmap),
            0x0201 => Some(Self::Munmap),
            0x0202 => Some(Self::Mprotect),
            0x0300 => Some(Self::Exit),
            0x0302 => Some(Self::Exec),
            0x0303 => Some(Self::Wait),
            0x0304 => Some(Self::GetPid),
            0x0305 => Some(Self::Yield),
            0xFF00 => Some(Self::DebugPrint),
            0xFF01 => Some(Self::DebugSetLevel),
            _ => None,
        }
    }

    /// Convert to raw u16 value
    #[must_use]
    pub const fn as_u16(self) -> u16 {
        self as u16
    }

    /// Get the category of this syscall
    #[must_use]
    pub const fn category(&self) -> SyscallCategory {
        match (*self as u16) >> 8 {
            0x00 => SyscallCategory::IoUring,
            0x01 => SyscallCategory::Capability,
            0x02 => SyscallCategory::Memory,
            0x03 => SyscallCategory::Process,
            0xFF => SyscallCategory::Debug,
            _ => SyscallCategory::Unknown,
        }
    }
}

/// Syscall category for grouping related calls
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyscallCategory {
    /// io_uring operations
    IoUring,
    /// Capability management
    Capability,
    /// Memory management
    Memory,
    /// Process management
    Process,
    /// Debug operations
    Debug,
    /// Unknown category
    Unknown,
}

/// Type-safe resource identifier
///
/// Encodes both the resource index and a generation number for ABA prevention.
///
/// # Memory Layout
/// ```text
/// [63..32] Generation number (32 bits)
/// [31..0]  Resource index (32 bits)
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ResourceId(u64);

impl ResourceId {
    /// Create a new resource ID from index and generation
    #[must_use]
    pub const fn new(index: u32, generation: u32) -> Self {
        Self(((generation as u64) << 32) | (index as u64))
    }

    /// Get the resource index
    #[must_use]
    pub const fn index(&self) -> u32 {
        self.0 as u32
    }

    /// Get the generation number
    #[must_use]
    pub const fn generation(&self) -> u32 {
        (self.0 >> 32) as u32
    }

    /// Get the raw u64 value
    #[must_use]
    pub const fn raw(&self) -> u64 {
        self.0
    }

    /// Create from raw u64 value
    #[must_use]
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    /// Invalid/null resource ID
    pub const INVALID: Self = Self(u64::MAX);
}

/// Zero-cost type marker for resource kinds
pub trait ResourceMarker: Send + Sync + 'static {
    /// Unique type identifier
    const TYPE_ID: u32;
    /// Human-readable name for debugging
    const NAME: &'static str;
}

/// File resource marker
pub struct FileMarker;
impl ResourceMarker for FileMarker {
    const TYPE_ID: u32 = 1;
    const NAME: &'static str = "file";
}

/// Socket resource marker
pub struct SocketMarker;
impl ResourceMarker for SocketMarker {
    const TYPE_ID: u32 = 2;
    const NAME: &'static str = "socket";
}

/// Pipe resource marker
pub struct PipeMarker;
impl ResourceMarker for PipeMarker {
    const TYPE_ID: u32 = 3;
    const NAME: &'static str = "pipe";
}

/// Registered buffer resource marker
pub struct BufferMarker;
impl ResourceMarker for BufferMarker {
    const TYPE_ID: u32 = 4;
    const NAME: &'static str = "buffer";
}

/// Directory resource marker
pub struct DirectoryMarker;
impl ResourceMarker for DirectoryMarker {
    const TYPE_ID: u32 = 5;
    const NAME: &'static str = "directory";
}

/// Type-safe capability handle
///
/// A zero-cost abstraction over `ResourceId` that adds compile-time
/// type checking for resource kinds.
///
/// # Ownership
///
/// `Handle` does NOT implement `Clone` or `Copy`. This enforces
/// move semantics: when a handle is passed to a function, ownership
/// is transferred.
#[repr(transparent)]
pub struct Handle<R: ResourceMarker> {
    id: ResourceId,
    _marker: PhantomData<R>,
}

impl<R: ResourceMarker> Handle<R> {
    /// Create a new handle from resource ID
    #[must_use]
    pub const fn new(id: ResourceId) -> Self {
        Self {
            id,
            _marker: PhantomData,
        }
    }

    /// Get the raw resource ID
    #[must_use]
    pub const fn id(&self) -> ResourceId {
        self.id
    }

    /// Get the raw u64 value for ABI crossing
    #[must_use]
    pub const fn raw(&self) -> u64 {
        self.id.raw()
    }

    /// Create from raw u64 value
    ///
    /// # Safety
    ///
    /// The caller must ensure that:
    /// 1. The raw value represents a valid resource of type `R`
    /// 2. The resource has not been closed
    /// 3. The caller has ownership of the resource
    #[must_use]
    pub const unsafe fn from_raw(raw: u64) -> Self {
        Self::new(ResourceId::from_raw(raw))
    }

    /// Consume this handle and return the raw ID
    ///
    /// This "forgets" the handle without dropping it.
    #[must_use]
    pub fn into_raw(self) -> u64 {
        let raw = self.id.raw();
        core::mem::forget(self);
        raw
    }

    /// Borrow this handle without consuming it
    ///
    /// Returns the raw ID for use in syscalls while keeping ownership.
    #[must_use]
    pub const fn as_raw(&self) -> u64 {
        self.id.raw()
    }
}

// Intentionally NOT implementing Clone/Copy to enforce move semantics

impl<R: ResourceMarker> Drop for Handle<R> {
    fn drop(&mut self) {
        // In user space, we should close the capability when dropped
        // For now, this is a no-op; explicit close is required
        // TODO: Implement automatic close via syscall
    }
}

impl<R: ResourceMarker> core::fmt::Debug for Handle<R> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Handle")
            .field("type", &R::NAME)
            .field("index", &self.id.index())
            .field("generation", &self.id.generation())
            .finish()
    }
}

/// File handle type alias
pub type FileHandle = Handle<FileMarker>;
/// Socket handle type alias
pub type SocketHandle = Handle<SocketMarker>;
/// Pipe handle type alias
pub type PipeHandle = Handle<PipeMarker>;
/// Buffer handle type alias
pub type BufferHandle = Handle<BufferMarker>;
/// Directory handle type alias
pub type DirectoryHandle = Handle<DirectoryMarker>;

/// Standard capability IDs for stdin/stdout/stderr
pub mod stdio {
    use super::{FileHandle, ResourceId};

    /// Standard input capability ID (index=0, generation=0)
    pub const STDIN_ID: u64 = 0;
    /// Standard output capability ID (index=1, generation=0)  
    pub const STDOUT_ID: u64 = 1;
    /// Standard error capability ID (index=2, generation=0)
    pub const STDERR_ID: u64 = 2;

    /// Get stdin handle
    ///
    /// # Safety
    /// 
    /// The caller must ensure the process has stdin capability.
    #[must_use]
    pub const unsafe fn stdin() -> FileHandle {
        FileHandle::new(ResourceId::from_raw(STDIN_ID))
    }

    /// Get stdout handle
    ///
    /// # Safety
    ///
    /// The caller must ensure the process has stdout capability.
    #[must_use]
    pub const unsafe fn stdout() -> FileHandle {
        FileHandle::new(ResourceId::from_raw(STDOUT_ID))
    }

    /// Get stderr handle
    ///
    /// # Safety
    ///
    /// The caller must ensure the process has stderr capability.
    #[must_use]
    pub const unsafe fn stderr() -> FileHandle {
        FileHandle::new(ResourceId::from_raw(STDERR_ID))
    }
}
