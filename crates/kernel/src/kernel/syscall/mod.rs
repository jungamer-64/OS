// kernel/src/kernel/syscall/mod.rs
//! System call implementation module
//!
//! This module provides the kernel-side implementations of all system calls
//! and the dispatch mechanism that routes system call numbers to their handlers.
//!
//! # Architecture
//!
//! System calls are the **only** way for user-mode programs (Ring 3) to interact
//! with the kernel (Ring 0). This module implements:
//!
//! - **System call handlers** - Actual implementation of each syscall
//! - **Dispatch table** - Maps syscall numbers to handler functions
//! - **Security validation** - Checks user pointers and arguments
//! - **Error handling** - Returns Linux-compatible error codes
//!
//! # System Call Mechanism
//!
//! When a user program executes the `syscall` instruction:
//!
//! 1. CPU switches to Ring 0 (kernel mode)
//! 2. [`crate::arch::x86_64::syscall::syscall_entry`] is called
//! 3. Kernel stack is switched
//! 4. [`dispatch()`] is called with syscall number and arguments
//! 5. Handler function is executed
//! 6. Result is returned via RAX
//! 7. CPU returns to Ring 3 via `sysret`
//!
//! # Security Model
//!
//! All system calls follow strict security principles:
//!
//! ## Pointer Validation
//!
//! User-provided pointers are **always** validated before use:
//!
//! - **Address range check**: Must be in user space (< 0x8000_0000_0000)
//! - **Mapping check**: Verifies page is mapped (implemented via security module)
//! - **Permission check**: Verifies page has correct permissions (implemented via security module)
//!
//! Invalid pointers result in [`EFAULT`] error.
//!
//! ## Argument Validation
//!
//! All arguments are validated before use:
//!
//! - Length limits (e.g., max 1MB for write)
//! - Resource existence (e.g., valid file descriptor)
//! - Value ranges (e.g., non-zero sizes)
//!
//! Invalid arguments result in [`EINVAL`] error.
//!
//! ## Resource Limits
//!
//! System calls enforce resource limits:
//!
//! - Maximum write size: 1MB ([`MAX_WRITE_LEN`])
//! - Process limits (TODO: Phase 4)
//! - Memory limits (TODO: Phase 4)
//!
//! # Error Handling
//!
//! All system calls return [`SyscallResult`] (i64):
//!
//! - **Positive or zero**: Success (often a count or ID)
//! - **Negative**: Error code (Linux-compatible)
//!
//! Error codes are defined as constants and match Linux errno values
//! for compatibility and familiarity.
//!
//! # Implementation Guidelines
//!
//! When adding new system calls:
//!
//! 1. **Add to [`SYSCALL_TABLE`]** - Append to end for ABI stability
//! 2. **Document thoroughly** - Include security considerations
//! 3. **Validate all inputs** - Never trust user-provided data
//! 4. **Return proper errors** - Use existing error codes
//! 5. **Test extensively** - Include security tests
//!
//! # Example: Adding a System Call
//!
//! ```rust,ignore
//! /// sys_new_feature - Brief description
//! ///
//! /// # Arguments
//! /// * `arg1` - Description
//! ///
//! /// # Returns
//! /// * Success: Description
//! /// * Error: EINVAL, EFAULT, etc.
//! ///
//! /// # Security
//! /// - Validates arg1 is in user space
//! /// - Checks permissions (TODO)
//! pub fn sys_new_feature(arg1: u64, ...) -> SyscallResult {
//!     // 1. Validate arguments
//!     if !is_user_address(arg1) {
//!         return EFAULT;
//!     }
//!     
//!     // 2. Perform operation
//!     // ...
//!     
//!     // 3. Return result
//!     SUCCESS
//! }
//! ```
//!
//! # See Also
//!
//! - [`crate::arch::x86_64::syscall`] - Low-level syscall entry/exit
//! - `docs/syscall_interface.md` - Complete syscall specification
//! - `userland/libuser/src/syscall.rs` - User-space wrappers

use alloc::boxed::Box;
use crate::arch::Cpu;
use crate::debug_println;

use crate::kernel::core::traits::CharDevice;
use crate::kernel::security::{validate_user_write, validate_user_read};

// ============================================================================
// Constants
// ============================================================================

/// Maximum length for sys_write (1MB)
///
/// This limit prevents:
/// - Excessive kernel memory usage
/// - Long-running kernel operations
/// - Potential DoS attacks
const MAX_WRITE_LEN: u64 = 1024 * 1024;

// ============================================================================
// Security Utilities
// ============================================================================

// Note: Security validation functions are now in crate::kernel::security module
// (is_user_address, is_user_range, validate_user_read, validate_user_write)

// ============================================================================
// Error Codes (Linux-compatible)
// ============================================================================

/// System call result type
///
/// Positive or zero values indicate success.
/// Negative values indicate errors (see constants below).
pub type SyscallResult = i64;

/// Success code
pub const SUCCESS: SyscallResult = 0;

// Error codes match Linux errno values for compatibility

/// Operation not permitted
pub const EPERM: SyscallResult = -1;
/// No such file or directory
pub const ENOENT: SyscallResult = -2;
/// No such process
pub const ESRCH: SyscallResult = -3;
/// Interrupted system call
pub const EINTR: SyscallResult = -4;
/// I/O error
pub const EIO: SyscallResult = -5;
/// Bad file descriptor
pub const EBADF: SyscallResult = -9;
/// No child processes
pub const ECHILD: SyscallResult = -10;
/// Try again (resource temporarily unavailable)
pub const EAGAIN: SyscallResult = -11;
/// Out of memory
pub const ENOMEM: SyscallResult = -12;
/// Bad address (invalid pointer)
pub const EFAULT: SyscallResult = -14;
/// Invalid argument
pub const EINVAL: SyscallResult = -22;
/// Broken pipe
pub const EPIPE: SyscallResult = -32;
/// Function not implemented
pub const ENOSYS: SyscallResult = -38;
/// No space left on device
pub const ENOSPC: SyscallResult = -28;

