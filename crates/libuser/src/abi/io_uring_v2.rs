// kernel/src/abi/io_uring_v2.rs
//! Next-Generation io_uring ABI
//!
//! This module defines the V2 io_uring ABI that uses capabilities instead
//! of file descriptors and returns typed Results instead of errno.
//!
//! # Changes from V1
//!
//! | Aspect | V1 | V2 |
//! |--------|----|----|
//! | Resource ID | `fd: i32` | `capability_id: u64` |
//! | Result | `-errno` | `AbiResult` |
//! | Buffer | `addr: u64` | `buf_index: u32` (registered only) |
//! | Alignment | 64 bytes | 64 bytes |
//!
//! # Memory Layout
//!
//! ```text
//! SubmissionEntryV2 (64 bytes, repr(C), align(64)):
//! +0   opcode (1)
//! +1   flags (1)
//! +2   ioprio (2)
//! +4   [padding] (4)        <- implicit padding for u64 alignment
//! +8   capability_id (8)
//! +16  off (8)
//! +24  buf_index (4)
//! +28  len (4)
//! +32  op_flags (4)
//! +36  _pad (4)             <- explicit padding
//! +40  user_data (8)
//! +48  aux1 (8)
//! +56  aux2 (8)
//! = 64 bytes
//!
//! CompletionEntryV2 (32 bytes, repr(C), align(32)):
//! +0   user_data (8)
//! +8   result_tag (4)
//! +12  result_value (4)
//! +16  error_code (4)
//! +20  flags (4)
//! +24  aux (8)
//! = 32 bytes
//! ```

use core::sync::atomic::{AtomicU32, Ordering};

use super::error::SyscallError;
use super::result::AbiResult;
use super::io_uring_common::{IoUringFlags, OpCode, RING_MASK, RING_SIZE};

/// V2 Submission Queue Entry
///
/// A capability-based submission entry for io_uring operations.
/// Uses capability IDs instead of file descriptors.
#[repr(C, align(64))]
#[derive(Debug, Clone, Copy)]
pub struct SubmissionEntryV2 {
    /// Operation code (see `OpCode`)
    pub opcode: u8,

    /// Flags for this operation (see `IoUringFlags`)
    pub flags: u8,

    /// I/O priority (lower = higher priority)
    pub ioprio: u16,

    /// Capability ID (replaces file descriptor)
    ///
    /// This is the raw value from `Handle::raw()`.
    /// The kernel will validate this against the process's capability table.
    pub capability_id: u64,

    /// Offset in file, or address hint for mmap
    pub off: u64,

    /// Registered buffer index (replaces raw address)
    ///
    /// For operations that need a buffer, this is an index into
    /// the process's registered buffer table. This enables:
    /// - Zero-copy I/O
    /// - No pointer validation needed per-operation
    /// - DMA-friendly memory regions
    pub buf_index: u32,

    /// Length of the operation
    pub len: u32,

    /// Operation-specific flags
    pub op_flags: u32,

    /// Padding to maintain 8-byte alignment for user_data
    pub _pad: u32,

    /// User data - passed back in completion
    pub user_data: u64,

    /// Auxiliary field 1 (operation-specific)
    /// - For splice: source capability ID
    /// - For accept: flags
    /// - For timeout: timeout in nanoseconds (low 64 bits)
    pub aux1: u64,

    /// Auxiliary field 2 (operation-specific)
    /// - For splice: source offset
    /// - For timeout: timeout in nanoseconds (high 64 bits)
    pub aux2: u64,
    // Note: No _reserved field - struct is exactly 64 bytes with implicit padding after ioprio
}

// Compile-time size check
const _: () = assert!(
    core::mem::size_of::<SubmissionEntryV2>() == 64,
    "SubmissionEntryV2 must be 64 bytes"
);

impl SubmissionEntryV2 {
    /// Create a NOP entry
    #[must_use]
    pub const fn nop(user_data: u64) -> Self {
        Self {
            opcode: OpCode::Nop as u8,
            flags: 0,
            ioprio: 0,
            capability_id: 0,
            off: 0,
            buf_index: 0,
            len: 0,
            op_flags: 0,
            _pad: 0,
            user_data,
            aux1: 0,
            aux2: 0,
        }
    }

