// kernel/src/kernel/io_uring/handlers_v2.rs
//! V2 io_uring operation handlers
//!
//! This module implements capability-based I/O operations for the V2 protocol.
//! All operations use capabilities instead of file descriptors.
//!
//! # Phase 1 Complete: Capability-based Resource Access
//!
//! Resources are stored directly in `CapabilityEntry::resource` as `VfsFile`.
//! File descriptors are no longer used for I/O operations.

use alloc::sync::Arc;

use crate::abi::error::SyscallError;
use crate::abi::io_uring::OpCode;
use crate::abi::io_uring_v2::{CompletionEntryV2, SubmissionEntryV2};
use crate::debug_println;
use crate::kernel::capability::{FileResource, Rights};
use crate::kernel::capability::table::CapabilityTable;
use crate::kernel::core::traits::CharDevice;
use crate::kernel::driver::serial::SERIAL1;
use crate::kernel::fs::VfsFile;
use crate::kernel::io_uring::registered_buffers::RegisteredBufferTable;
use crate::kernel::process::PROCESS_TABLE;

/// Dispatch a V2 SQE to its handler
///
/// This function validates the capability and dispatches to the appropriate handler.
///
/// # Arguments
/// * `sqe` - The V2 submission entry
/// * `cap_table` - The process's capability table
/// * `buf_table` - The registered buffer table (optional)
///
/// # Returns
/// A V2 completion entry with the result
pub fn dispatch_sqe_v2(
    sqe: &SubmissionEntryV2,
    cap_table: &CapabilityTable,
    buf_table: Option<&RegisteredBufferTable>,
) -> CompletionEntryV2 {
    let user_data = sqe.user_data;

    let op = match OpCode::from_u8(sqe.opcode) {
        Some(op) => op,
        None => return CompletionEntryV2::error(user_data, SyscallError::InvalidOpCode),
    };

    match op {
        OpCode::Nop => handle_nop_v2(sqe),
        OpCode::Read => handle_read_v2(sqe, cap_table, buf_table),
        OpCode::Write => handle_write_v2(sqe, cap_table, buf_table),
        OpCode::Close => handle_close_v2(sqe, cap_table),
        OpCode::Mmap => handle_mmap_v2(sqe),
        OpCode::Munmap => handle_munmap_v2(sqe),

        // Not yet implemented
        OpCode::Open
        | OpCode::Fsync
        | OpCode::Poll
        | OpCode::Cancel
        | OpCode::LinkTimeout
        | OpCode::Connect
        | OpCode::Accept
        | OpCode::Send
        | OpCode::Recv => {
            debug_println!("[io_uring_v2] Unimplemented opcode: {:?}", op);
            CompletionEntryV2::error(user_data, SyscallError::NotImplemented)
        }

        // Exit is handled specially
        OpCode::Exit => CompletionEntryV2::error(user_data, SyscallError::InvalidArgument),
    }
}

/// Handle NOP operation (V2)
fn handle_nop_v2(sqe: &SubmissionEntryV2) -> CompletionEntryV2 {
    CompletionEntryV2::success(sqe.user_data, 0)
}