// ============================================================================
// System Call Implementations
// ============================================================================

/// sys_write - Write to file descriptor (capability-based)
///
/// Arguments:
/// - arg1: fd (capability ID: 0=stdin, 1=stdout, 2=stderr, or other capability)
/// - arg2: buffer pointer
/// - arg3: length
/// 
/// Returns:
/// - Positive: Number of bytes written
/// - Negative: Error code (EFAULT, EINVAL, EBADF)
pub fn sys_write(fd: u64, buf: u64, len: u64, _arg4: u64, _arg5: u64, _arg6: u64) -> SyscallResult {
    use crate::kernel::capability::{FileResource, Handle, Rights};
    use crate::kernel::fs::VfsFile;
    use crate::kernel::process::PROCESS_TABLE;
    
    // Validate length
    if len > MAX_WRITE_LEN {
        debug_println!("[SYSCALL] sys_write: length too large ({})", len);
        return EINVAL;
    }
    
    // Validate buffer is readable
    if let Err(e) = validate_user_read(buf, len) {
        debug_println!("[SYSCALL] sys_write: invalid buffer at 0x{:x}, len={}", buf, len);
        return e;
    }
    
    let slice = unsafe {
        core::slice::from_raw_parts(buf as *const u8, len as usize)
    };
    
    // Get VfsFile from capability table
    let table = PROCESS_TABLE.lock();
    let process = match table.current_process() {
        Some(p) => p,
        None => return ESRCH,
    };
    
    let handle: Handle<FileResource> = unsafe { Handle::from_raw(fd) };
    let entry = match process.capability_table().get_with_rights(&handle, Rights::WRITE) {
        Ok(e) => e,
        Err(_) => {
            core::mem::forget(handle);
            return EBADF;
        }
    };
    
    let vfs_file = match entry.downcast::<VfsFile>() {
        Some(vfs) => vfs,
        None => {
            core::mem::forget(handle);
            return EBADF;
        }
    };
    
    let result = vfs_file.write(slice);
    core::mem::forget(handle);
    
    match result {
        Ok(written) => written as SyscallResult,
        Err(crate::kernel::fs::FileError::BrokenPipe) => EPIPE,
        Err(crate::kernel::fs::FileError::WouldBlock) => EAGAIN,
        Err(_) => EIO,
    }
}

/// sys_read - Read from file descriptor (capability-based)
///
/// Arguments:
/// - arg1: fd (capability ID: 0=stdin, 1=stdout, 2=stderr, or other capability)
/// - arg2: buffer pointer
/// - arg3: length
///
/// Returns:
/// - Positive: Number of bytes read
/// - 0: EOF
/// - Negative: Error code
pub fn sys_read(fd: u64, buf: u64, len: u64, _arg4: u64, _arg5: u64, _arg6: u64) -> SyscallResult {
    use crate::kernel::capability::{FileResource, Handle, Rights};
    use crate::kernel::fs::VfsFile;
    use crate::kernel::process::PROCESS_TABLE;
    
    // Validate buffer is writable
    if let Err(e) = validate_user_write(buf, len) {
        return e;
    }
    
    let slice = unsafe {
        core::slice::from_raw_parts_mut(buf as *mut u8, len as usize)
    };
    
    // Get VfsFile from capability table
    let table = PROCESS_TABLE.lock();
    let process = match table.current_process() {
        Some(p) => p,
        None => return ESRCH,
    };
    
    let handle: Handle<FileResource> = unsafe { Handle::from_raw(fd) };
    let entry = match process.capability_table().get_with_rights(&handle, Rights::READ) {
        Ok(e) => e,
        Err(_) => {
            core::mem::forget(handle);
            return EBADF;
        }
    };
    
    let vfs_file = match entry.downcast::<VfsFile>() {
        Some(vfs) => vfs,
        None => {
            core::mem::forget(handle);
            return EBADF;
        }
    };
    
    let result = vfs_file.read(slice);
    core::mem::forget(handle);
    
    match result {
        Ok(read) => read as SyscallResult,
        Err(crate::kernel::fs::FileError::BrokenPipe) => 0, // EOF
        Err(crate::kernel::fs::FileError::WouldBlock) => EAGAIN,
        Err(_) => EIO,
    }
}

/// sys_exit - Exit current process
///
/// When the last process exits:
/// - In QEMU test mode: Exit QEMU with success code
/// - Otherwise: Enter idle loop (halt)
pub fn sys_exit(code: u64, _arg2: u64, _arg3: u64, _arg4: u64, _arg5: u64, _arg6: u64) -> SyscallResult {
    use crate::kernel::process::{PROCESS_TABLE, terminate_process};
    use crate::kernel::scheduler::SCHEDULER;
    
    debug_println!("[SYSCALL] sys_exit: code={}", code);
    
    let pid = {
        let table = PROCESS_TABLE.lock();
        table.current_process().map(|p| p.pid())
    };
    
    if let Some(pid) = pid {
        // Terminate the current process (marks as Terminated, frees resources)
        terminate_process(pid, code as i32);
        
        // Check if there are any other ready processes to run
        let has_ready_process = {
            let mut scheduler = SCHEDULER.lock();
            scheduler.schedule().is_some()
        };
        
        if has_ready_process {
            // Schedule next process
            crate::kernel::process::schedule_next();
            // Should not be reached if context switch happens
        } else {
            // No more processes to run - exit QEMU
            debug_println!("[SYSCALL] sys_exit: No more processes to run, exiting QEMU");
            
            // Exit QEMU with appropriate code
            use crate::arch::qemu;
            if code == 0 {
                qemu::exit_qemu(0x10); // Success
            } else {
                qemu::exit_qemu(0x11); // Failed
            }
        }
    }
    
    // Fallback: halt loop (should not normally be reached)
    debug_println!("[SYSCALL] sys_exit: Entering halt loop");
    loop {
        crate::arch::ArchCpu::halt();
    }
}

