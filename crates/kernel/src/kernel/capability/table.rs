// kernel/src/kernel/capability/table.rs
//! Capability Table Implementation
//!
//! This module implements the per-process capability table that maps
//! capability handles to actual resources.
//!
//! # Design
//!
//! Each process has its own `CapabilityTable` that:
//! - Maps handle IDs to `CapabilityEntry` structures
//! - Tracks rights and reference counts
//! - Prevents use-after-close via generation numbers
//!
//! # Thread Safety
//!
//! The table uses fine-grained locking to allow concurrent access:
//! - Reads can proceed in parallel
//! - Writes to different slots can proceed in parallel
//! - Generation numbers prevent ABA problems

use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::any::Any;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use spin::RwLock;

use super::{next_generation, Handle, ResourceKind, Rights};
use crate::abi::error::SyscallError;

/// Maximum number of capabilities per process
pub const MAX_CAPABILITIES: usize = 4096;

/// Initial capacity for the capability table
const INITIAL_CAPACITY: usize = 64;

/// A single entry in the capability table
pub struct CapabilityEntry {
    /// Resource type ID
    pub type_id: u32,

    /// Generation number for this slot
    pub generation: u32,

    /// Rights associated with this capability
    pub rights: Rights,

    /// Reference count (for shared capabilities)
    pub ref_count: AtomicU32,

    /// The actual resource
    pub resource: Arc<dyn Any + Send + Sync>,
}

impl CapabilityEntry {
    /// Create a new capability entry
    pub fn new<R: Any + Send + Sync>(
        type_id: u32,
        generation: u32,
        rights: Rights,
        resource: Arc<R>,
    ) -> Self {
        Self {
            type_id,
            generation,
            rights,
            ref_count: AtomicU32::new(1),
            resource,
        }
    }

    /// Increment the reference count
    pub fn inc_ref(&self) -> u32 {
        self.ref_count.fetch_add(1, Ordering::AcqRel) + 1
    }

    /// Decrement the reference count, returns true if it reached zero
    pub fn dec_ref(&self) -> bool {
        self.ref_count.fetch_sub(1, Ordering::AcqRel) == 1
    }

    /// Get the current reference count
    pub fn ref_count(&self) -> u32 {
        self.ref_count.load(Ordering::Acquire)
    }

    /// Try to downcast the resource to a specific type
    pub fn downcast<T: Any + Send + Sync>(&self) -> Option<&T> {
        self.resource.downcast_ref::<T>()
    }

    /// Try to get an Arc to the resource as a specific type
    pub fn downcast_arc<T: Any + Send + Sync>(self: &Arc<Self>) -> Option<Arc<T>> {
        // We can't directly convert Arc<dyn Any> to Arc<T>, so we return a clone
        self.resource.clone().downcast::<T>().ok()
    }
}

impl core::fmt::Debug for CapabilityEntry {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CapabilityEntry")
            .field("type_id", &self.type_id)
            .field("generation", &self.generation)
            .field("rights", &self.rights)
            .field("ref_count", &self.ref_count())
            .finish()
    }
}

/// Slot in the capability table
enum Slot {
    /// Empty slot, available for allocation
    Empty,
    /// Occupied slot with a capability entry
    Occupied(Box<CapabilityEntry>),
}

impl Slot {
    fn is_empty(&self) -> bool {
        matches!(self, Slot::Empty)
    }

    fn as_entry(&self) -> Option<&CapabilityEntry> {
        match self {
            Slot::Occupied(entry) => Some(entry),
            Slot::Empty => None,
        }
    }

    fn as_entry_mut(&mut self) -> Option<&mut CapabilityEntry> {
        match self {
            Slot::Occupied(entry) => Some(entry),
            Slot::Empty => None,
        }
    }

    fn take(&mut self) -> Option<Box<CapabilityEntry>> {
        match core::mem::replace(self, Slot::Empty) {
            Slot::Occupied(entry) => Some(entry),
            Slot::Empty => None,
        }
    }
}

/// Per-process capability table
///
/// This table maps capability handles to actual resources. Each process
/// has its own table, providing isolation between processes.
pub struct CapabilityTable {
    /// Slots in the table
    slots: RwLock<Vec<Slot>>,

    /// Number of active capabilities
    count: AtomicU32,

    /// Next slot to try for allocation (hint for O(1) average insertion)
    next_free_hint: AtomicU32,

    /// Generation counter for this table
    generation: AtomicU64,
}