    /// Create a read entry using registered buffer
    #[must_use]
    pub const fn read(
        capability_id: u64,
        buf_index: u32,
        len: u32,
        offset: u64,
        user_data: u64,
    ) -> Self {
        Self {
            opcode: OpCode::Read as u8,
            flags: IoUringFlags::FIXED_BUFFER.0,
            ioprio: 0,
            capability_id,
            off: offset,
            buf_index,
            len,
            op_flags: 0,
            _pad: 0,
            user_data,
            aux1: 0,
            aux2: 0,
        }
    }

    /// Create a write entry using registered buffer
    #[must_use]
    pub const fn write(
        capability_id: u64,
        buf_index: u32,
        len: u32,
        offset: u64,
        user_data: u64,
    ) -> Self {
        Self {
            opcode: OpCode::Write as u8,
            flags: IoUringFlags::FIXED_BUFFER.0,
            ioprio: 0,
            capability_id,
            off: offset,
            buf_index,
            len,
            op_flags: 0,
            _pad: 0,
            user_data,
            aux1: 0,
            aux2: 0,
        }
    }

    /// Create a raw read entry (kernel use only)
    /// Uses aux1 as buffer address
    #[must_use]
    pub const fn read_raw(
        capability_id: u64,
        addr: u64,
        len: u32,
        offset: u64,
        user_data: u64,
    ) -> Self {
        Self {
            opcode: OpCode::Read as u8,
            flags: 0, // No FIXED_BUFFER
            ioprio: 0,
            capability_id,
            off: offset,
            buf_index: 0,
            len,
            op_flags: 0,
            _pad: 0,
            user_data,
            aux1: addr, // Use aux1 for address
            aux2: 0,
        }
    }

    /// Create a raw write entry (kernel use only)
    /// Uses aux1 as buffer address
    #[must_use]
    pub const fn write_raw(
        capability_id: u64,
        addr: u64,
        len: u32,
        offset: u64,
        user_data: u64,
    ) -> Self {
        Self {
            opcode: OpCode::Write as u8,
            flags: 0, // No FIXED_BUFFER
            ioprio: 0,
            capability_id,
            off: offset,
            buf_index: 0,
            len,
            op_flags: 0,
            _pad: 0,
            user_data,
            aux1: addr, // Use aux1 for address
            aux2: 0,
        }
    }

    /// Create a close entry
    #[must_use]
    pub const fn close(capability_id: u64, user_data: u64) -> Self {
        Self {
            opcode: OpCode::Close as u8,
            flags: 0,
            ioprio: 0,
            capability_id,
            off: 0,
            buf_index: 0,
            len: 0,
            op_flags: 0,
            _pad: 0,
            user_data,
            aux1: 0,
            aux2: 0,
        }
    }

    /// Create an mmap entry
    #[must_use]
    pub const fn mmap(addr_hint: u64, len: u32, user_data: u64) -> Self {
        Self {
            opcode: OpCode::Mmap as u8,
            flags: 0,
            ioprio: 0,
            capability_id: 0,
            off: addr_hint,
            buf_index: 0,
            len,
            op_flags: 0,
            _pad: 0,
            user_data,
            aux1: 0,
            aux2: 0,
        }
    }

    /// Get the operation code
    #[must_use]
    pub const fn op(&self) -> Option<OpCode> {
        OpCode::from_u8(self.opcode)
    }

    /// Check if this uses a fixed buffer
    #[must_use]
    pub const fn uses_fixed_buffer(&self) -> bool {
        (self.flags & IoUringFlags::FIXED_BUFFER.0) != 0
    }
}

impl Default for SubmissionEntryV2 {
    fn default() -> Self {
        Self::nop(0)
    }
}