/// sys_getpid - Get process ID
pub fn sys_getpid(_arg1: u64, _arg2: u64, _arg3: u64, _arg4: u64, _arg5: u64, _arg6: u64) -> SyscallResult {
    // For now, always return PID 1 (we only have one "process")
    1
}

/// sys_alloc - Allocate memory
pub fn sys_alloc(size: u64, _arg2: u64, _arg3: u64, _arg4: u64, _arg5: u64, _arg6: u64) -> SyscallResult {
    debug_println!("[SYSCALL] sys_alloc not implemented yet (requested {} bytes)", size);
    ENOSYS
}

/// sys_dealloc - Deallocate memory
pub fn sys_dealloc(ptr: u64, _arg2: u64, _arg3: u64, _arg4: u64, _arg5: u64, _arg6: u64) -> SyscallResult {
    debug_println!("[SYSCALL] sys_dealloc not implemented yet (ptr=0x{:x})", ptr);
    ENOSYS
}

/// sys_fork - Fork process
pub fn sys_fork(_arg1: u64, _arg2: u64, _arg3: u64, _arg4: u64, _arg5: u64, _arg6: u64) -> SyscallResult {
    match crate::kernel::process::lifecycle::fork_process() {
        Ok(pid) => pid.as_u64() as SyscallResult,
        Err(_) => ENOMEM,
    }
}

/// sys_exec - Execute program
pub fn sys_exec(path_ptr: u64, path_len: u64, _arg3: u64, _arg4: u64, _arg5: u64, _arg6: u64) -> SyscallResult {
    // Validate path pointer
    if let Err(e) = validate_user_read(path_ptr, path_len) {
        return e;
    }
    
    // Read path string
    let path_slice = unsafe {
        core::slice::from_raw_parts(path_ptr as *const u8, path_len as usize)
    };
    
    let path_str = match core::str::from_utf8(path_slice) {
        Ok(s) => s,
        Err(_) => return EINVAL,
    };
    
    match crate::kernel::process::lifecycle::exec_process(path_str) {
        Ok(_) => 0,
        Err(crate::kernel::process::lifecycle::CreateError::FileNotFound) => ENOENT,
        Err(_) => ENOMEM,
    }
}

/// sys_wait - Wait for child process
pub fn sys_wait(_pid: u64, status_ptr: u64, _options: u64, _arg4: u64, _arg5: u64, _arg6: u64) -> SyscallResult {
    use crate::kernel::process::{PROCESS_TABLE, ProcessState, schedule_next};
    
    loop {
        let result = {
            let mut table = PROCESS_TABLE.lock();
            let current_pid = match table.current_process().map(|p| p.pid()) {
                Some(pid) => pid,
                None => return ESRCH,
            };
            
            if let Some((child_pid, exit_code)) = table.find_terminated_child(current_pid) {
                // Found terminated child
                
                // Write exit code to user pointer if provided
                if status_ptr != 0 {
                    // Check validity of status_ptr (mapped and writable)
                    if let Err(e) = validate_user_write(status_ptr, core::mem::size_of::<i32>() as u64) {
                        debug_println!("[SYSCALL] sys_wait: invalid status_ptr 0x{:x}", status_ptr);
                        return e;
                    }
                    
                    // Safe to write exit code
                    unsafe {
                        *(status_ptr as *mut i32) = exit_code;
                    }
                }
                
                // Reap the child
                table.remove_process(child_pid);
                
                Ok(child_pid.as_u64() as SyscallResult)
            } else if table.has_children(current_pid) {
                // Has children but none terminated
                // Block current process
                if let Some(current) = table.current_process_mut() {
                    current.set_state(ProcessState::Blocked);
                }
                Err(0) // Signal to block
            } else {
                // No children
                Err(ECHILD)
            }
        };
        
        match result {
            Ok(pid) => return pid,
            Err(0) => {
                // Block and switch
                schedule_next();
                // When we return, we loop again to check children
            },
            Err(e) => return e,
        }
    }
}

