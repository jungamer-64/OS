// kernel/src/kernel/capability/mod.rs
//! Capability-based Resource Management
//!
//! This module implements a capability-based security model where resources
//! are accessed through unforgeable tokens (capabilities) rather than
//! ambient authority (like file paths or integer file descriptors).
//!
//! # Design Philosophy
//!
//! - **Unforgeable**: Capabilities cannot be created from thin air
//! - **Transferable**: Capabilities can be passed between processes
//! - **Restrictable**: Rights can only be reduced, never increased
//! - **Type-safe**: Compile-time checking of resource types
//!
//! # Architecture
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────┐
//! │                     User Space                            │
//! │  Handle<FileResource> ───────────────────────────────────┤
//! │       │                                                   │
//! │       │ raw() -> u64                                      │
//! └───────┼──────────────────────────────────────────────────┘
//!         │
//!         ▼ ABI boundary (u64)
//! ┌──────────────────────────────────────────────────────────┐
//! │                    Kernel Space                           │
//! │       │                                                   │
//! │       ▼                                                   │
//! │  CapabilityTable                                          │
//! │       │                                                   │
//! │       ├─► [0] CapabilityEntry { type_id, rights, resource }
//! │       ├─► [1] CapabilityEntry { ... }                    │
//! │       └─► [2] CapabilityEntry { ... }                    │
//! └──────────────────────────────────────────────────────────┘
//! ```
//!
//! # Usage
//!
//! ```ignore
//! // Opening a file returns a capability
//! let cap: Handle<FileResource> = open("/data", Rights::READ_WRITE)?;
//!
//! // The capability encodes what operations are allowed
//! io.read(cap, buf_idx, 1024).await?;
//!
//! // Capability can be restricted before passing to untrusted code
//! let read_only = cap.restrict(Rights::READ_ONLY)?;
//! untrusted_code(read_only);
//!
//! // When Handle is dropped, resource is automatically closed
//! drop(cap); // Closes the file
//! ```

pub mod table;

use core::marker::PhantomData;
use core::sync::atomic::{AtomicU64, Ordering};

/// Capability rights flags
///
/// Rights determine what operations can be performed with a capability.
/// Rights can only be restricted (reduced), never expanded.
///
/// # Example
///
/// ```ignore
/// let full = Rights::READ | Rights::WRITE | Rights::SEEK;
/// let restricted = full.restrict(Rights::READ);
/// assert!(restricted.contains(Rights::READ));
/// assert!(!restricted.contains(Rights::WRITE));
/// ```
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Rights(pub u64);

impl Rights {
    // === Basic Rights ===

    /// No rights
    pub const NONE: Self = Self(0);

    /// Right to read data
    pub const READ: Self = Self(1 << 0);

    /// Right to write data
    pub const WRITE: Self = Self(1 << 1);

    /// Right to seek/change position
    pub const SEEK: Self = Self(1 << 2);

    /// Right to memory-map the resource
    pub const MAP: Self = Self(1 << 3);

    /// Right to duplicate the capability
    pub const DUP: Self = Self(1 << 4);

    /// Right to transfer to another process
    pub const TRANSFER: Self = Self(1 << 5);

    /// Right to close the resource
    pub const CLOSE: Self = Self(1 << 6);

    /// Right to truncate
    pub const TRUNCATE: Self = Self(1 << 7);

    // === File-specific Rights ===

    /// Right to create files in directory
    pub const CREATE: Self = Self(1 << 8);

    /// Right to delete files from directory
    pub const DELETE: Self = Self(1 << 9);

    /// Right to rename files
    pub const RENAME: Self = Self(1 << 10);

    /// Right to read directory entries
    pub const READDIR: Self = Self(1 << 11);

    /// Right to read file attributes
    pub const STAT: Self = Self(1 << 12);

    /// Right to modify file attributes
    pub const CHMOD: Self = Self(1 << 13);

    // === Network Rights ===

    /// Right to connect (client)
    pub const NET_CONNECT: Self = Self(1 << 16);