/// Handle read operation with capability verification (V2)
///
/// # Phase 1: Capability-based resource access
///
/// Resources are retrieved from `CapabilityEntry::resource` as `VfsFile`.
/// No longer uses `process.get_file_descriptor()`.
fn handle_read_v2(
    sqe: &SubmissionEntryV2,
    cap_table: &CapabilityTable,
    buf_table: Option<&RegisteredBufferTable>,
) -> CompletionEntryV2 {
    let capability_id = sqe.capability_id;
    let buf_index = sqe.buf_index;
    let len = sqe.len;
    let user_data = sqe.user_data;

    // Special case: capability_id 0 = stdin (not implemented)
    if capability_id == 0 {
        debug_println!("[io_uring_v2] read from stdin not implemented");
        return CompletionEntryV2::error(user_data, SyscallError::NotImplemented);
    }

    // V2 requires registered buffers
    let buf_table = match buf_table {
        Some(t) => t,
        None => return CompletionEntryV2::error(user_data, SyscallError::BufferNotRegistered),
    };

    // Get the registered buffer
    let buf_ref = match buf_table.acquire(buf_index) {
        Some(r) => r,
        None => return CompletionEntryV2::error(user_data, SyscallError::InvalidBufferIndex),
    };

    // Validate buffer is readable (kernel can write to it)
    let slice = match unsafe { buf_ref.as_mut_slice() } {
        Some(s) => s,
        None => return CompletionEntryV2::error(user_data, SyscallError::InsufficientRights),
    };

    // Limit read to requested length
    let read_len = (len as usize).min(slice.len());

    // Get VfsFile from capability table
    let handle: crate::kernel::capability::Handle<FileResource> =
        unsafe { crate::kernel::capability::Handle::from_raw(capability_id) };

    let entry = match cap_table.get_with_rights(&handle, Rights::READ) {
        Ok(e) => e,
        Err(e) => {
            core::mem::forget(handle);
            return CompletionEntryV2::error(user_data, e);
        }
    };

    // Downcast resource to VfsFile
    let vfs_file = match entry.downcast::<VfsFile>() {
        Some(vfs) => vfs,
        None => {
            core::mem::forget(handle);
            return CompletionEntryV2::error(user_data, SyscallError::WrongCapabilityType);
        }
    };

    // Perform read operation
    let result = vfs_file.read(&mut slice[..read_len]);
    core::mem::forget(handle);

    match result {
        Ok(read) => CompletionEntryV2::success(user_data, read as i32),
        Err(crate::kernel::fs::FileError::BrokenPipe) => CompletionEntryV2::success(user_data, 0), // EOF
        Err(crate::kernel::fs::FileError::WouldBlock) => {
            CompletionEntryV2::error(user_data, SyscallError::WouldBlock)
        }
        Err(_) => CompletionEntryV2::error(user_data, SyscallError::IoError),
    }
}

/// Handle write operation with capability verification (V2)
///
/// # Phase 1: Capability-based resource access
///
/// All I/O including stdin/stdout/stderr uses the capability table.
/// Resources are retrieved from `CapabilityEntry::resource` as `VfsFile`.
fn handle_write_v2(
    sqe: &SubmissionEntryV2,
    cap_table: &CapabilityTable,
    buf_table: Option<&RegisteredBufferTable>,
) -> CompletionEntryV2 {
    let capability_id = sqe.capability_id;
    let buf_index = sqe.buf_index;
    let len = sqe.len;
    let user_data = sqe.user_data;

    // V2 requires registered buffers
    let buf_table = match buf_table {
        Some(t) => t,
        None => return CompletionEntryV2::error(user_data, SyscallError::BufferNotRegistered),
    };

    // Get the registered buffer
    let buf_ref = match buf_table.acquire(buf_index) {
        Some(r) => r,
        None => return CompletionEntryV2::error(user_data, SyscallError::InvalidBufferIndex),
    };

    // Validate buffer is writable (kernel can read from it)
    let slice = match unsafe { buf_ref.as_slice() } {
        Some(s) => s,
        None => return CompletionEntryV2::error(user_data, SyscallError::InsufficientRights),
    };

    // Limit write to requested length
    let write_len = (len as usize).min(slice.len());

    // Get VfsFile from capability table (including stdout/stderr at IDs 1, 2)
    let handle: crate::kernel::capability::Handle<FileResource> =
        unsafe { crate::kernel::capability::Handle::from_raw(capability_id) };

    let entry = match cap_table.get_with_rights(&handle, Rights::WRITE) {
        Ok(e) => e,
        Err(e) => {
            core::mem::forget(handle);
            return CompletionEntryV2::error(user_data, e);
        }
    };

    // Downcast resource to VfsFile
    let vfs_file = match entry.downcast::<VfsFile>() {
        Some(vfs) => vfs,
        None => {
            core::mem::forget(handle);
            return CompletionEntryV2::error(user_data, SyscallError::WrongCapabilityType);
        }
    };

    // Perform write operation
    let result = vfs_file.write(&slice[..write_len]);
    core::mem::forget(handle);

    match result {
        Ok(written) => CompletionEntryV2::success(user_data, written as i32),
        Err(crate::kernel::fs::FileError::BrokenPipe) => {
            CompletionEntryV2::error(user_data, SyscallError::BrokenPipe)
        }
        Err(crate::kernel::fs::FileError::WouldBlock) => {
            CompletionEntryV2::error(user_data, SyscallError::WouldBlock)
        }
        Err(_) => CompletionEntryV2::error(user_data, SyscallError::IoError),
    }
}