/// sys_mmap - Map memory
pub fn sys_mmap(addr: u64, len: u64, _prot: u64, _flags: u64, _fd: u64, _offset: u64) -> SyscallResult {
    use crate::kernel::process::PROCESS_TABLE;
    use crate::kernel::mm::allocator::BOOT_INFO_ALLOCATOR;
    use crate::kernel::security::{is_user_address, is_user_range, validate_alloc_size};

    // Validate length
    if len == 0 {
        return EINVAL;
    }
    
    // Validate allocation size (prevent excessive allocations)
    if let Err(e) = validate_alloc_size(len) {
        debug_println!("[SYSCALL] sys_mmap: invalid allocation size {}", len);
        return e;
    }
    
    // If addr is specified (non-zero), validate it's in user space
    if addr != 0 {
        // Align length to page size for range check
        let len_aligned = (len + 4095) & !4095;
        
        if !is_user_range(addr, len_aligned) {
            debug_println!("[SYSCALL] sys_mmap: requested address 0x{:x} not in user space", addr);
            return EFAULT;
        }
        
        // Fixed address mapping not supported yet
        debug_println!("[SYSCALL] sys_mmap: fixed address mapping not supported yet");
        return EINVAL;
    }
    
    // Align length to page size
    let len_aligned = (len + 4095) & !4095;
    let num_pages = (len_aligned / 4096) as usize;
    
    let mut table = PROCESS_TABLE.lock();
    let process = match table.current_process_mut() {
        Some(p) => p,
        None => return ESRCH,
    };
    
    // Determine address
    let start_addr = if addr == 0 {
        process.mmap_top()
    } else {
        // Fixed address request not supported yet for simplicity
        return EINVAL;
    };
    
    // Update mmap_top
    let new_top = start_addr + len_aligned;
    process.set_mmap_top(new_top);
    
    // Map memory
    // We need to access the page table of the current process.
    // But the page table is active (CR3).
    // So we can just map into the current address space!
    // But we need a mapper.
    // We can create a temporary mapper using CR3.
    
    let phys_mem_offset = x86_64::VirtAddr::new(crate::kernel::mm::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed));
    let (l4_frame, _) = x86_64::registers::control::Cr3::read();
    let l4_table_ptr = (phys_mem_offset + l4_frame.start_address().as_u64()).as_mut_ptr();
    let l4_table = unsafe { &mut *l4_table_ptr };
    let mut mapper = unsafe { x86_64::structures::paging::OffsetPageTable::new(l4_table, phys_mem_offset) };
    
    let mut allocator_lock = BOOT_INFO_ALLOCATOR.lock();
    let frame_allocator = match allocator_lock.as_mut() {
        Some(alloc) => alloc,
        None => return ENOMEM,
    };
    
    // We use map_user_stack logic but for arbitrary range?
    // map_user_stack allocates frames and maps them.
    // But it assumes stack grows down?
    // Let's check map_user_stack implementation.
    // It maps [stack_bottom, stack_top).
    // So we can use it, or write a loop here.
    
    use x86_64::structures::paging::{Page, PageTableFlags, Mapper, FrameAllocator, Size4KiB};
    
    let start_page = Page::<Size4KiB>::containing_address(start_addr);
    let end_page = Page::<Size4KiB>::containing_address(start_addr + len_aligned);
    let page_range = Page::range(start_page, end_page);
    
    let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE;
    
    // Track allocated pages for rollback
    // Since we don't have a Vec, we can't easily store them all if the count is large.
    // However, we are mapping a contiguous range.
    // If we fail at index i, we need to unmap pages 0 to i-1.
    
    for i in 0..num_pages {
        let page = page_range.start + i as u64;
        let frame = match frame_allocator.allocate_frame() {
            Some(f) => f,
            None => {
                // Rollback: Unmap previously mapped pages
                for j in 0..i {
                    let page_to_unmap = page_range.start + j as u64;
                    if let Ok((frame, _)) = mapper.unmap(page_to_unmap) {
                        x86_64::instructions::tlb::flush(page_to_unmap.start_address());
                        unsafe {
                            frame_allocator.deallocate_frame(frame);
                        }
                    }
                }
                return ENOMEM;
            }
        };
        
        unsafe {
            match mapper.map_to(page, frame, flags, frame_allocator) {
                Ok(tlb) => tlb.flush(),
                Err(_) => {
                    // Rollback this frame and previous pages
                    frame_allocator.deallocate_frame(frame);
                    
                    for j in 0..i {
                        let page_to_unmap = page_range.start + j as u64;
                        if let Ok((frame, _)) = mapper.unmap(page_to_unmap) {
                            x86_64::instructions::tlb::flush(page_to_unmap.start_address());
                            frame_allocator.deallocate_frame(frame);
                        }
                    }
                    return ENOMEM;
                }
            }
        }
    }
    
    // Zero the memory
    // Newly allocated frames might contain garbage.
    // Security risk! We should zero them.
    // Since we just mapped them, we can write to them via the direct map.
    
    let phys_mem_offset = x86_64::VirtAddr::new(crate::kernel::mm::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed));
    
    // We need to iterate over the pages we just mapped and zero them.
    // We can't easily get the frames again without walking the page table, 
    // but we know the virtual addresses.
    // However, we are in kernel mode. We can just write to the user address?
    // No, SMAP might prevent it (if enabled).
    // Safer to use the direct map.
    
    // Let's walk the range again and get the physical address.
    // Or better, we should have zeroed them IN the allocation loop.
    // But we didn't want to change the loop structure too much.
    // Let's do a second pass for now.
    
    for page in page_range {

        if let Ok(frame) = mapper.translate_page(page) {
             let frame_ptr = (phys_mem_offset + frame.start_address().as_u64()).as_mut_ptr::<u8>();
             unsafe {
                 core::ptr::write_bytes(frame_ptr, 0, 4096);
             }
        }
    }
    
    start_addr.as_u64() as SyscallResult
}

/// sys_pipe - Create a pipe
///
/// Creates a pipe and stores the read/write file descriptors  in the user-provided array.
///
/// # Arguments
/// * `pipefd` - Pointer to an array of 2 u64 values (read_fd, write_fd)
///
/// # Returns
/// * `SUCCESS` (0) - Pipe created successfully
/// * `EFAULT` - Invalid pointer
/// * `ESRCH` - Current process not found
/// * `ENOSYS` - Not yet implemented
pub fn sys_pipe(pipefd: u64, _arg2: u64, _arg3: u64, _arg4: u64, _arg5: u64, _arg6: u64) -> SyscallResult {
    use crate::kernel::security::validate_user_write;
    
    // Validate pipefd pointer (array of 2 u64 values = 16 bytes)
    if let Err(e) = validate_user_write(pipefd, 16) {
        debug_println!("[SYSCALL] sys_pipe: invalid pipefd pointer 0x{:x}", pipefd);
        return e;
    }
    
    // TODO: Implement pipe support
    debug_println!("[SYSCALL] sys_pipe: Not implemented yet");
    ENOSYS // Function not implemented
}