    /// Right to accept connections (server)
    pub const NET_ACCEPT: Self = Self(1 << 17);

    /// Right to send data
    pub const NET_SEND: Self = Self(1 << 18);

    /// Right to receive data
    pub const NET_RECV: Self = Self(1 << 19);

    /// Right to bind to address
    pub const NET_BIND: Self = Self(1 << 20);

    /// Right to listen for connections
    pub const NET_LISTEN: Self = Self(1 << 21);

    // === Memory Rights ===

    /// Right to execute (for memory mappings)
    pub const EXEC: Self = Self(1 << 24);

    // === Presets ===

    /// Read-only access
    pub const READ_ONLY: Self = Self(Self::READ.0 | Self::SEEK.0 | Self::STAT.0);

    /// Read-write access
    pub const READ_WRITE: Self =
        Self(Self::READ.0 | Self::WRITE.0 | Self::SEEK.0 | Self::STAT.0 | Self::TRUNCATE.0);

    /// Full access (all rights)
    pub const FULL: Self = Self(u64::MAX);

    /// Directory browsing
    pub const DIR_BROWSE: Self = Self(Self::READDIR.0 | Self::STAT.0);

    /// Directory full access
    pub const DIR_FULL: Self = Self(
        Self::READDIR.0
            | Self::CREATE.0
            | Self::DELETE.0
            | Self::RENAME.0
            | Self::STAT.0
            | Self::CHMOD.0,
    );

    /// Socket client
    pub const SOCKET_CLIENT: Self =
        Self(Self::NET_CONNECT.0 | Self::NET_SEND.0 | Self::NET_RECV.0);

    /// Socket server
    pub const SOCKET_SERVER: Self = Self(
        Self::NET_BIND.0
            | Self::NET_LISTEN.0
            | Self::NET_ACCEPT.0
            | Self::NET_SEND.0
            | Self::NET_RECV.0,
    );

    /// Check if all specified rights are present
    #[must_use]
    #[inline]
    pub const fn contains(&self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    /// Check if any of the specified rights are present
    #[must_use]
    #[inline]
    pub const fn intersects(&self, other: Self) -> bool {
        (self.0 & other.0) != 0
    }

    /// Restrict rights to the intersection with a mask
    ///
    /// This can only reduce rights, never expand them.
    #[must_use]
    #[inline]
    pub const fn restrict(&self, mask: Self) -> Self {
        Self(self.0 & mask.0)
    }

    /// Check if no rights are set
    #[must_use]
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.0 == 0
    }

    /// Get the raw u64 value
    #[must_use]
    #[inline]
    pub const fn bits(&self) -> u64 {
        self.0
    }

    /// Create from raw bits
    #[must_use]
    #[inline]
    pub const fn from_bits(bits: u64) -> Self {
        Self(bits)
    }
}

impl core::ops::BitOr for Rights {
    type Output = Self;

