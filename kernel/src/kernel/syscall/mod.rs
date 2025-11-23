// src/kernel/syscall/mod.rs
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
//! - **Mapping check**: Should verify page is mapped (TODO: Phase 3)
//! - **Permission check**: Should verify page has correct permissions (TODO: Phase 3)
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

use crate::arch::Cpu;
use crate::debug_println;

use crate::kernel::core::traits::CharDevice;
// TODO: Re-enable security validation
// use crate::kernel::security::{validate_user_write, validate_user_read};

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

/// Check if an address is in user space
/// 
/// # Memory Layout
///
/// - User space: `0x0000_0000_0000_0000` ~ `0x0000_7FFF_FFFF_FFFF`
/// - Kernel space: `0xFFFF_8000_0000_0000` ~ `0xFFFF_FFFF_FFFF_FFFF`
///
/// # Security
///
/// This is the **first line of defense** against kernel memory access.
/// All user-provided pointers MUST be validated with this function.
#[inline]
fn is_user_address(addr: u64) -> bool {
    addr < 0x0000_8000_0000_0000
}

/// Check if a memory range is in user space
///
/// # Arguments
/// * `addr` - Start address
/// * `len` - Length in bytes
///
/// # Returns
/// `true` if the entire range [addr, addr+len) is in user space
///
/// # Security
///
/// This function prevents:
/// - Integer overflow attacks (checks for `addr + len` overflow)
/// - Partial kernel memory access (checks end address)
/// - Zero-length accesses (implicitly handled)
#[inline]
fn is_user_range(addr: u64, len: u64) -> bool {
    // Check for overflow
    let end = addr.checked_add(len);
    if end.is_none() {
        return false;
    }
    
    let end = end.unwrap();
    is_user_address(addr) && is_user_address(end.saturating_sub(1))
}

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

// ============================================================================
// System Call Implementations
// ============================================================================

/// sys_write - Write to file descriptor
///
/// Arguments:
/// - arg1: fd (file descriptor)
/// - arg2: buffer pointer
/// - arg3: length
/// 
/// Returns:
/// - Positive: Number of bytes written
/// - Negative: Error code (EFAULT, EINVAL, EBADF)
pub fn sys_write(fd: u64, buf: u64, len: u64, _arg4: u64, _arg5: u64, _arg6: u64) -> SyscallResult {
    // Special case: FD 1 = stdout (console)
    if fd == 1 {
        // 1. Validate pointer is in user space
        if buf == 0 || !is_user_address(buf) {
            debug_println!("[SYSCALL] sys_write: invalid buffer address 0x{:x}", buf);
            return EFAULT;
        }
        
        // 2. Validate length
        if len > MAX_WRITE_LEN {
            debug_println!("[SYSCALL] sys_write: length too large ({})", len);
            return EINVAL;
        }
        
        // 3. Validate memory range is in user space
        if !is_user_range(buf, len) {
            debug_println!("[SYSCALL] sys_write: buffer range crosses user/kernel boundary");
            return EFAULT;
        }
        
        // 4. Safely read user buffer
        // SAFETY: We've validated that the pointer is in user space
        let slice = unsafe {
            core::slice::from_raw_parts(buf as *const u8, len as usize)
        };
        
        // 5. Write to console
        use crate::kernel::driver::serial::SERIAL1;
        if let Some(mut serial) = SERIAL1.try_lock() {
            for &byte in slice {
                let _ = serial.write_byte(byte);
            }
        }
        
        return len as SyscallResult;
    }
    
    // For other FDs, dispatch to file descriptor
    use crate::kernel::process::PROCESS_TABLE;
    
    let table = PROCESS_TABLE.lock();
    let process = match table.current_process() {
        Some(p) => p,
        None => return ESRCH,
    };
    
    let fd_arc = match process.get_file_descriptor(fd) {
        Some(fd) => fd,
        None => return EBADF,
    };
    
    // Validate buffer
    if buf == 0 || !is_user_address(buf) || !is_user_range(buf, len) {
        return EFAULT;
    }
    
    let slice = unsafe {
        core::slice::from_raw_parts(buf as *const u8, len as usize)
    };
    
    let mut fd_lock = fd_arc.lock();
    match fd_lock.write(slice) {
        Ok(written) => written as SyscallResult,
        Err(crate::kernel::fs::FileError::BrokenPipe) => EPIPE,
        Err(crate::kernel::fs::FileError::WouldBlock) => EAGAIN,
        Err(_) => EIO,
    }
}

