//! Security validation and access control
//!
//! This module provides security checks for system calls and kernel operations.
//! It validates user-provided pointers, enforces access controls, and prevents
//! privilege escalation.
//!
//! # Overview
//!
//! The security module implements defense-in-depth by providing multiple
//! layers of validation:
//!
//! 1. **Address range validation** - Basic checks that pointers are in user space
//! 2. **Page mapping validation** - Verifies pages are actually mapped
//! 3. **Permission validation** - Checks read/write/execute permissions
//! 4. **Size validation** - Prevents integer overflow and excessive allocations
//!
//! # Usage
//!
//! System call handlers should validate all user-provided pointers:
//!
//! ```rust,ignore
//! pub fn sys_write(fd: u64, buf: u64, len: u64) -> SyscallResult {
//!     // Validate user buffer
//!     validate_user_read(buf, len)?;
//!     
//!     // Safe to use buffer
//!     let slice = unsafe { core::slice::from_raw_parts(buf as *const u8, len as usize) };
//!     // ...
//! }
//! ```

use x86_64::structures::paging::{OffsetPageTable, PageTableFlags, Mapper, Page, Size4KiB, PageTable};
use x86_64::{VirtAddr, registers::control::Cr3};
use core::sync::atomic::Ordering;
use crate::kernel::mm::PHYS_MEM_OFFSET;

// Type alias for syscall results
type SyscallResult<T> = Result<T, i64>;

// Error codes (Linux-compatible)
const EFAULT: i64 = -14;   // Bad address
const EINVAL: i64 = -22;   // Invalid argument

/// User space address range
pub mod address_space {
    /// Start of user space
    pub const USER_START: u64 = 0x0000_0000_0000_0000;
    
    /// End of user space (exclusive)
    pub const USER_END: u64 = 0x0000_8000_0000_0000;
    
    /// Start of kernel space
    pub const KERNEL_START: u64 = 0xFFFF_8000_0000_0000;
    
    /// Size of user space
    pub const USER_SIZE: u64 = USER_END - USER_START;
}

/// Check if an address is in user space
///
/// # Arguments
/// * `addr` - Virtual address to check
///
/// # Returns
/// `true` if the address is in user space ([`address_space::USER_START`], [`address_space::USER_END`])
///
/// # Examples
/// ```rust,ignore
/// assert!(is_user_address(0x1000));
/// assert!(!is_user_address(0xFFFF_8000_0000_0000));
/// ```
#[inline]
pub const fn is_user_address(addr: u64) -> bool {
    addr < address_space::USER_END
}

/// Check if a memory range is entirely in user space
///
/// This function prevents:
/// - Integer overflow attacks (checked addition)
/// - Partial kernel access (validates entire range)
/// - Zero-length exploits (implicitly handled)
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
/// This is a **critical security function**. It must:
/// - Check for integer overflow in addr + len
/// - Verify both start and end are in user space
/// - Handle edge cases (0-length, max address)
#[inline]
pub fn is_user_range(addr: u64, len: u64) -> bool {
    // Check for overflow
    let end = match addr.checked_add(len) {
        Some(e) => e,
        None => return false, // Overflow
    };
    
    // Check range is in user space
    is_user_address(addr) && is_user_address(end.saturating_sub(1))
}

/// Verify that a memory range is mapped with the required permissions
///
/// # Arguments
/// * `ptr` - Start address
/// * `len` - Length in bytes
/// * `_check_read` - Whether to check read permission
/// * `check_write` - Whether to check write permission
///
/// # Returns
/// * `Ok(())` - All pages are mapped with required permissions
/// * `Err(EFAULT)` - At least one page is not mapped or lacks permissions
///
/// # Safety
///
/// This function accesses page tables which requires proper synchronization.
/// It should only be called with valid user-space addresses.
fn verify_page_mapping(ptr: u64, len: u64, _check_read: bool, check_write: bool) -> SyscallResult<()> {
    if len == 0 {
        return Ok(());
    }
    
    // Get physical memory offset
    let phys_mem_offset = VirtAddr::new(PHYS_MEM_OFFSET.load(Ordering::Relaxed));
    
    // Get current page table
    let (level_4_table_frame, _) = Cr3::read();
    let phys = level_4_table_frame.start_address();
    let virt = phys_mem_offset + phys.as_u64();
    let page_table_ptr = virt.as_mut_ptr();
    
    // Safety: We're reading from the active page table pointed to by CR3
    let level_4_table = unsafe { &mut *page_table_ptr };
    let mapper = unsafe { OffsetPageTable::new(level_4_table, phys_mem_offset) };
    
    // Check each page in the range
    let start_page = Page::<Size4KiB>::containing_address(VirtAddr::new(ptr));
    let end_addr = ptr + len - 1;
    let end_page = Page::<Size4KiB>::containing_address(VirtAddr::new(end_addr));
    
    let page_range = Page::range_inclusive(start_page, end_page);
    
    for page in page_range {
        // Try to translate the page
        match mapper.translate_page(page) {
            Ok(_frame) => {
                // Page is mapped, now check permissions
                // We need to get the flags for this page
                let flags = get_page_flags(&mapper, page)?;
                
                // Check if page is present and user-accessible
                if !flags.contains(PageTableFlags::PRESENT) {
                    return Err(EFAULT);
                }
                
                if !flags.contains(PageTableFlags::USER_ACCESSIBLE) {
                    return Err(EFAULT);
                }
                
                // Check read permission (on x86_64, if page is present and user-accessible, it's readable)
                // No additional check needed for read
                
                // Check write permission if required
                if check_write && !flags.contains(PageTableFlags::WRITABLE) {
                    return Err(EFAULT);
                }
            }
            Err(_) => {
                // Page is not mapped
                return Err(EFAULT);
            }
        }
    }
    
    Ok(())
}