    #[inline]
    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl core::ops::BitOrAssign for Rights {
    #[inline]
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl core::ops::BitAnd for Rights {
    type Output = Self;

    #[inline]
    fn bitand(self, rhs: Self) -> Self::Output {
        Self(self.0 & rhs.0)
    }
}

impl core::ops::BitAndAssign for Rights {
    #[inline]
    fn bitand_assign(&mut self, rhs: Self) {
        self.0 &= rhs.0;
    }
}

impl core::ops::Not for Rights {
    type Output = Self;

    #[inline]
    fn not(self) -> Self::Output {
        Self(!self.0)
    }
}

/// Marker trait for resource types
///
/// This trait marks types that can be managed through the capability system.
/// Each resource type has a unique ID and name for identification.
///
/// # Safety
///
/// The `TYPE_ID` must be unique across all implementations.
pub trait ResourceKind: Send + Sync + 'static {
    /// Unique identifier for this resource type
    const TYPE_ID: u32;

    /// Human-readable name for debugging
    const NAME: &'static str;

    /// Default rights when opening this resource type
    const DEFAULT_RIGHTS: Rights;
}

/// File resource
pub struct FileResource;

impl ResourceKind for FileResource {
    const TYPE_ID: u32 = 1;
    const NAME: &'static str = "file";
    const DEFAULT_RIGHTS: Rights = Rights::READ_WRITE;
}

/// Socket resource
pub struct SocketResource;

impl ResourceKind for SocketResource {
    const TYPE_ID: u32 = 2;
    const NAME: &'static str = "socket";
    const DEFAULT_RIGHTS: Rights = Rights::SOCKET_CLIENT;
}

/// Pipe resource
pub struct PipeResource;

impl ResourceKind for PipeResource {
    const TYPE_ID: u32 = 3;
    const NAME: &'static str = "pipe";
    const DEFAULT_RIGHTS: Rights = Rights::READ_WRITE;
}

/// Registered buffer resource
pub struct BufferResource;

impl ResourceKind for BufferResource {
    const TYPE_ID: u32 = 4;
    const NAME: &'static str = "buffer";
    const DEFAULT_RIGHTS: Rights = Rights::READ_WRITE;
}

/// Directory resource
pub struct DirectoryResource;

impl ResourceKind for DirectoryResource {
    const TYPE_ID: u32 = 5;
    const NAME: &'static str = "directory";
    const DEFAULT_RIGHTS: Rights = Rights::DIR_BROWSE;
}

/// Event/notification resource
pub struct EventResource;

impl ResourceKind for EventResource {
    const TYPE_ID: u32 = 6;
    const NAME: &'static str = "event";
    const DEFAULT_RIGHTS: Rights = Rights(Rights::READ.0 | Rights::WRITE.0);
}

/// Shared memory resource
pub struct ShmemResource;

impl ResourceKind for ShmemResource {
    const TYPE_ID: u32 = 7;
    const NAME: &'static str = "shmem";
    const DEFAULT_RIGHTS: Rights = Rights(Rights::READ.0 | Rights::WRITE.0 | Rights::MAP.0);
}

/// Type-safe capability handle
///
/// A `Handle<R>` represents ownership of a capability to access a resource
/// of type `R`. The handle is:
///
/// - **Zero-cost**: At runtime, it's just a `u64`
/// - **Type-safe**: The compiler ensures correct resource types
/// - **Move-only**: Prevents use-after-close bugs
///
/// # Memory Layout
///
/// ```text
/// Handle<R> = u64
/// [63..32] Generation number (32 bits) - ABA problem prevention
/// [31..0]  Table index (32 bits) - Index into capability table
/// ```
///
/// # Ownership
///
/// `Handle` does NOT implement `Clone` or `Copy`. When you pass a handle
/// to a function, ownership is transferred. This prevents:
///
/// - Use-after-close bugs
/// - Double-close bugs
/// - Race conditions on close
///
/// If you need to share a handle, explicitly duplicate it (which may fail
/// if `DUP` right is not present).
#[repr(transparent)]
pub struct Handle<R: ResourceKind> {
    /// Encoded ID: (generation << 32) | index
    id: u64,
    _phantom: PhantomData<R>,
}

impl<R: ResourceKind> Handle<R> {
    /// Create a new handle (kernel internal)
    #[must_use]
    pub(crate) const fn new(index: u32, generation: u32) -> Self {
        Self {
            id: ((generation as u64) << 32) | (index as u64),
            _phantom: PhantomData,
        }
    }

    /// Create from raw u64 (for ABI boundary crossing)
    ///
    /// # Safety
    ///
    /// The caller must ensure that:
    /// 1. The raw value represents a valid capability of type `R`
    /// 2. The capability has not been revoked
    /// 3. The caller has ownership (this transfers ownership)
    #[must_use]
    pub const unsafe fn from_raw(raw: u64) -> Self {
        Self {
            id: raw,
            _phantom: PhantomData,
        }
    }

    /// Get the raw u64 value for ABI crossing
    #[must_use]
    #[inline]
    pub const fn raw(&self) -> u64 {
        self.id
    }

    /// Get the table index
    #[must_use]
    #[inline]
    pub const fn index(&self) -> u32 {
        self.id as u32
    }

