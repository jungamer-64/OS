// kernel/src/kernel/io_uring/handlers.rs
//! io_uring operation handlers
//!
//! This module implements the actual I/O operations triggered by SQEs.
//! Each handler takes a validated SQE and returns a result to be posted to the CQ.

use crate::abi::io_uring::{SubmissionEntry, OpCode};
use crate::debug_println;
use crate::kernel::core::traits::CharDevice;
use crate::kernel::driver::serial::SERIAL1;
use crate::kernel::process::PROCESS_TABLE;
use crate::kernel::syscall::{EFAULT, EBADF, EINVAL, EIO, ENOMEM, ENOSYS};

/// Result from an io_uring operation
#[derive(Debug)]
pub struct OpResult {
    /// The user_data from the SQE
    pub user_data: u64,
    /// Result value (bytes transferred or error)
    pub result: i32,
    /// Completion flags
    pub flags: u32,
}

impl OpResult {
    /// Create a success result
    #[must_use]
    pub const fn success(user_data: u64, result: i32) -> Self {
        Self { user_data, result, flags: 0 }
    }
    
    /// Create an error result
    #[must_use]
    pub const fn error(user_data: u64, errno: i32) -> Self {
        Self { user_data, result: -errno, flags: 0 }
    }
}

/// Dispatch an SQE to its handler
///
/// # Arguments
/// * `sqe` - The submission entry (already validated)
///
/// # Returns
/// The operation result to be posted to the CQ
pub fn dispatch_sqe(sqe: &SubmissionEntry) -> OpResult {
    let user_data = sqe.user_data;
    
    let op = match OpCode::from_u8(sqe.opcode) {
        Some(op) => op,
        None => return OpResult::error(user_data, 38), // ENOSYS
    };
    
    match op {
        OpCode::Nop => handle_nop(sqe),
        OpCode::Read => handle_read(sqe),
        OpCode::Write => handle_write(sqe),
        OpCode::Close => handle_close(sqe),
        OpCode::Mmap => handle_mmap(sqe),
        OpCode::Munmap => handle_munmap(sqe),
        
        // Not yet implemented
        OpCode::Open |
        OpCode::Fsync |
        OpCode::Poll |
        OpCode::Cancel |
        OpCode::LinkTimeout |
        OpCode::Connect |
        OpCode::Accept |
        OpCode::Send |
        OpCode::Recv => {
            debug_println!("[io_uring] Unimplemented opcode: {:?}", op);
            OpResult::error(user_data, 38) // ENOSYS
        }
        
        // Exit is handled specially (doesn't go through normal path)
        OpCode::Exit => OpResult::error(user_data, 22), // EINVAL
    }
}

/// Handle NOP operation
fn handle_nop(sqe: &SubmissionEntry) -> OpResult {
    OpResult::success(sqe.user_data, 0)
}

/// Handle read operation
fn handle_read(sqe: &SubmissionEntry) -> OpResult {
    let fd = sqe.fd;
    let buf = sqe.addr;
    let len = sqe.len;
    let user_data = sqe.user_data;
    
    // Special case: FD 0 = stdin (not implemented)
    if fd == 0 {
        debug_println!("[io_uring] read from stdin not implemented");
        return OpResult::error(user_data, 38); // ENOSYS
    }
    
    // Get file descriptor from process
    let table = PROCESS_TABLE.lock();
    let process = match table.current_process() {
        Some(p) => p,
        None => return OpResult::error(user_data, 3), // ESRCH
    };
    
    let fd_arc = match process.get_file_descriptor(fd as u64) {
        Some(fd) => fd,
        None => return OpResult::error(user_data, 9), // EBADF
    };
    
    // Read into buffer
    let slice = unsafe {
        core::slice::from_raw_parts_mut(buf as *mut u8, len as usize)
    };
    
    let mut fd_lock = fd_arc.lock();
    match fd_lock.read(slice) {
        Ok(read) => OpResult::success(user_data, read as i32),
        Err(crate::kernel::fs::FileError::BrokenPipe) => OpResult::success(user_data, 0), // EOF
        Err(crate::kernel::fs::FileError::WouldBlock) => OpResult::error(user_data, 11), // EAGAIN
        Err(_) => OpResult::error(user_data, 5), // EIO
    }
}

/// Handle write operation
fn handle_write(sqe: &SubmissionEntry) -> OpResult {
    let fd = sqe.fd;
    let buf = sqe.addr;
    let len = sqe.len;
    let user_data = sqe.user_data;
    
    // Special case: FD 1 = stdout (console)
    if fd == 1 {
        // Safety: Buffer has been validated by validate_sqe
        let slice = unsafe {
            core::slice::from_raw_parts(buf as *const u8, len as usize)
        };
        
        // Write to serial console
        if let Some(mut serial) = SERIAL1.try_lock() {
            for &byte in slice {
                let _ = serial.write_byte(byte);
            }
        }
        
        return OpResult::success(user_data, len as i32);
    }
    
    // FD 2 = stderr (same as stdout for now)
    if fd == 2 {
        let slice = unsafe {
            core::slice::from_raw_parts(buf as *const u8, len as usize)
        };
        
        if let Some(mut serial) = SERIAL1.try_lock() {
            for &byte in slice {
                let _ = serial.write_byte(byte);
            }
        }
        
        return OpResult::success(user_data, len as i32);
    }
    
    // Get file descriptor from process
    let table = PROCESS_TABLE.lock();
    let process = match table.current_process() {
        Some(p) => p,
        None => return OpResult::error(user_data, 3), // ESRCH
    };
    
    let fd_arc = match process.get_file_descriptor(fd as u64) {
        Some(fd) => fd,
        None => return OpResult::error(user_data, 9), // EBADF
    };
    
    let slice = unsafe {
        core::slice::from_raw_parts(buf as *const u8, len as usize)
    };
    
    let mut fd_lock = fd_arc.lock();
    match fd_lock.write(slice) {
        Ok(written) => OpResult::success(user_data, written as i32),
        Err(crate::kernel::fs::FileError::BrokenPipe) => OpResult::error(user_data, 32), // EPIPE
        Err(crate::kernel::fs::FileError::WouldBlock) => OpResult::error(user_data, 11), // EAGAIN
        Err(_) => OpResult::error(user_data, 5), // EIO
    }
}