/// Get page table flags for a specific page
///
/// # Arguments
/// * `mapper` - Page table mapper
/// * `page` - Page to get flags for
///
/// # Returns
/// * `Ok(flags)` - Page table flags
/// * `Err(EFAULT)` - Failed to get flags
fn get_page_flags(_mapper: &OffsetPageTable, page: Page<Size4KiB>) -> SyscallResult<PageTableFlags> {
    // Walk page table manually to get flags
    let addr = page.start_address();
    unsafe {
        let (l4_table_frame, _) = Cr3::read();
        let phys_mem_offset = VirtAddr::new(PHYS_MEM_OFFSET.load(Ordering::Relaxed));
        let l4_table_ptr = (phys_mem_offset + l4_table_frame.start_address().as_u64()).as_mut_ptr();
        let l4_table = &*(l4_table_ptr as *const PageTable);
        
        let l4_index = (addr.as_u64() >> 39) & 0o777;
        let l3_index = (addr.as_u64() >> 30) & 0o777;
        let l2_index = (addr.as_u64() >> 21) & 0o777;
        let l1_index = (addr.as_u64() >> 12) & 0o777;
        
        let l4_entry = &l4_table[l4_index as usize];
        if l4_entry.is_unused() {
            return Err(EFAULT);
        }
        let l4_flags = l4_entry.flags();
        
        let l3_table_ptr = (phys_mem_offset + l4_entry.addr().as_u64()).as_ptr();
        let l3_table = &*(l3_table_ptr as *const PageTable);
        let l3_entry = &l3_table[l3_index as usize];
        if l3_entry.is_unused() {
            return Err(EFAULT);
        }
        let l3_flags = l3_entry.flags();
        
        let l2_table_ptr = (phys_mem_offset + l3_entry.addr().as_u64()).as_ptr();
        let l2_table = &*(l2_table_ptr as *const PageTable);
        let l2_entry = &l2_table[l2_index as usize];
        if l2_entry.is_unused() {
            return Err(EFAULT);
        }
        let l2_flags = l2_entry.flags();
        
        let l1_table_ptr = (phys_mem_offset + l2_entry.addr().as_u64()).as_ptr();
        let l1_table = &*(l1_table_ptr as *const PageTable);
        let l1_entry = &l1_table[l1_index as usize];
        if l1_entry.is_unused() {
            return Err(EFAULT);
        }
        let l1_flags = l1_entry.flags();
        
        // Combine flags from all levels (AND operation for restrictive flags)
        let combined_flags = l4_flags & l3_flags & l2_flags & l1_flags;
        Ok(combined_flags)
    }
}

/// Validate that a user pointer can be read from
///
/// # Arguments
/// * `ptr` - User-provided pointer
/// * `len` - Number of bytes to read
///
/// # Returns
/// * `Ok(())` - Pointer is valid for reading
/// * `Err(EFAULT)` - Invalid pointer or not mapped
/// * `Err(EINVAL)` - Invalid length
///
/// # Security
///
/// Checks:
/// 1. Pointer is in user space
/// 2. Range doesn't overflow
/// 3. TODO: Verify pages are mapped
/// 4. TODO: Verify pages have read permission
pub fn validate_user_read(ptr: u64, len: u64) -> SyscallResult<()> {
    // Check for zero-length (allowed, no-op)
    if len == 0 {
        return Ok(());
    }
    
    // Check null pointer
    if ptr == 0 {
        return Err(EFAULT);
    }
    
    // Check range is in user space
    if !is_user_range(ptr, len) {
        return Err(EFAULT);
    }
    
    // Verify pages are actually mapped and have read permission
    verify_page_mapping(ptr, len, true, false)?;
    
    Ok(())
}