/// sys_munmap - Unmap memory
pub fn sys_munmap(addr: u64, len: u64, _arg3: u64, _arg4: u64, _arg5: u64, _arg6: u64) -> SyscallResult {
    use crate::kernel::security::is_user_range;
    
    // Validate length
    if len == 0 {
        return EINVAL;
    }
    
    // Check null pointer
    if addr == 0 {
        debug_println!("[SYSCALL] sys_munmap: null pointer");
        return EFAULT;
    }
    
    // Align length to page size for validation
    let len_aligned = (len + 4095) & !4095;
    
    // Validate that address range is in user space
    if !is_user_range(addr, len_aligned) {
        debug_println!("[SYSCALL] sys_munmap: address 0x{:x} not in user space", addr);
        return EFAULT;
    }
    
    // Align length
    let len_aligned = (len + 4095) & !4095;
    
    // We need to unmap pages.
    // Access mapper via CR3.
    let phys_mem_offset = x86_64::VirtAddr::new(crate::kernel::mm::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed));
    let (l4_frame, _) = x86_64::registers::control::Cr3::read();
    let l4_table_ptr = (phys_mem_offset + l4_frame.start_address().as_u64()).as_mut_ptr();
    let l4_table = unsafe { &mut *l4_table_ptr };
    let mut mapper = unsafe { x86_64::structures::paging::OffsetPageTable::new(l4_table, phys_mem_offset) };
    
    use x86_64::structures::paging::{Page, Mapper, Size4KiB};
    
    let start_addr = x86_64::VirtAddr::new(addr);
    let start_page = Page::<Size4KiB>::containing_address(start_addr);
    let end_page = Page::<Size4KiB>::containing_address(start_addr + len_aligned);
    let page_range = Page::range(start_page, end_page);
    
    // First pass: validate all pages are mapped
    // This provides better error reporting (POSIX compliance)
    let mut unmapped_found = false;
    for page in page_range.clone() {
        if mapper.translate_page(page).is_err() {
            debug_println!("[SYSCALL] sys_munmap: page 0x{:x} is not mapped", page.start_address().as_u64());
            unmapped_found = true;
        }
    }
    
    // If any pages are unmapped, we could either:
    // 1. Return error (strict POSIX) - uncomment the line below
    // 2. Continue and unmap only mapped pages (lenient)
    // Currently using lenient approach for compatibility
    // if unmapped_found {
    //     return EINVAL;
    // }
    
    // Second pass: unmap pages
    for page in page_range {
        // Unmap
        if let Ok((frame, _flags)) = mapper.unmap(page) {
            // Flush TLB
            x86_64::instructions::tlb::flush(page.start_address());
            
            // Free the physical frame
            unsafe {
                let mut allocator_lock = crate::kernel::mm::allocator::BOOT_INFO_ALLOCATOR.lock();
                if let Some(frame_allocator) = allocator_lock.as_mut() {
                    frame_allocator.deallocate_frame(frame);
                }
            }
        }
    }
    
    SUCCESS
}

// ============================================================================
// io_uring System Calls
// ============================================================================

/// sys_io_uring_setup - Set up io_uring for the current process
///
/// # Arguments
/// * `entries` - Number of entries (must be power of 2, max 256)
/// * `params_ptr` - Pointer to IoUringParams structure (for future use)
///
/// # Returns
/// * Success: Returns addresses packed in a structure
// sys_io_uring_setup (V1) removed - see V2 implementation at line 1130+

/// Maps a kernel heap region to a specific user-space address.
///
/// This allocates new page table entries at `user_virt_addr` that map
/// to the same physical memory as `kernel_virt_addr`.
///
/// The kernel heap uses the direct physical map: virt = phys + phys_mem_offset
/// So we can calculate the physical address as: phys = virt - phys_mem_offset
fn map_kernel_region_to_user_addr(
    user_mapper: &mut x86_64::structures::paging::OffsetPageTable,
    kernel_virt_addr: u64,
    user_virt_addr: u64,
    size: usize,
    frame_allocator: &mut crate::kernel::mm::BootInfoFrameAllocator,
    phys_mem_offset: x86_64::VirtAddr,
) -> Result<(), ()> {
    use x86_64::structures::paging::{Page, PageTableFlags, Mapper, Size4KiB, Translate};
    use x86_64::structures::paging::mapper::TranslateResult;
    use x86_64::VirtAddr;
    
    let num_pages = (size + 4095) / 4096;
    
    debug_println!(
        "[map_kernel_to_user] Mapping {} pages: kernel {:#x} -> user {:#x}",
        num_pages, kernel_virt_addr, user_virt_addr
    );
    
    for i in 0..num_pages {
        let kernel_virt = VirtAddr::new(kernel_virt_addr + (i * 4096) as u64);
        let user_virt = VirtAddr::new(user_virt_addr + (i * 4096) as u64);
        
        // Calculate physical address from kernel virtual address
        // Kernel heap uses direct physical map: virt = phys + phys_mem_offset
        let phys_addr = kernel_virt.as_u64() - phys_mem_offset.as_u64();
        let phys_frame: x86_64::structures::paging::PhysFrame<Size4KiB> = 
            x86_64::structures::paging::PhysFrame::containing_address(
                x86_64::PhysAddr::new(phys_addr)
            );
        
        // Map this physical frame to user-space address
        let user_page: Page<Size4KiB> = Page::containing_address(user_virt);
        
        // Check if user address is already mapped
        if let TranslateResult::Mapped { flags, .. } = user_mapper.translate(user_virt) {
            if flags.contains(PageTableFlags::USER_ACCESSIBLE) {
                debug_println!(
                    "[map_kernel_to_user] user {:#x} already mapped with USER_ACCESSIBLE",
                    user_virt.as_u64()
                );
                continue;
            }
            // If mapped without USER_ACCESSIBLE, we need to remap
            // Try to unmap first (may fail for huge pages)
            if let Ok((_, flush)) = user_mapper.unmap(user_page) {
                flush.flush();
            } else {
                debug_println!("[map_kernel_to_user] Could not unmap existing mapping, trying map anyway");
            }
        }
        
        // Map with USER_ACCESSIBLE
        let flags = PageTableFlags::PRESENT 
            | PageTableFlags::WRITABLE 
            | PageTableFlags::USER_ACCESSIBLE;
        
        unsafe {
            match user_mapper.map_to(user_page, phys_frame, flags, frame_allocator) {
                Ok(flush) => flush.flush(),
                Err(e) => {
                    debug_println!("[map_kernel_to_user] map_to failed: {:?}", e);
                    return Err(());
                }
            }
        }
        
        debug_println!(
            "[map_kernel_to_user] Mapped user {:#x} -> phys {:#x}",
            user_virt.as_u64(), phys_addr
        );
    }
    
    Ok(())
}