/// Handle close operation with capability (V2)
///
/// # Phase 1: Capability-based resource access
///
/// When the capability is removed from the table, the `VfsFile` resource
/// is automatically dropped, which calls `close()` on the underlying
/// `FileDescriptor`.
fn handle_close_v2(sqe: &SubmissionEntryV2, cap_table: &CapabilityTable) -> CompletionEntryV2 {
    let capability_id = sqe.capability_id;
    let user_data = sqe.user_data;

    // Special capabilities 0, 1, 2 (stdin/stdout/stderr) cannot be closed
    // These will be proper Capabilities after Task 3 is complete
    if capability_id < 3 {
        return CompletionEntryV2::error(user_data, SyscallError::InvalidArgument);
    }

    // Create handle and remove from capability table
    let handle: crate::kernel::capability::Handle<FileResource> =
        unsafe { crate::kernel::capability::Handle::from_raw(capability_id) };

    match cap_table.remove(handle) {
        Ok(entry) => {
            // VfsFile::drop() will be called when entry goes out of scope,
            // which calls FileDescriptor::close() automatically
            debug_println!(
                "[io_uring_v2] Closed capability {:#x}, type={}, rights={:?}",
                capability_id,
                entry.type_id,
                entry.rights
            );
            CompletionEntryV2::success(user_data, 0)
        }
        Err(e) => CompletionEntryV2::error(user_data, e),
    }
}

/// Handle mmap operation (V2)
///
/// Note: mmap doesn't use capabilities directly, but creates new memory mappings.
fn handle_mmap_v2(sqe: &SubmissionEntryV2) -> CompletionEntryV2 {
    use crate::kernel::mm::allocator::BOOT_INFO_ALLOCATOR;
    use x86_64::structures::paging::{FrameAllocator, Mapper, Page, PageTableFlags, Size4KiB};

    let addr_hint = sqe.off; // Address hint is in `off` field for mmap
    let len = sqe.len as u64;
    let user_data = sqe.user_data;

    if len == 0 {
        return CompletionEntryV2::error(user_data, SyscallError::InvalidArgument);
    }

    // Align length to page size
    let len_aligned = (len + 4095) & !4095;
    let num_pages = (len_aligned / 4096) as usize;

    // Get current process's mmap_top
    let mut table = PROCESS_TABLE.lock();
    let process = match table.current_process_mut() {
        Some(p) => p,
        None => return CompletionEntryV2::error(user_data, SyscallError::NoSuchProcess),
    };

    let start_addr = if addr_hint == 0 {
        process.mmap_top()
    } else {
        // Fixed address not supported
        return CompletionEntryV2::error(user_data, SyscallError::InvalidArgument);
    };

    // Update mmap_top
    let new_top = start_addr + len_aligned;
    process.set_mmap_top(new_top);
    drop(table);

    // Map memory
    let phys_mem_offset = x86_64::VirtAddr::new(
        crate::kernel::mm::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed),
    );
    let (l4_frame, _) = x86_64::registers::control::Cr3::read();
    let l4_table_ptr = (phys_mem_offset + l4_frame.start_address().as_u64()).as_mut_ptr();
    let l4_table = unsafe { &mut *l4_table_ptr };
    let mut mapper =
        unsafe { x86_64::structures::paging::OffsetPageTable::new(l4_table, phys_mem_offset) };

    let mut allocator_lock = BOOT_INFO_ALLOCATOR.lock();
    let frame_allocator = match allocator_lock.as_mut() {
        Some(alloc) => alloc,
        None => return CompletionEntryV2::error(user_data, SyscallError::OutOfMemory),
    };

    let start_page = Page::<Size4KiB>::containing_address(start_addr);
    let flags =
        PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE;

    for i in 0..num_pages {
        let page = start_page + i as u64;
        let frame = match frame_allocator.allocate_frame() {
            Some(f) => f,
            None => {
                // Rollback previous allocations
                for j in 0..i {
                    let page_to_unmap = start_page + j as u64;
                    if let Ok((frame, _)) = mapper.unmap(page_to_unmap) {
                        x86_64::instructions::tlb::flush(page_to_unmap.start_address());
                        unsafe {
                            frame_allocator.deallocate_frame(frame);
                        }
                    }
                }
                return CompletionEntryV2::error(user_data, SyscallError::OutOfMemory);
            }
        };

        unsafe {
            match mapper.map_to(page, frame, flags, frame_allocator) {
                Ok(tlb) => tlb.flush(),
                Err(_) => {
                    frame_allocator.deallocate_frame(frame);
                    // Rollback
                    for j in 0..i {
                        let page_to_unmap = start_page + j as u64;
                        if let Ok((frame, _)) = mapper.unmap(page_to_unmap) {
                            x86_64::instructions::tlb::flush(page_to_unmap.start_address());
                            frame_allocator.deallocate_frame(frame);
                        }
                    }
                    return CompletionEntryV2::error(user_data, SyscallError::MmapFailed);
                }
            }
        }

        // Zero the frame
        if let Ok(frame) = mapper.translate_page(page) {
            let frame_ptr =
                (phys_mem_offset + frame.start_address().as_u64()).as_mut_ptr::<u8>();
            unsafe {
                core::ptr::write_bytes(frame_ptr, 0, 4096);
            }
        }
    }

    CompletionEntryV2::success(user_data, start_addr.as_u64() as i32)
}