/// Validate that a user pointer can be written to
///
/// # Arguments
/// * `ptr` - User-provided pointer
/// * `len` - Number of bytes to write
///
/// # Returns
/// * `Ok(())` - Pointer is valid for writing
/// * `Err(EFAULT)` - Invalid pointer or not mapped
/// * `Err(EINVAL)` - Invalid length
///
/// # Security
///
/// Checks:
/// 1. Pointer is in user space
/// 2. Range doesn't overflow
/// 3. TODO: Verify pages are mapped
/// 4. TODO: Verify pages have write permission
pub fn validate_user_write(ptr: u64, len: u64) -> SyscallResult<()> {
    // Check for zero-length (allowed, no-op)
    if len == 0 {
        return Ok(());
    }
    
    // Check null pointer
    if ptr == 0 {
        return Err(EFAULT);
    }
    
    // Check range is in user space
    if !is_user_range(ptr, len) {
        return Err(EFAULT);
    }
    
    // Verify pages are actually mapped and have write permission
    verify_page_mapping(ptr, len, true, true)?;
    
    Ok(())
}

/// Validate a user string pointer
///
/// This validates that a null-terminated string is entirely in user space.
///
/// # Arguments
/// * `ptr` - Pointer to null-terminated string
/// * `max_len` - Maximum length to scan (prevents infinite loops)
///
/// # Returns
/// * `Ok(len)` - String is valid, returns length (excluding null terminator)
/// * `Err(EFAULT)` - Invalid pointer
/// * `Err(EINVAL)` - String too long or not null-terminated
///
/// # Safety
///
/// This function must scan user memory to find the null terminator.
/// It's inherently slower than fixed-length validation.
pub fn validate_user_string(ptr: u64, max_len: usize) -> SyscallResult<usize> {
    if ptr == 0 {
        return Err(EFAULT);
    }
    
    if !is_user_address(ptr) {
        return Err(EFAULT);
    }
    
    // Scan user memory for null terminator
    let mut len = 0;
    while len < max_len {
        let current_ptr = ptr + len as u64;
        
        // Check if we can still read this byte
        if !is_user_address(current_ptr) {
            return Err(EFAULT);
        }
        
        // Validate current byte is readable
        validate_user_read(current_ptr, 1)?;
        
        // Read the byte
        let byte = unsafe { *(current_ptr as *const u8) };
        
        // Check for null terminator
        if byte == 0 {
            return Ok(len);
        }
        
        len += 1;
    }
    
    // String too long or not null-terminated
    Err(EINVAL)
}

/// Validate an array of user pointers
///
/// # Arguments
/// * `ptr` - Pointer to array of pointers
/// * `count` - Number of pointers in array
///
/// # Returns
/// * `Ok(())` - All pointers are valid
/// * `Err(EFAULT)` - Invalid pointer or array
pub fn validate_user_pointer_array(ptr: u64, count: usize) -> SyscallResult<()> {
    let len = count.checked_mul(8).ok_or(EINVAL)?; // 8 bytes per pointer on x86-64
    validate_user_read(ptr, len as u64)
}

/// Maximum reasonable allocation size (1GB)
///
/// This prevents:
/// - Out of memory attacks
/// - Integer overflow in size calculations
/// - Excessive resource consumption
pub const MAX_ALLOC_SIZE: u64 = 1024 * 1024 * 1024;

/// Validate an allocation size
///
/// # Arguments
/// `size` - Requested allocation size
///
/// # Returns
/// * `Ok(())` - Size is reasonable
/// * `Err(EINVAL)` - Size is zero or too large
pub fn validate_alloc_size(size: u64) -> SyscallResult<()> {
    if size == 0 {
        return Err(EINVAL);
    }
    
    if size > MAX_ALLOC_SIZE {
        return Err(EINVAL);
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_is_user_address() {
        assert!(is_user_address(0));
        assert!(is_user_address(0x1000));
        assert!(is_user_address(0x7FFF_FFFF_FFFF));
        assert!(!is_user_address(0x8000_0000_0000));
        assert!(!is_user_address(0xFFFF_8000_0000_0000));
    }
    
    #[test]
    fn test_is_user_range() {
        assert!(is_user_range(0x1000, 0x1000));
        assert!(is_user_range(0, 0x1000));
        assert!(!is_user_range(0x7FFF_FFFF_F000, 0x2000)); // Crosses boundary
        assert!(!is_user_range(u64::MAX, 1)); // Overflow
    }
}
