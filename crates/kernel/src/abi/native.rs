// kernel/src/abi/native.rs
//! Rust-Native ABI Definitions
//!
//! This module defines Rust-native ABI types that do NOT use `repr(C)`.
//! These types are designed for maximum efficiency and Rust idiomacy,
//! completely abandoning C compatibility.
//!
//! # Design Philosophy
//!
//! - **No C compatibility**: We use Rust's native representations
//! - **Type safety first**: All types are strongly typed
//! - **Zero-cost abstractions**: No runtime overhead for type safety
//! - **Move semantics**: Resources are automatically cleaned up
//!
//! # Memory Safety
//!
//! All types in this module are designed to be memory-safe when used
//! correctly. The type system enforces correct usage patterns.

use core::marker::PhantomData;

/// Syscall number enumeration
///
/// Type-safe syscall numbers that replace raw integers.
/// The compiler will catch invalid syscall numbers at compile time.
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

    /// Get the category of this syscall
    #[must_use]
    pub const fn category(&self) -> SyscallCategory {
        match *self as u16 >> 8 {
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
/// This is a strongly-typed wrapper around `u64` that encodes both
/// the resource index and a generation number for ABA problem prevention.
///
/// # Memory Layout
/// ```text
/// [63..32] Generation number (32 bits)
/// [31..0]  Resource index (32 bits)
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ResourceId(u64);

impl ResourceId {
    /// Maximum number of resources (2^32)
    pub const MAX_RESOURCES: u64 = 1 << 32;

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
/// # Type Safety
///
/// The phantom type parameter `R` ensures that a `Handle<FileMarker>`
/// cannot be used where a `Handle<SocketMarker>` is expected, even though
/// both are represented as `u64` at runtime.
///
/// # Ownership
///
/// `Handle` does NOT implement `Clone` or `Copy`. This enforces
/// move semantics: when a handle is passed to a function, ownership
/// is transferred. This prevents use-after-close bugs.
///
/// # Example
/// ```ignore
/// let file: Handle<FileMarker> = open("/data", Rights::READ)?;
/// let _ = read(file, buf); // `file` is moved here
/// // read(file, buf); // ERROR: use of moved value
/// ```
#[repr(transparent)]
pub struct Handle<R: ResourceMarker> {
    id: ResourceId,
    _marker: PhantomData<R>,
}

impl<R: ResourceMarker> Handle<R> {
    /// Create a new handle (kernel-internal)
    #[must_use]
    pub(crate) const fn new(id: ResourceId) -> Self {
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
    /// This "forgets" the handle without dropping it, transferring
    /// ownership responsibility to the caller.
    #[must_use]
    pub fn into_raw(self) -> u64 {
        let raw = self.id.raw();
        core::mem::forget(self);
        raw
    }
}

// Intentionally NOT implementing Clone/Copy to enforce move semantics

impl<R: ResourceMarker> Drop for Handle<R> {
    fn drop(&mut self) {
        // In a real implementation, this would close the resource
        // For now, just log it in debug builds
        #[cfg(debug_assertions)]
        {
            // Note: Can't use debug_println! here as it might cause issues
            // during panic unwinding. The actual close will be done via
            // the capability table's Drop implementation.
        }
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

/// Type aliases for common handle types
pub type FileHandle = Handle<FileMarker>;
pub type SocketHandle = Handle<SocketMarker>;
pub type PipeHandle = Handle<PipeMarker>;
pub type BufferHandle = Handle<BufferMarker>;
pub type DirectoryHandle = Handle<DirectoryMarker>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_id() {
        let id = ResourceId::new(42, 7);
        assert_eq!(id.index(), 42);
        assert_eq!(id.generation(), 7);

        let raw = id.raw();
        let restored = ResourceId::from_raw(raw);
        assert_eq!(restored.index(), 42);
        assert_eq!(restored.generation(), 7);
    }

    #[test]
    fn test_syscall_category() {
        assert_eq!(
            SyscallNumber::IoUringSetup.category(),
            SyscallCategory::IoUring
        );
        assert_eq!(SyscallNumber::CapOpen.category(), SyscallCategory::Capability);
        assert_eq!(SyscallNumber::Mmap.category(), SyscallCategory::Memory);
        assert_eq!(SyscallNumber::Exit.category(), SyscallCategory::Process);
        assert_eq!(
            SyscallNumber::DebugPrint.category(),
            SyscallCategory::Debug
        );
    }

    #[test]
    fn test_handle_size() {
        // Handle should be zero-cost: same size as u64
        assert_eq!(
            core::mem::size_of::<Handle<FileMarker>>(),
            core::mem::size_of::<u64>()
        );
    }
}