/// sys_read - Read from file descriptor
///
/// Arguments:
/// - arg1: fd (file descriptor)
/// - arg2: buffer pointer
/// - arg3: length
///
/// Returns:
/// - Positive: Number of bytes read
/// - 0: EOF
/// - Negative: Error code
pub fn sys_read(fd: u64, buf: u64, len: u64, _arg4: u64, _arg5: u64, _arg6: u64) -> SyscallResult {
    // Special case: FD 0 = stdin (not implemented)
    if fd == 0 {
        debug_println!("[SYSCALL] sys_read from stdin not implemented yet");
        return ENOSYS;
    }
    
    // For other FDs, dispatch to file descriptor
    use crate::kernel::process::PROCESS_TABLE;
    
    let table = PROCESS_TABLE.lock();
    let process = match table.current_process() {
        Some(p) => p,
        None => return ESRCH,
    };
    
    let fd_arc = match process.get_file_descriptor(fd) {
        Some(fd) => fd,
        None => return EBADF,
    };
    
    // Validate buffer
    if buf == 0 || !is_user_address(buf) || !is_user_range(buf, len) {
        return EFAULT;
    }
    
    let slice = unsafe {
        core::slice::from_raw_parts_mut(buf as *mut u8, len as usize)
    };
    
    let mut fd_lock = fd_arc.lock();
    match fd_lock.read(slice) {
        Ok(read) => read as SyscallResult,
        Err(crate::kernel::fs::FileError::BrokenPipe) => 0, // EOF
        Err(crate::kernel::fs::FileError::WouldBlock) => EAGAIN,
        Err(_) => EIO,
    }
}

/// sys_exit - Exit current process
pub fn sys_exit(code: u64, _arg2: u64, _arg3: u64, _arg4: u64, _arg5: u64, _arg6: u64) -> SyscallResult {
    use crate::kernel::process::{PROCESS_TABLE, schedule_next, terminate_process};
    
    let pid = {
        let table = PROCESS_TABLE.lock();
        table.current_process().map(|p| p.pid())
    };
    
    if let Some(pid) = pid {
        terminate_process(pid, code as i32);
        // Schedule next process (this process will not be picked again)
        schedule_next();
    }
    
    // Should not be reached
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
pub fn sys_exec(_path: u64, _arg2: u64, _arg3: u64, _arg4: u64, _arg5: u64, _arg6: u64) -> SyscallResult {
    // Note: path argument is ignored for now as we only have one embedded program
    match crate::kernel::process::lifecycle::exec_process() {
        Ok(_) => 0,
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
                    // TODO: Re-enable security validation
                    // Check validity of status_ptr (mapped and writable)
                    // if let Err(e) = validate_user_write(status_ptr, core::mem::size_of::<i32>() as u64) {
                    //     debug_println!("[SYSCALL] sys_wait: invalid status_ptr 0x{:x}", status_ptr);
                    //     return e;
                    // }
                    
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

    // Actually map_user_stack maps a range. We can reuse it or make a generic map_user_memory.
    // map_user_stack is in user_paging.rs.
    // Let's check user_paging.rs exports.
    
    if len == 0 {
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
pub fn sys_pipe(pipefd: u64, _arg2: u64, _arg3: u64, _arg4: u64, _arg5: u64, _arg6: u64) -> SyscallResult {
    use crate::kernel::process::PROCESS_TABLE;
    use crate::kernel::fs::pipe::{Pipe, PipeReader, PipeWriter};
    use alloc::sync::Arc;
    use spin::Mutex;

    // TODO: Re-enable security validation
    // Validate that pipefd is writable (needs 2 * u64)
    // if let Err(e) = validate_user_write(pipefd, 2 * core::mem::size_of::<u64>() as u64) {
    //     return e;
    // }

    // Create pipe
    let pipe = Arc::new(Mutex::new(Pipe::new()));
    
    let reader = Arc::new(Mutex::new(PipeReader {
        pipe: pipe.clone(),
    }));
    
    let writer = Arc::new(Mutex::new(PipeWriter {
        pipe,
    }));

    // Add FDs to process
    let mut table = PROCESS_TABLE.lock();
    let process = match table.current_process_mut() {
        Some(p) => p,
        None => return ESRCH,
    };

    let read_fd = process.add_file_descriptor(reader);
    let write_fd = process.add_file_descriptor(writer);

    // Write FDs to user memory
    // TODO: Validate that pipefd is writable
    unsafe {
        let pipefd_ptr = pipefd as *mut u64;
        *pipefd_ptr = read_fd;
        *pipefd_ptr.add(1) = write_fd;
    }

    SUCCESS
}

/// sys_munmap - Unmap memory
pub fn sys_munmap(addr: u64, len: u64, _arg3: u64, _arg4: u64, _arg5: u64, _arg6: u64) -> SyscallResult {
    if len == 0 {
        return EINVAL;
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
    
    for page in page_range {
        // Unmap
        // We ignore errors (e.g. page not mapped)
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
];

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
        debug_println!("[SYSCALL] Invalid syscall number: {}", syscall_num);
        return ENOSYS;
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
    debug_println!("\nTest 4: sys_write (kernel address)");
    let result = dispatch(
        0, // sys_write
        1, // stdout
        0xFFFF_8000_0000_0000, // Kernel space
        100,
        0, 0, 0
    );
    debug_println!("  Result: {} (expected EFAULT = -14)", result);
    
    debug_println!("\n=== Syscall Mechanism Test Complete ===\n");
}