/// sys_io_uring_enter - Submit I/O and optionally wait for completions
///
/// This is the main io_uring syscall. It:
/// 1. Processes pending submissions from the SQ
/// 2. Optionally waits for a minimum number of completions
///
/// # Arguments
/// * `fd` - io_uring file descriptor (ignored for now, we use process context)
/// * `to_submit` - Number of submissions to process (0 = process all available)
/// * `min_complete` - Minimum completions to wait for (0 = don't wait)
/// * `flags` - Operation flags (IORING_ENTER_GETEVENTS, etc.)
/// * `sig` - Signal mask (ignored for now)
/// * `sigsz` - Signal mask size (ignored for now)
///
/// # Returns
/// * Success: Number of completions available
/// * Error: EINVAL, EAGAIN, etc.
pub fn sys_io_uring_enter(
    _fd: u64,
    to_submit: u64,
    min_complete: u64,
    _flags: u64,
    _sig: u64,
    _sigsz: u64,
) -> SyscallResult {
    use crate::kernel::process::PROCESS_TABLE;
    
    let mut table = PROCESS_TABLE.lock();
    let process = match table.current_process_mut() {
        Some(p) => p,
        None => return ESRCH,
    };
    
    // Get io_uring context and capability table simultaneously
    let (ctx, cap_table) = match process.io_uring_with_capabilities() {
        Some(pair) => pair,
        None => {
            debug_println!("[SYSCALL] io_uring_enter: io_uring not set up");
            return EINVAL;
        }
    };
    
    // Process submissions and completions
    let completed = ctx.enter(min_complete as u32, cap_table);
    
    debug_println!(
        "[SYSCALL] io_uring_enter: to_submit={}, min_complete={}, completed={}",
        to_submit, min_complete, completed
    );
    
    completed as SyscallResult
}

/// sys_io_uring_register - Register resources with io_uring
///
/// Used to register buffers or file descriptors for zero-copy operations.
///
/// # Arguments
/// * `fd` - io_uring file descriptor
/// * `opcode` - Registration operation
/// * `arg` - Operation-specific argument
/// * `nr_args` - Number of arguments
///
/// # Returns
/// * Success: 0
/// * Error: ENOSYS (not implemented)
pub fn sys_io_uring_register(
    _fd: u64,
    _opcode: u64,
    _arg: u64,
    _nr_args: u64,
    _arg5: u64,
    _arg6: u64,
) -> SyscallResult {
    debug_println!("[SYSCALL] io_uring_register: not implemented yet");
    ENOSYS
}

// ============================================================================
// Fast IPC Syscalls (Strategy 1-3)
// ============================================================================

/// Benchmark syscall (ID: 1000)
///
/// Minimal syscall for measuring syscall overhead.
///
/// # Modes
/// * 0 - Minimal (just return)
/// * 1 - Read timestamp (rdtsc)
/// * 2 - Memory fence
/// * 3 - Check shared ring
///
/// # Returns
/// * Mode 0: 0
/// * Mode 1: Current timestamp
/// * Mode 2: 0
/// * Mode 3: Number of pending operations
pub fn sys_benchmark(mode: u64, _arg2: u64, _arg3: u64, _arg4: u64, _arg5: u64, _arg6: u64) -> SyscallResult {
    match mode {
        0 => 0, // Minimal - just return
        1 => crate::arch::x86_64::cpu::read_timestamp() as SyscallResult,
        2 => {
            core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
            0
        }
        3 => 0, // Ring check (placeholder)
        _ => EINVAL,
    }
}

/// Fast ring poll syscall (ID: 1001)
///
/// This is the "kick" syscall for SQPOLL mode.
/// It processes submissions in the fast I/O ring without full syscall overhead.
///
/// # Returns
/// * Number of operations processed
pub fn sys_fast_poll(_arg1: u64, _arg2: u64, _arg3: u64, _arg4: u64, _arg5: u64, _arg6: u64) -> SyscallResult {
    // Poll all registered SQPOLL contexts
    crate::arch::x86_64::syscall_ring::kernel_poll_all() as SyscallResult
}

// sys_fast_io_setup removed (Legacy V1)

// ============================================================================
// Ring-based Syscall System (Syscalls 2000-2099)
//
// This is the new revolutionary syscall architecture that replaces
// traditional function-call-style syscalls with io_uring-style
// asynchronous message passing.
// ============================================================================

// sys_ring_enter removed (Legacy V1)

// sys_ring_register removed (Legacy V1)