impl CapabilityTable {
    /// Create a new capability table
    pub fn new() -> Self {
        let mut slots = Vec::with_capacity(INITIAL_CAPACITY);
        for _ in 0..INITIAL_CAPACITY {
            slots.push(Slot::Empty);
        }

        Self {
            slots: RwLock::new(slots),
            count: AtomicU32::new(0),
            next_free_hint: AtomicU32::new(0),
            generation: AtomicU64::new(1),
        }
    }

    /// Get the number of active capabilities
    pub fn count(&self) -> u32 {
        self.count.load(Ordering::Acquire)
    }

    /// Check if the table is empty
    pub fn is_empty(&self) -> bool {
        self.count() == 0
    }

    /// Insert a new capability into the table
    ///
    /// Returns a handle to the capability, or an error if the table is full.
    pub fn insert<R: ResourceKind, T: Any + Send + Sync>(
        &self,
        resource: Arc<T>,
        rights: Rights,
    ) -> Result<Handle<R>, SyscallError> {
        // Check if we've hit the limit
        if self.count() >= MAX_CAPABILITIES as u32 {
            return Err(SyscallError::CapabilityTableFull);
        }

        let generation = next_generation();
        let entry = Box::new(CapabilityEntry::new(R::TYPE_ID, generation, rights, resource));

        let mut slots = self.slots.write();

        // Try the hint first
        let hint = self.next_free_hint.load(Ordering::Relaxed) as usize;
        let index = if hint < slots.len() && slots[hint].is_empty() {
            hint
        } else {
            // Linear search for a free slot
            match slots.iter().position(|s| s.is_empty()) {
                Some(idx) => idx,
                None => {
                    // Need to grow the table
                    if slots.len() >= MAX_CAPABILITIES {
                        return Err(SyscallError::CapabilityTableFull);
                    }
                    let idx = slots.len();
                    slots.push(Slot::Empty);
                    idx
                }
            }
        };

        slots[index] = Slot::Occupied(entry);

        // Update the hint for next allocation
        self.next_free_hint
            .store((index + 1) as u32, Ordering::Relaxed);
        self.count.fetch_add(1, Ordering::Release);

        Ok(Handle::new(index as u32, generation))
    }

    /// Insert a capability at a specific index with generation 0
    ///
    /// This is used for well-known capabilities like stdin/stdout/stderr
    /// that have fixed IDs (0, 1, 2). Using generation=0 allows user programs
    /// to use simple integer FD values like 0, 1, 2.
    ///
    /// # Arguments
    /// * `index` - The fixed index (0, 1, or 2 for stdio)
    /// * `resource` - The resource to store
    /// * `rights` - Access rights
    ///
    /// # Returns
    /// A handle with the specified index and generation=0
    pub fn insert_at_index<R: ResourceKind, T: Any + Send + Sync>(
        &self,
        index: u32,
        resource: Arc<T>,
        rights: Rights,
    ) -> Result<Handle<R>, SyscallError> {
        let index_usize = index as usize;

        // Use generation 0 for well-known capabilities
        // This allows userland to use simple values like 0, 1, 2 for stdio
        let generation = 0u32;
        let entry = Box::new(CapabilityEntry::new(R::TYPE_ID, generation, rights, resource));

        let mut slots = self.slots.write();

        // Ensure the table is large enough
        while slots.len() <= index_usize {
            slots.push(Slot::Empty);
        }

        // Check if slot is already occupied
        if !slots[index_usize].is_empty() {
            return Err(SyscallError::CapabilityTableFull);
        }

        slots[index_usize] = Slot::Occupied(entry);
        self.count.fetch_add(1, Ordering::Release);

        // Update hint to skip well-known slots
        let current_hint = self.next_free_hint.load(Ordering::Relaxed);
        if current_hint <= index {
            self.next_free_hint.store(index + 1, Ordering::Relaxed);
        }

        Ok(Handle::new(index, generation))
    }

    /// Get a capability entry by handle
    ///
    /// Returns the entry if valid, or an error if the handle is invalid.
    pub fn get<R: ResourceKind>(&self, handle: &Handle<R>) -> Result<&CapabilityEntry, SyscallError> {
        let index = handle.index() as usize;
        let generation = handle.generation();

        let slots = self.slots.read();

        if index >= slots.len() {
            return Err(SyscallError::InvalidCapability);
        }

        let entry = slots[index]
            .as_entry()
            .ok_or(SyscallError::InvalidCapability)?;

        // Verify generation (prevents ABA problems)
        if entry.generation != generation {
            return Err(SyscallError::CapabilityRevoked);
        }

        // Verify type
        if entry.type_id != R::TYPE_ID {
            return Err(SyscallError::WrongCapabilityType);
        }

        // Safety: The entry is valid and won't be modified while we hold the read lock
        // We return a reference with lifetime tied to self, which is safe because
        // the table outlives any operation using it.
        Ok(unsafe { &*(entry as *const CapabilityEntry) })
    }

