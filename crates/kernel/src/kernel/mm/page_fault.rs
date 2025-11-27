// src/kernel/mm/page_fault.rs
//! User-space page fault handling
//!
//! This module provides page fault handling for user-space processes,
//! including lazy allocation, copy-on-write, and stack growth.

use x86_64::structures::idt::PageFaultErrorCode;
use x86_64::{VirtAddr, structures::paging::{Page, PageTableFlags, Mapper, Size4KiB, FrameAllocator, Translate, mapper::TranslateResult}};
use crate::kernel::mm::user_paging::{USER_STACK_TOP, USER_CODE_BASE};
use crate::kernel::mm::paging::COW_FLAG;
use crate::kernel::mm::BootInfoFrameAllocator;

/// User stack size (64 KiB)
const USER_STACK_SIZE: u64 = 64 * 1024;
use crate::debug_println;

/// Result type for page fault handling
pub type PageFaultResult<T> = Result<T, PageFaultError>;

/// Error types for page fault handling
#[derive(Debug, Clone, Copy)]
pub enum PageFaultError {
    /// Invalid memory access (segmentation fault)
    InvalidAccess,
    /// Access violation (e.g., write to read-only page)
    AccessViolation,
    /// Out of memory
    OutOfMemory,
    /// Stack overflow
    StackOverflow,
    /// Invalid address (not in user space)
    InvalidAddress,
    /// Translation failed
    TranslationFailed,
}

/// Page fault handler for user-space addresses
///
/// This function handles page faults that occur in user space. It implements:
/// - Lazy stack allocation (allocate stack pages on demand)
/// - Stack growth (expand stack when needed)
/// - Access violation detection
/// - Copy-on-Write (CoW) handling
///
/// # Arguments
///
/// * `fault_addr` - The virtual address that caused the page fault
/// * `error_code` - The page fault error code from the CPU
/// * `mapper` - Page table mapper for the faulting process
/// * `frame_allocator` - Frame allocator for allocating physical memory
///
/// # Returns
///
/// `Ok(())` if the page fault was successfully handled, `Err(PageFaultError)` otherwise
pub fn handle_user_page_fault<M>(
    fault_addr: VirtAddr,
    error_code: PageFaultErrorCode,
    mapper: &mut M,
    frame_allocator: &mut BootInfoFrameAllocator,
) -> PageFaultResult<()>
where
    M: Mapper<Size4KiB> + Translate,
{
    let fault_page = Page::containing_address(fault_addr);
    let fault_addr_u64 = fault_addr.as_u64();
    
    debug_println!(
        "[PageFault] User space fault at {:#x}, error: {:?}",
        fault_addr_u64,
        error_code
    );
    
    // Check if the fault is in user stack region
    let stack_top = USER_STACK_TOP;
    let stack_bottom = stack_top - USER_STACK_SIZE;
    
    if fault_addr_u64 >= stack_bottom && fault_addr_u64 < stack_top {
        // This is a stack access
        
        // Check if it's a protection violation (page is present but access denied)
        if error_code.contains(PageFaultErrorCode::PROTECTION_VIOLATION) {
            // Check for CoW
            if handle_cow_fault(fault_page, error_code, mapper, frame_allocator)? {
                return Ok(());
            }
            
            debug_println!("[PageFault] Stack protection violation");
            return Err(PageFaultError::AccessViolation);
        }
        
        // Check if it's a write to a not-present page (lazy allocation)
        if !error_code.contains(PageFaultErrorCode::PROTECTION_VIOLATION) {
            // Page not present - allocate it
            debug_println!("[PageFault] Lazy stack allocation at page {:#x}", fault_page.start_address().as_u64());
            
            // Allocate a physical frame
            let frame = frame_allocator
                .allocate_frame()
                .ok_or(PageFaultError::OutOfMemory)?;
            
            // Map the page with user-accessible, writable flags
            let flags = PageTableFlags::PRESENT 
                | PageTableFlags::WRITABLE 
                | PageTableFlags::USER_ACCESSIBLE;
            
            unsafe {
                mapper
                    .map_to(fault_page, frame, flags, frame_allocator)
                    .map_err(|_| PageFaultError::OutOfMemory)?
                    .flush();
            }
            
            // Zero-initialize the new stack page
            unsafe {
                let phys_offset = VirtAddr::new(crate::kernel::mm::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed));
                let frame_ptr = (phys_offset + frame.start_address().as_u64()).as_mut_ptr::<u8>();
                core::ptr::write_bytes(frame_ptr, 0, 4096);
            }
            
            debug_println!("[PageFault] Stack page allocated successfully");
            return Ok(());
        }
    }
    
    // Check if the fault is in code region (should already be mapped)
    let code_end = USER_CODE_BASE + (1024 * 1024); // 1 MB max program size
    if fault_addr_u64 >= USER_CODE_BASE && fault_addr_u64 < code_end {
        // Check for CoW
        if error_code.contains(PageFaultErrorCode::PROTECTION_VIOLATION) {
             if handle_cow_fault(fault_page, error_code, mapper, frame_allocator)? {
                return Ok(());
            }
            // Trying to write to code segment
            return Err(PageFaultError::AccessViolation);
        }
        
        debug_println!("[PageFault] Code region fault - likely protection violation");
        
        // Code page not present - this shouldn't happen
        return Err(PageFaultError::InvalidAccess);
    }
    
    // Check for generic CoW in other regions (heap, etc.)
    if error_code.contains(PageFaultErrorCode::PROTECTION_VIOLATION) {
        if handle_cow_fault(fault_page, error_code, mapper, frame_allocator)? {
            return Ok(());
        }
    }
    
    // Fault is outside valid user memory regions
    debug_println!("[PageFault] Invalid user address: {:#x}", fault_addr_u64);
    Err(PageFaultError::InvalidAddress)
}