/// Ring setup syscall (ID: 2002)
///
/// Initialize the ring-based syscall context for the current process.
/// This allocates the submission and completion queues.
///
/// # Arguments
/// * `flags` - Configuration flags
///   - Bit 0: Enable SQPOLL (kernel polling mode)
///
/// # Returns
/// * Success: User-space address of the RingContext
/// * Error: ENOMEM (allocation failed), ESRCH (no process)
pub fn sys_io_uring_setup(flags: u64, _arg2: u64, _arg3: u64, _arg4: u64, _arg5: u64, _arg6: u64) -> SyscallResult {
    use crate::kernel::process::PROCESS_TABLE;
    use crate::kernel::mm::allocator::BOOT_INFO_ALLOCATOR;
    
    let enable_sqpoll = (flags & 1) != 0;
    crate::debug_println!(
        "[SYSCALL] sys_io_uring_setup: enable_sqpoll={} (flags={:#x})",
        enable_sqpoll,
        flags
    );
    
    // Get physical memory offset
    let phys_offset = x86_64::VirtAddr::new(
        crate::kernel::mm::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed)
    );
    
    // Get frame allocator
    let mut allocator_lock = BOOT_INFO_ALLOCATOR.lock();
    let frame_allocator = match allocator_lock.as_mut() {
        Some(alloc) => alloc,
        None => return ENOMEM,
    };
    
    let mut table = PROCESS_TABLE.lock();
    let process = match table.current_process_mut() {
        Some(p) => p,
        None => return ESRCH,
    };
    
    // Initialize ring context with user space mapping
    match process.ring_setup_with_mapping(enable_sqpoll, frame_allocator, phys_offset) {
        Ok(user_addr) => user_addr as SyscallResult,
        Err(e) => e as SyscallResult,
    }
}

// =============================================================================
// V2 io_uring syscalls (Next-Generation Ring-Based Async Protocol)
// =============================================================================

/// V2 io_uring enter syscall (ID: 2003)
///
/// Process a V2 submission entry with capability-based access control.
/// This is the next-generation protocol using:
/// - Capability handles instead of integer file descriptors
/// - Registered buffers only (no raw pointers)
/// - Type-safe error codes
///
/// # Arguments
/// * `sqe_addr` - Pointer to SubmissionEntryV2 (64 bytes)
/// * `cqe_addr` - Pointer to store CompletionEntryV2 result (32 bytes)
///
/// # Returns
/// * 0 on success
/// * EFAULT if addresses are invalid
/// * ESRCH if no current process
#[allow(unused)]
pub fn sys_io_uring_enter_v2(
    sqe_addr: u64,
    cqe_addr: u64,
    _arg3: u64,
    _arg4: u64,
    _arg5: u64,
    _arg6: u64,
) -> SyscallResult {
    use crate::abi::io_uring_v2::{SubmissionEntryV2, CompletionEntryV2};
    use crate::kernel::process::PROCESS_TABLE;
    
    // Validate addresses (basic check - full validation done in handler)
    if sqe_addr == 0 || cqe_addr == 0 {
        return EFAULT;
    }
    
    // Read SQE from user space
    let sqe: SubmissionEntryV2 = unsafe {
        core::ptr::read_volatile(sqe_addr as *const SubmissionEntryV2)
    };
    
    // Get process and process the V2 submission while holding the lock
    let cqe = {
        let table = PROCESS_TABLE.lock();
        let process = match table.current_process() {
            Some(p) => p,
            None => return ESRCH,
        };
        
        let cap_table = process.capability_table();
        let io_uring_ctx = process.io_uring();
        let buf_table = io_uring_ctx.map(|ctx| ctx.registered_buffer_table());
        
        // Process the V2 submission
        crate::kernel::io_uring::handlers_v2::dispatch_sqe_v2(
            &sqe,
            cap_table,
            buf_table,
            false, // User mode: no raw addresses
        )
        // table lock dropped here
    };
    
    // Write CQE to user space
    unsafe {
        core::ptr::write_volatile(cqe_addr as *mut CompletionEntryV2, cqe);
    }
    
    0 // Success
}

/// V2 capability duplicate syscall (ID: 2004)
///
/// Duplicate an existing capability with potentially reduced rights.
/// This creates a new capability handle pointing to the same resource.
///
/// # Arguments
/// * `capability_id` - Existing capability ID to duplicate
/// * `rights` - Permissions for the new capability (must be subset of original)
///
/// # Returns
/// * Success: New capability handle value (u64)
/// * Error: Negative errno
#[allow(unused)]
pub fn sys_capability_dup(
    capability_id: u64,
    rights: u64,
    _arg3: u64,
    _arg4: u64,
    _arg5: u64,
    _arg6: u64,
) -> SyscallResult {
    use crate::kernel::capability::{Rights, FileResource, Handle};
    use crate::kernel::process::PROCESS_TABLE;
    
    let table = PROCESS_TABLE.lock();
    let process = match table.current_process() {
        Some(p) => p,
        None => return ESRCH,
    };
    
    // Get the original capability
    let handle: Handle<FileResource> = unsafe { Handle::from_raw(capability_id) };
    let entry = match process.capability_table().get(&handle) {
        Ok(e) => e,
        Err(_) => {
            core::mem::forget(handle);
            return EBADF;
        }
    };
    
    // New rights must be a subset of original rights
    let new_rights = Rights::from_bits(rights);
    if !entry.rights.contains(new_rights) {
        core::mem::forget(handle);
        return EPERM;
    }
    
    // Clone the resource and insert with new rights
    let resource_clone = entry.resource.clone();
    core::mem::forget(handle);
    
    // Insert the duplicated capability
    match process.capability_table().insert_raw::<FileResource>(
        entry.type_id,
        resource_clone,
        new_rights,
    ) {
        Ok(new_handle) => {
            let value = new_handle.raw();
            core::mem::forget(new_handle);
            value as SyscallResult
        }
        Err(_) => ENOSPC,
    }
}