    /// Get a capability entry with rights verification
    pub fn get_with_rights<R: ResourceKind>(
        &self,
        handle: &Handle<R>,
        required: Rights,
    ) -> Result<&CapabilityEntry, SyscallError> {
        let entry = self.get(handle)?;

        if !entry.rights.contains(required) {
            return Err(SyscallError::InsufficientRights);
        }

        Ok(entry)
    }

    /// Remove a capability from the table
    ///
    /// Returns the entry if it was removed, or an error if invalid.
    pub fn remove<R: ResourceKind>(
        &self,
        handle: Handle<R>,
    ) -> Result<Box<CapabilityEntry>, SyscallError> {
        let index = handle.index() as usize;
        let generation = handle.generation();

        let mut slots = self.slots.write();

        if index >= slots.len() {
            return Err(SyscallError::InvalidCapability);
        }

        // Verify the slot before removing
        {
            let entry = slots[index]
                .as_entry()
                .ok_or(SyscallError::InvalidCapability)?;

            if entry.generation != generation {
                return Err(SyscallError::CapabilityRevoked);
            }

            if entry.type_id != R::TYPE_ID {
                return Err(SyscallError::WrongCapabilityType);
            }
        }

        let entry = slots[index].take().unwrap();

        // Update hint if this slot is earlier than the current hint
        let hint = self.next_free_hint.load(Ordering::Relaxed) as usize;
        if index < hint {
            self.next_free_hint.store(index as u32, Ordering::Relaxed);
        }

        self.count.fetch_sub(1, Ordering::Release);

        // Forget the handle to prevent double-drop
        core::mem::forget(handle);

        Ok(entry)
    }

    /// Duplicate a capability with potentially restricted rights
    ///
    /// The new capability will have at most the rights of the original,
    /// further restricted by the `restrict_to` mask if provided.
    pub fn duplicate<R: ResourceKind>(
        &self,
        handle: &Handle<R>,
        restrict_to: Option<Rights>,
    ) -> Result<Handle<R>, SyscallError> {
        // First, verify the original handle and check DUP right
        let entry = self.get_with_rights(handle, Rights::DUP)?;

        // Calculate new rights
        let new_rights = match restrict_to {
            Some(mask) => entry.rights.restrict(mask),
            None => entry.rights,
        };

        // Clone the resource and insert a new entry
        let resource = entry.resource.clone();

        // Create a new entry with the same resource but potentially different rights
        let generation = next_generation();
        let new_entry = Box::new(CapabilityEntry {
            type_id: R::TYPE_ID,
            generation,
            rights: new_rights,
            ref_count: AtomicU32::new(1),
            resource,
        });

        let mut slots = self.slots.write();

        // Find a free slot
        let index = match slots.iter().position(|s| s.is_empty()) {
            Some(idx) => idx,
            None => {
                if slots.len() >= MAX_CAPABILITIES {
                    return Err(SyscallError::CapabilityTableFull);
                }
                let idx = slots.len();
                slots.push(Slot::Empty);
                idx
            }
        };

        slots[index] = Slot::Occupied(new_entry);
        self.count.fetch_add(1, Ordering::Release);

        Ok(Handle::new(index as u32, generation))
    }

    /// Check if a capability exists and has certain rights
    pub fn has_rights<R: ResourceKind>(&self, handle: &Handle<R>, rights: Rights) -> bool {
        self.get_with_rights(handle, rights).is_ok()
    }

    /// Get a snapshot of all valid capability indices (for debugging/cleanup)
    ///
    /// Returns a vector of (index, type_id, rights) tuples for all valid entries.
    pub fn snapshot_entries(&self) -> Vec<(u32, u32, Rights)> {
        let slots = self.slots.read();
        slots
            .iter()
            .enumerate()
            .filter_map(|(i, slot)| {
                slot.as_entry()
                    .map(|e| (i as u32, e.type_id, e.rights))
            })
            .collect()
    }

    /// Clear all capabilities (used during process cleanup)
    pub fn clear(&self) {
        let mut slots = self.slots.write();
        for slot in slots.iter_mut() {
            *slot = Slot::Empty;
        }
        self.count.store(0, Ordering::Release);
        self.next_free_hint.store(0, Ordering::Relaxed);
    }
}