/// Handle Copy-on-Write fault
fn handle_cow_fault<M>(
    page: Page<Size4KiB>,
    error_code: PageFaultErrorCode,
    mapper: &mut M,
    frame_allocator: &mut BootInfoFrameAllocator,
) -> PageFaultResult<bool>
where
    M: Mapper<Size4KiB> + Translate,
{
    // CoW only applies to write violations
    if !error_code.contains(PageFaultErrorCode::CAUSED_BY_WRITE) {
        return Ok(false);
    }

    // Get current flags and frame
    let (phys_frame, flags) = match mapper.translate(page.start_address()) {
        TranslateResult::Mapped { frame, flags, .. } => (frame, flags),
        _ => return Err(PageFaultError::TranslationFailed),
    };
        
    // Check if CoW flag is set
    if !flags.contains(COW_FLAG) {
        return Ok(false);
    }
    
    debug_println!("[PageFault] Handling CoW for page {:#x}", page.start_address().as_u64());
    
    // Allocate new frame
    let new_frame = frame_allocator.allocate_frame().ok_or(PageFaultError::OutOfMemory)?;
    
    // Copy data
    unsafe {
        let phys_offset = VirtAddr::new(crate::kernel::mm::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed));
        let src_ptr = (phys_offset + phys_frame.start_address().as_u64()).as_ptr::<u8>();
        let dst_ptr = (phys_offset + new_frame.start_address().as_u64()).as_mut_ptr::<u8>();
        core::ptr::copy_nonoverlapping(src_ptr, dst_ptr, 4096);
    }
    
    // Unmap old frame (this is tricky because unmap returns the frame, but we need to be careful not to double free if we just drop it)
    // Actually, we should use `mapper.unmap` which returns the frame and flush.
    // Then we call `frame_allocator.deallocate_frame` which handles ref count decrement.
    
    let (old_frame, flush) = mapper.unmap(page).map_err(|_| PageFaultError::TranslationFailed)?;
    flush.flush();
    
    // Decrement ref count of old frame
    unsafe {
        frame_allocator.deallocate_frame(old_frame);
    }
    
    // Map new frame with WRITABLE and NO CoW flag
    let new_flags = (flags | PageTableFlags::WRITABLE) - COW_FLAG;
    
    unsafe {
        mapper.map_to(page, new_frame, new_flags, frame_allocator)
            .map_err(|_| PageFaultError::OutOfMemory)?
            .flush();
    }
    
    debug_println!("[PageFault] CoW complete for page {:#x}", page.start_address().as_u64());
    
    Ok(true)
}

/// Check if a virtual address is in valid user space range
///
/// # Arguments
///
/// * `addr` - The virtual address to check
///
/// # Returns
///
/// `true` if the address is in user space, `false` otherwise
pub fn is_user_space_address(addr: VirtAddr) -> bool {
    let addr_u64 = addr.as_u64();
    
    // User space is typically 0x0000_0000_0000_0000 to 0x0000_7FFF_FFFF_FFFF
    // We use a more conservative range for our OS
    
    // Check code region (4 MiB to 5 MiB)
    let code_start = USER_CODE_BASE;
    let code_end = code_start + (1024 * 1024); // 1 MB
    
    if addr_u64 >= code_start && addr_u64 < code_end {
        return true;
    }
    
    // Check stack region
    let stack_top = USER_STACK_TOP;
    let stack_bottom = stack_top - USER_STACK_SIZE;
    
    if addr_u64 >= stack_bottom && addr_u64 < stack_top {
        return true;
    }
    
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_is_user_space_address() {
        // Code region
        assert!(is_user_space_address(VirtAddr::new(USER_CODE_BASE)));
        assert!(is_user_space_address(VirtAddr::new(USER_CODE_BASE + 0x1000)));
        
        // Stack region
        assert!(is_user_space_address(VirtAddr::new(USER_STACK_TOP - 0x1000)));
        assert!(is_user_space_address(VirtAddr::new(USER_STACK_TOP - USER_STACK_SIZE + 1)));
        
        // Invalid addresses
        assert!(!is_user_space_address(VirtAddr::new(0)));
        assert!(!is_user_space_address(VirtAddr::new(0xFFFF_8000_0000_0000))); // Kernel space
    }
}