/// Handle close operation
fn handle_close(sqe: &SubmissionEntry) -> OpResult {
    let fd = sqe.fd as u64;
    let user_data = sqe.user_data;
    
    // Can't close stdin/stdout/stderr
    if fd < 3 {
        return OpResult::error(user_data, 22); // EINVAL
    }
    
    let mut table = PROCESS_TABLE.lock();
    let process = match table.current_process_mut() {
        Some(p) => p,
        None => return OpResult::error(user_data, 3), // ESRCH
    };
    
    match process.remove_file_descriptor(fd) {
        Some(fd_arc) => {
            let mut fd_lock = fd_arc.lock();
            let _ = fd_lock.close();
            OpResult::success(user_data, 0)
        }
        None => OpResult::error(user_data, 9), // EBADF
    }
}

/// Handle mmap operation
fn handle_mmap(sqe: &SubmissionEntry) -> OpResult {
    use crate::kernel::mm::allocator::BOOT_INFO_ALLOCATOR;
    use x86_64::structures::paging::{Page, PageTableFlags, Mapper, FrameAllocator, Size4KiB};
    
    let addr_hint = sqe.addr;
    let len = sqe.len as u64;
    let user_data = sqe.user_data;
    
    if len == 0 {
        return OpResult::error(user_data, 22); // EINVAL
    }
    
    // Align length to page size
    let len_aligned = (len + 4095) & !4095;
    let num_pages = (len_aligned / 4096) as usize;
    
    // Get current process's mmap_top
    let mut table = PROCESS_TABLE.lock();
    let process = match table.current_process_mut() {
        Some(p) => p,
        None => return OpResult::error(user_data, 3), // ESRCH
    };
    
    let start_addr = if addr_hint == 0 {
        process.mmap_top()
    } else {
        // Fixed address not supported
        return OpResult::error(user_data, 22); // EINVAL
    };
    
    // Update mmap_top
    let new_top = start_addr + len_aligned;
    process.set_mmap_top(new_top);
    
    // Need to drop table lock before accessing BOOT_INFO_ALLOCATOR
    drop(table);
    
    // Map memory
    let phys_mem_offset = x86_64::VirtAddr::new(
        crate::kernel::mm::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed)
    );
    let (l4_frame, _) = x86_64::registers::control::Cr3::read();
    let l4_table_ptr = (phys_mem_offset + l4_frame.start_address().as_u64()).as_mut_ptr();
    let l4_table = unsafe { &mut *l4_table_ptr };
    let mut mapper = unsafe {
        x86_64::structures::paging::OffsetPageTable::new(l4_table, phys_mem_offset)
    };
    
    let mut allocator_lock = BOOT_INFO_ALLOCATOR.lock();
    let frame_allocator = match allocator_lock.as_mut() {
        Some(alloc) => alloc,
        None => return OpResult::error(user_data, 12), // ENOMEM
    };
    
    let start_page = Page::<Size4KiB>::containing_address(start_addr);
    let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE;
    
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
                        unsafe { frame_allocator.deallocate_frame(frame); }
                    }
                }
                return OpResult::error(user_data, 12); // ENOMEM
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
                    return OpResult::error(user_data, 12); // ENOMEM
                }
            }
        }
        
        // Zero the frame
        if let Ok(frame) = mapper.translate_page(page) {
            let frame_ptr = (phys_mem_offset + frame.start_address().as_u64()).as_mut_ptr::<u8>();
            unsafe { core::ptr::write_bytes(frame_ptr, 0, 4096); }
        }
    }
    
    OpResult::success(user_data, start_addr.as_u64() as i32)
}

/// Handle munmap operation
fn handle_munmap(sqe: &SubmissionEntry) -> OpResult {
    use x86_64::structures::paging::{Page, Mapper, Size4KiB};
    
    let addr = sqe.addr;
    let len = sqe.len as u64;
    let user_data = sqe.user_data;
    
    if addr == 0 || len == 0 {
        return OpResult::error(user_data, 22); // EINVAL
    }
    
    // Align length to page size
    let len_aligned = (len + 4095) & !4095;
    
    let phys_mem_offset = x86_64::VirtAddr::new(
        crate::kernel::mm::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed)
    );
    let (l4_frame, _) = x86_64::registers::control::Cr3::read();
    let l4_table_ptr = (phys_mem_offset + l4_frame.start_address().as_u64()).as_mut_ptr();
    let l4_table = unsafe { &mut *l4_table_ptr };
    let mut mapper = unsafe {
        x86_64::structures::paging::OffsetPageTable::new(l4_table, phys_mem_offset)
    };
    
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
    
    OpResult::success(user_data, 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::abi::io_uring::SubmissionEntry;
    
    #[test]
    fn test_handle_nop() {
        let sqe = SubmissionEntry::nop(42);
        let result = handle_nop(&sqe);
        assert_eq!(result.user_data, 42);
        assert_eq!(result.result, 0);
    }
}