/// V2 Completion Queue Entry
///
/// A completion entry that uses typed results instead of errno.
/// 
/// This structure uses `AbiResult` to provide type-safe, ABI-stable
/// result passing consistent with the rest of the V2 protocol.
///
/// # Memory Layout
/// - user_data: 8 bytes
/// - result (AbiResult): 16 bytes (tag + padding + data)
/// - flags: 4 bytes
/// - aux: 8 bytes
/// - _pad: 4 bytes
/// Total: 40 bytes
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CompletionEntryV2 {
    /// User data from the corresponding SQE
    pub user_data: u64,

    /// Operation result (Ok with value, or Err with error code)
    pub result: AbiResult<i32, SyscallError>,

    /// Completion flags
    pub flags: u32,

    /// Auxiliary data (operation-specific)
    /// - For accept: peer address info
    /// - For recv: message flags
    pub aux: u64,

    /// Padding to maintain alignment
    _pad: u32,
}

// Compile-time size check
const _: () = assert!(
    core::mem::size_of::<CompletionEntryV2>() == 40,
    "CompletionEntryV2 must be 40 bytes"
);

impl CompletionEntryV2 {
    /// Create a successful completion
    #[must_use]
    #[inline]
    pub fn success(user_data: u64, value: i32) -> Self {
        Self {
            user_data,
            result: AbiResult::ok(value),
            flags: 0,
            aux: 0,
            _pad: 0,
        }
    }

    /// Create a successful completion with auxiliary data
    #[must_use]
    #[inline]
    pub fn success_with_aux(user_data: u64, value: i32, aux: u64) -> Self {
        Self {
            user_data,
            result: AbiResult::ok(value),
            flags: 0,
            aux,
            _pad: 0,
        }
    }

    /// Create an error completion
    #[must_use]
    #[inline]
    pub fn error(user_data: u64, err: SyscallError) -> Self {
        Self {
            user_data,
            result: AbiResult::err(err),
            flags: 0,
            aux: 0,
            _pad: 0,
        }
    }

    /// Check if this is a success
    #[must_use]
    #[inline]
    pub fn is_ok(&self) -> bool {
        self.result.is_ok()
    }

    /// Check if this is an error
    #[must_use]
    #[inline]
    pub fn is_err(&self) -> bool {
        self.result.is_err()
    }

    /// Get the result as a Rust Result
    #[must_use]
    #[inline]
    pub fn into_result(self) -> Result<i32, SyscallError> {
        self.result.into_result()
    }

    /// Get the error, if any
    #[must_use]
    #[inline]
    pub fn get_error(&self) -> Option<SyscallError> {
        self.result.err_value()
    }
}

impl Default for CompletionEntryV2 {
    fn default() -> Self {
        Self::success(0, 0)
    }
}

impl From<CompletionEntryV2> for Result<i32, SyscallError> {
    fn from(cqe: CompletionEntryV2) -> Self {
        cqe.into_result()
    }
}

/// V2 Ring Header
///
/// Extended header with additional fields for the V2 protocol.
#[repr(C, align(64))]
#[derive(Debug)]
pub struct RingHeaderV2 {
    /// Consumer index
    pub head: AtomicU32,

    /// Producer index
    pub tail: AtomicU32,

    /// Ring mask
    pub ring_mask: u32,

    /// Number of entries
    pub ring_entries: u32,

    /// Flags
    pub flags: AtomicU32,

    /// Dropped count
    pub dropped: AtomicU32,

    /// Feature flags (what V2 features are enabled)
    pub features: u32,

    /// Padding
    _pad: [u32; 9],
}

// Compile-time size check
const _: () = assert!(
    core::mem::size_of::<RingHeaderV2>() == 64,
    "RingHeaderV2 must be 64 bytes"
);

/// V2 Feature flags
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct V2Features(pub u32);

impl V2Features {
    /// No features enabled
    pub const NONE: Self = Self(0);

    /// Capability-based resource IDs
    pub const CAPABILITY: Self = Self(1 << 0);

    /// Registered buffers only (no raw addresses)
    pub const FIXED_BUFFERS_ONLY: Self = Self(1 << 1);

    /// Typed results in CQE
    pub const TYPED_RESULT: Self = Self(1 << 2);