/// Handle munmap operation (V2)
fn handle_munmap_v2(sqe: &SubmissionEntryV2) -> CompletionEntryV2 {
    use x86_64::structures::paging::{Mapper, Page, Size4KiB};

    let addr = sqe.off; // Address is in `off` field
    let len = sqe.len as u64;
    let user_data = sqe.user_data;

    if addr == 0 || len == 0 {
        return CompletionEntryV2::error(user_data, SyscallError::InvalidArgument);
    }

    // Align length to page size
    let len_aligned = (len + 4095) & !4095;

    let phys_mem_offset = x86_64::VirtAddr::new(
        crate::kernel::mm::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed),
    );
    let (l4_frame, _) = x86_64::registers::control::Cr3::read();
    let l4_table_ptr = (phys_mem_offset + l4_frame.start_address().as_u64()).as_mut_ptr();
    let l4_table = unsafe { &mut *l4_table_ptr };
    let mut mapper =
        unsafe { x86_64::structures::paging::OffsetPageTable::new(l4_table, phys_mem_offset) };

    let start_addr = x86_64::VirtAddr::new(addr);
    let start_page = Page::<Size4KiB>::containing_address(start_addr);
    let end_page = Page::<Size4KiB>::containing_address(start_addr + len_aligned);

    for page in Page::range(start_page, end_page) {
        if let Ok((frame, _)) = mapper.unmap(page) {
            x86_64::instructions::tlb::flush(page.start_address());

            unsafe {
                let mut allocator_lock = crate::kernel::mm::allocator::BOOT_INFO_ALLOCATOR.lock();
                if let Some(frame_allocator) = allocator_lock.as_mut() {
                    frame_allocator.deallocate_frame(frame);
                }
            }
        }
    }

    CompletionEntryV2::success(user_data, 0)
}

/// Convert a V2 SQE to V1 format for backward compatibility
///
/// This allows gradual migration by converting V2 requests to V1
/// when the full V2 path is not yet implemented.
pub fn sqe_v2_to_v1(sqe_v2: &SubmissionEntryV2) -> crate::abi::io_uring::SubmissionEntry {
    crate::abi::io_uring::SubmissionEntry {
        opcode: sqe_v2.opcode,
        flags: sqe_v2.flags,
        ioprio: sqe_v2.ioprio,
        fd: sqe_v2.capability_id as i32, // Truncate to i32 for V1 compatibility
        off: sqe_v2.off,
        addr: 0, // V2 uses registered buffers, not addresses
        len: sqe_v2.len,
        op_flags: sqe_v2.op_flags,
        user_data: sqe_v2.user_data,
        buf_index: sqe_v2.buf_index as u16,
        personality: 0,
        splice_fd_in: 0,
        _reserved: [0; 2],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handle_nop_v2() {
        let sqe = SubmissionEntryV2::nop(42);
        let cqe = handle_nop_v2(&sqe);
        assert!(cqe.is_ok());
        assert_eq!(cqe.user_data, 42);
        assert_eq!(cqe.result_value, 0);
    }

    #[test]
    fn test_sqe_v2_to_v1_conversion() {
        let sqe_v2 = SubmissionEntryV2::read(0x12345678, 0, 1024, 100, 42);
        let sqe_v1 = sqe_v2_to_v1(&sqe_v2);

        assert_eq!(sqe_v1.opcode, sqe_v2.opcode);
        assert_eq!(sqe_v1.fd, 0x12345678); // Truncated from u64
        assert_eq!(sqe_v1.len, sqe_v2.len);
        assert_eq!(sqe_v1.user_data, sqe_v2.user_data);
    }
}