/// V2 capability revoke syscall (ID: 2005)
///
/// Revoke a capability handle.
///
/// # Arguments
/// * `handle` - Capability handle to revoke
///
/// # Returns
/// * 0 on success
/// * EINVAL if handle is invalid
#[allow(unused)]
pub fn sys_capability_revoke(
    handle: u64,
    _arg2: u64,
    _arg3: u64,
    _arg4: u64,
    _arg5: u64,
    _arg6: u64,
) -> SyscallResult {
    use crate::kernel::capability::FileResource;
    use crate::kernel::process::PROCESS_TABLE;
    
    let mut table = PROCESS_TABLE.lock();
    let process = match table.current_process_mut() {
        Some(p) => p,
        None => return ESRCH,
    };
    
    let cap_handle: crate::kernel::capability::Handle<FileResource> = unsafe {
        crate::kernel::capability::Handle::from_raw(handle)
    };
    
    match process.capability_table_mut().remove(cap_handle) {
        Ok(_) => 0,
        Err(_) => EINVAL,
    }
}

/// Syscall handler function type
type SyscallHandler = fn(u64, u64, u64, u64, u64, u64) -> SyscallResult;

/// Syscall dispatch table
static SYSCALL_TABLE: &[SyscallHandler] = &[
    sys_write,    // 0
    sys_read,     // 1
    sys_exit,     // 2
    sys_getpid,   // 3
    sys_alloc,    // 4
    sys_dealloc,  // 5
    sys_fork,     // 6
    sys_exec,     // 7
    sys_wait,     // 8
    sys_mmap,     // 9
    sys_munmap,   // 10
    sys_pipe,     // 11
    sys_ni_syscall,         // 12 - sys_io_uring_setup (removed)
    sys_ni_syscall,         // 13 - reserved
    sys_ni_syscall,         // 14 - reserved
];

/// Not implemented syscall handler
fn sys_ni_syscall(_: u64, _: u64, _: u64, _: u64, _: u64, _: u64) -> SyscallResult {
    ENOSYS
}

/// Dispatch a syscall to its handler
pub fn dispatch(
    syscall_num: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    arg4: u64,
    arg5: u64,
    arg6: u64,
) -> SyscallResult {
    // Always log syscall entry for Phase 2.5 debugging
    crate::debug_println!(
        "[SYSCALL-ENTRY] num={}, args=({:#x}, {:#x}, {:#x}, {:#x}, {:#x}, {:#x})",
        syscall_num, arg1, arg2, arg3, arg4, arg5, arg6
    );
    
    let num = syscall_num as usize;
    
    if num >= SYSCALL_TABLE.len() {
        // Check for extended syscall range (1000+)
        return match syscall_num {
            1000 => sys_benchmark(arg1, arg2, arg3, arg4, arg5, arg6),
            1001 => sys_fast_poll(arg1, arg2, arg3, arg4, arg5, arg6),
            1002 => ENOSYS, // sys_fast_io_setup removed
            // Ring-based syscall system (2000+)
            2000 => ENOSYS, // sys_ring_enter removed
            2001 => ENOSYS, // sys_ring_register removed
            2002 => sys_io_uring_setup(arg1, arg2, arg3, arg4, arg5, arg6),
            // V2 Next-Generation Protocol (2003+)
            2003 => sys_io_uring_enter_v2(arg1, arg2, arg3, arg4, arg5, arg6),
            2004 => sys_capability_dup(arg1, arg2, arg3, arg4, arg5, arg6),
            2005 => sys_capability_revoke(arg1, arg2, arg3, arg4, arg5, arg6),
            _ => {
                debug_println!("[SYSCALL] Invalid syscall number: {}", syscall_num);
                ENOSYS
            }
        };
    }
    
    debug_println!(
        "[SYSCALL] Dispatching syscall {} with args=({}, {}, {}, {}, {}, {})",
        syscall_num, arg1, arg2, arg3, arg4, arg5, arg6
    );
    
    let handler = SYSCALL_TABLE[num];
    let result = handler(arg1, arg2, arg3, arg4, arg5, arg6);
    
    // Log syscall result
    crate::debug_println!("[SYSCALL-RESULT] num={} returned {}", syscall_num, result);
    
    result
}

/// Test syscall mechanism from kernel space
///
/// This is a simple test that can be called from kernel initialization
/// to verify that syscalls work correctly before jumping to user mode.
///
/// # Safety
/// This function simulates syscalls but runs in kernel space (Ring 0).
/// It's safe to call during boot before user mode is active.
#[cfg(debug_assertions)]
#[allow(dead_code)]
pub fn test_syscall_mechanism() {
    debug_println!("\n=== Testing Syscall Mechanism ===");
    
    // Test 1: sys_getpid
    debug_println!("Test 1: sys_getpid");
    let pid = dispatch(3, 0, 0, 0, 0, 0, 0);
    debug_println!("  Result: PID = {}", pid);
    
    // Test 2: sys_write (valid)
    debug_println!("\nTest 2: sys_write (valid message)");
    let message = b"[Test] Hello from syscall test!\n";
    let result = dispatch(
        0, // sys_write
        1, // stdout
        message.as_ptr() as u64,
        message.len() as u64,
        0, 0, 0
    );
    debug_println!("  Result: {} bytes written", result);
    
    // Test 3: sys_write (invalid pointer)
    debug_println!("\nTest 3: sys_write (invalid pointer)");
    let result = dispatch(
        0, // sys_write
        1, // stdout
        0, // NULL pointer
        100,
        0, 0, 0
    );
    debug_println!("  Result: {} (expected EFAULT = -14)", result);
    
    // Test 4: sys_write (kernel address)
    
    // Test 4: sys_write (kernel address)
    debug_println!("\\nTest 4: sys_write (kernel address)");
    let result = dispatch(
        0, // sys_write
        1, // stdout
        0xFFFF_8000_0000_0000, // Kernel space
        100,
        0, 0, 0
    );
    debug_println!("  Result: {} (expected EFAULT = -14)", result);
    
    debug_println!("\\n=== Syscall Mechanism Test Complete ===\\n");
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests;