impl Default for CapabilityTable {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for CapabilityTable {
    fn drop(&mut self) {
        // Ensure all resources are properly cleaned up
        self.clear();
    }
}

impl core::fmt::Debug for CapabilityTable {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CapabilityTable")
            .field("count", &self.count())
            .finish()
    }
}

/// Error type for capability operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityError {
    /// The capability handle is invalid
    Invalid,
    /// The capability has been revoked (generation mismatch)
    Revoked,
    /// Wrong capability type for this operation
    WrongType,
    /// Insufficient rights for the requested operation
    InsufficientRights,
    /// The capability table is full
    TableFull,
}

impl From<CapabilityError> for SyscallError {
    fn from(err: CapabilityError) -> Self {
        match err {
            CapabilityError::Invalid => SyscallError::InvalidCapability,
            CapabilityError::Revoked => SyscallError::CapabilityRevoked,
            CapabilityError::WrongType => SyscallError::WrongCapabilityType,
            CapabilityError::InsufficientRights => SyscallError::InsufficientRights,
            CapabilityError::TableFull => SyscallError::CapabilityTableFull,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Simple test resource
    struct TestResource {
        value: i32,
    }

    #[test]
    fn test_insert_and_get() {
        let table = CapabilityTable::new();

        let resource = Arc::new(TestResource { value: 42 });
        let handle: Handle<FileResource> = table
            .insert(resource, Rights::READ_WRITE)
            .expect("insert failed");

        let entry = table.get(&handle).expect("get failed");
        assert_eq!(entry.type_id, FileResource::TYPE_ID);
        assert!(entry.rights.contains(Rights::READ));
        assert!(entry.rights.contains(Rights::WRITE));

        let test_res = entry.downcast::<TestResource>().expect("downcast failed");
        assert_eq!(test_res.value, 42);
    }

    #[test]
    fn test_rights_check() {
        let table = CapabilityTable::new();

        let resource = Arc::new(TestResource { value: 42 });
        let handle: Handle<FileResource> = table
            .insert(resource, Rights::READ_ONLY)
            .expect("insert failed");

        // READ should work
        assert!(table.get_with_rights(&handle, Rights::READ).is_ok());

        // WRITE should fail
        assert!(table.get_with_rights(&handle, Rights::WRITE).is_err());
    }

    #[test]
    fn test_remove() {
        let table = CapabilityTable::new();

        let resource = Arc::new(TestResource { value: 42 });
        let handle: Handle<FileResource> = table
            .insert(resource, Rights::READ_WRITE)
            .expect("insert failed");

        assert_eq!(table.count(), 1);

        let entry = table.remove(handle).expect("remove failed");
        assert_eq!(entry.type_id, FileResource::TYPE_ID);

        assert_eq!(table.count(), 0);
    }

    #[test]
    fn test_duplicate() {
        let table = CapabilityTable::new();

        let resource = Arc::new(TestResource { value: 42 });
        let handle: Handle<FileResource> = table
            .insert(resource, Rights::FULL)
            .expect("insert failed");

        // Duplicate with restricted rights
        let dup_handle = table
            .duplicate(&handle, Some(Rights::READ_ONLY))
            .expect("duplicate failed");

        assert_eq!(table.count(), 2);

        // Original should still have full rights
        assert!(table.get_with_rights(&handle, Rights::WRITE).is_ok());

        // Duplicate should only have read rights
        let dup_entry = table.get(&dup_handle).expect("get dup failed");
        assert!(dup_entry.rights.contains(Rights::READ));
        assert!(!dup_entry.rights.contains(Rights::WRITE));
    }

    #[test]
    fn test_generation_check() {
        let table = CapabilityTable::new();

        let resource = Arc::new(TestResource { value: 42 });
        let handle: Handle<FileResource> = table
            .insert(resource, Rights::READ_WRITE)
            .expect("insert failed");

        let index = handle.index();
        let _entry = table.remove(handle).expect("remove failed");

        // Insert a new capability in the same slot
        let resource2 = Arc::new(TestResource { value: 100 });
        let _handle2: Handle<FileResource> = table
            .insert(resource2, Rights::READ_WRITE)
            .expect("insert failed");

        // Create a fake handle with the old generation
        let fake_handle: Handle<FileResource> = unsafe { Handle::from_raw(index as u64) };

        // Should fail due to generation mismatch
        let result = table.get(&fake_handle);
        assert!(result.is_err());
    }
}