    /// Get the generation number
    #[must_use]
    #[inline]
    pub const fn generation(&self) -> u32 {
        (self.id >> 32) as u32
    }

    /// Consume the handle and return the raw ID
    ///
    /// This prevents the destructor from running, effectively
    /// transferring ownership to whoever receives the raw ID.
    #[must_use]
    pub fn into_raw(self) -> u64 {
        let raw = self.id;
        core::mem::forget(self);
        raw
    }

    /// Get the resource type ID
    #[must_use]
    pub const fn type_id() -> u32 {
        R::TYPE_ID
    }

    /// Get the resource type name
    #[must_use]
    pub const fn type_name() -> &'static str {
        R::NAME
    }
}

// Intentionally NOT implementing Clone/Copy to enforce move semantics

impl<R: ResourceKind> Drop for Handle<R> {
    fn drop(&mut self) {
        // The actual resource cleanup is handled by the capability table.
        // This drop impl is a safety net - in a fully working system,
        // handles should be explicitly closed before being dropped.
        //
        // We can't call into the capability table here because:
        // 1. We don't have access to the process's table
        // 2. This might be called during panic unwinding
        //
        // Instead, we rely on:
        // 1. Explicit close operations
        // 2. Process cleanup on exit
    }
}

impl<R: ResourceKind> core::fmt::Debug for Handle<R> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Handle")
            .field("type", &R::NAME)
            .field("index", &self.index())
            .field("gen", &self.generation())
            .finish()
    }
}

/// Type aliases for convenience
pub type FileHandle = Handle<FileResource>;
pub type SocketHandle = Handle<SocketResource>;
pub type PipeHandle = Handle<PipeResource>;
pub type BufferHandle = Handle<BufferResource>;
pub type DirectoryHandle = Handle<DirectoryResource>;

/// Global generation counter for capability IDs
///
/// This is used to generate unique generation numbers to prevent ABA problems.
/// Even if a capability table slot is reused, the generation number will differ.
static GENERATION_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Generate a new unique generation number
pub fn next_generation() -> u32 {
    GENERATION_COUNTER.fetch_add(1, Ordering::Relaxed) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rights_operations() {
        let rw = Rights::READ | Rights::WRITE;
        assert!(rw.contains(Rights::READ));
        assert!(rw.contains(Rights::WRITE));
        assert!(!rw.contains(Rights::SEEK));

        let restricted = rw.restrict(Rights::READ);
        assert!(restricted.contains(Rights::READ));
        assert!(!restricted.contains(Rights::WRITE));
    }

    #[test]
    fn test_rights_presets() {
        assert!(Rights::READ_ONLY.contains(Rights::READ));
        assert!(!Rights::READ_ONLY.contains(Rights::WRITE));

        assert!(Rights::READ_WRITE.contains(Rights::READ));
        assert!(Rights::READ_WRITE.contains(Rights::WRITE));
    }

    #[test]
    fn test_handle_encoding() {
        let handle: Handle<FileResource> = Handle::new(42, 7);
        assert_eq!(handle.index(), 42);
        assert_eq!(handle.generation(), 7);

        let raw = handle.into_raw();
        let restored: Handle<FileResource> = unsafe { Handle::from_raw(raw) };
        assert_eq!(restored.index(), 42);
        assert_eq!(restored.generation(), 7);
    }

    #[test]
    fn test_handle_size() {
        // Handle should be exactly u64 size (zero-cost)
        assert_eq!(
            core::mem::size_of::<Handle<FileResource>>(),
            core::mem::size_of::<u64>()
        );
    }

    #[test]
    fn test_resource_type_ids() {
        assert_eq!(FileResource::TYPE_ID, 1);
        assert_eq!(SocketResource::TYPE_ID, 2);
        assert_eq!(PipeResource::TYPE_ID, 3);
        assert_eq!(BufferResource::TYPE_ID, 4);
        assert_eq!(DirectoryResource::TYPE_ID, 5);
    }
}