    /// SQPOLL enabled
    pub const SQPOLL: Self = Self(1 << 3);

    /// All V2 features
    pub const ALL_V2: Self = Self(
        Self::CAPABILITY.0 | Self::FIXED_BUFFERS_ONLY.0 | Self::TYPED_RESULT.0,
    );

    /// Check if a feature is enabled
    #[must_use]
    pub const fn has(&self, feature: Self) -> bool {
        (self.0 & feature.0) == feature.0
    }
}

impl RingHeaderV2 {
    /// Create a new V2 ring header
    #[must_use]
    pub const fn new(features: V2Features) -> Self {
        Self {
            head: AtomicU32::new(0),
            tail: AtomicU32::new(0),
            ring_mask: RING_MASK,
            ring_entries: RING_SIZE,
            flags: AtomicU32::new(0),
            dropped: AtomicU32::new(0),
            features: features.0,
            _pad: [0; 9],
        }
    }

    /// Get pending count
    #[must_use]
    pub fn pending_count(&self) -> u32 {
        let tail = self.tail.load(Ordering::Acquire);
        let head = self.head.load(Ordering::Acquire);
        tail.wrapping_sub(head)
    }

    /// Get available count
    #[must_use]
    pub fn available_count(&self) -> u32 {
        self.ring_entries - self.pending_count()
    }

    /// Check if empty
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pending_count() == 0
    }

    /// Check if full
    #[must_use]
    pub fn is_full(&self) -> bool {
        self.pending_count() >= self.ring_entries
    }

    /// Advance head
    pub fn advance_head(&self, count: u32) {
        let old = self.head.load(Ordering::Relaxed);
        self.head.store(old.wrapping_add(count), Ordering::Release);
    }

    /// Advance tail
    pub fn advance_tail(&self, count: u32) {
        let old = self.tail.load(Ordering::Relaxed);
        self.tail.store(old.wrapping_add(count), Ordering::Release);
    }

    /// Get features
    #[must_use]
    pub const fn features(&self) -> V2Features {
        V2Features(self.features)
    }
}

impl Default for RingHeaderV2 {
    fn default() -> Self {
        Self::new(V2Features::ALL_V2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sqe_v2_creation() {
        let sqe = SubmissionEntryV2::read(0x12345678, 0, 1024, 0, 42);

        assert_eq!(sqe.op(), Some(OpCode::Read));
        assert_eq!(sqe.capability_id, 0x12345678);
        assert_eq!(sqe.buf_index, 0);
        assert_eq!(sqe.len, 1024);
        assert_eq!(sqe.user_data, 42);
        assert!(sqe.uses_fixed_buffer());
    }

    #[test]
    fn test_cqe_v2_success() {
        let cqe = CompletionEntryV2::success(42, 1024);

        assert!(cqe.is_ok());
        assert!(!cqe.is_err());
        assert_eq!(cqe.into_result(), Ok(1024));
    }

    #[test]
    fn test_cqe_v2_error() {
        let cqe = CompletionEntryV2::error(42, SyscallError::NotFound);

        assert!(!cqe.is_ok());
        assert!(cqe.is_err());
        assert_eq!(cqe.into_result(), Err(SyscallError::NotFound));
    }

    #[test]
    fn test_v2_features() {
        let features = V2Features::ALL_V2;

        assert!(features.has(V2Features::CAPABILITY));
        assert!(features.has(V2Features::FIXED_BUFFERS_ONLY));
        assert!(features.has(V2Features::TYPED_RESULT));
        assert!(!features.has(V2Features::SQPOLL));
    }

    #[test]
    fn test_ring_header_v2() {
        let header = RingHeaderV2::new(V2Features::ALL_V2);

        assert!(header.is_empty());
        assert!(!header.is_full());
        assert_eq!(header.pending_count(), 0);
        assert_eq!(header.available_count(), RING_SIZE);

        header.advance_tail(10);
        assert_eq!(header.pending_count(), 10);

        header.advance_head(5);
        assert_eq!(header.pending_count(), 5);
    }
}
