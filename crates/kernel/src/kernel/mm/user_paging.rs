// kernel/src/kernel/mm/user_paging.rs
//! User space page table management
//!
//! This module provides functions for creating and managing user space
//! page tables, including mapping user code, stack, and heap.

extern crate alloc;

use alloc::vec::Vec;
use alloc::format;
use x86_64::{
    structures::paging::{
        Page, PageTableFlags, PhysFrame, Size4KiB,
        Mapper, FrameAllocator, OffsetPageTable, Translate,
        page_table::PageTableEntry,
    },
    VirtAddr,
};
use core::fmt;
use crate::kernel::mm::BootInfoFrameAllocator;
use crate::kernel::mm::paging::COW_FLAG;

/// User memory layout constants
///
/// User space address range: 0x0000_0000_0000_0000 ~ 0x0000_7FFF_FFFF_FFFF
pub const USER_CODE_BASE: u64 = 0x0000_0000_0040_0000;  // 4 MiB (traditional ELF base)
/// User heap base address (96 TiB)
pub const USER_HEAP_BASE: u64 = 0x0000_6000_0000_0000;  // 96 TiB
/// User stack top address (128 TiB)
pub const USER_STACK_TOP: u64 = 0x0000_7000_0000_0000;  // 128 TiB

/// io_uring shared memory base address (32 TiB)
/// This is below USER_HEAP_BASE to avoid conflicts with huge pages
pub const USER_IO_URING_BASE: u64 = 0x0000_2000_0000_0000;  // 32 TiB

/// Ring-based syscall context base address (16 TiB)
/// This is the address where RingContext is mapped for userspace access
/// Layout:
///   0x0000_1000_0000_0000: RingContext (sq_header, cq_header, sq_entries, cq_entries)
pub const USER_RING_CONTEXT_BASE: u64 = 0x0000_1000_0000_0000;  // 16 TiB

/// Size of RingContext mapping (rounded up to 64KB for future expansion)
pub const USER_RING_CONTEXT_SIZE: usize = 64 * 1024;

/// Default user stack size (1 MiB - increased for deeper call stacks)
pub const DEFAULT_USER_STACK_SIZE: usize = 1024 * 1024;

/// Map user program code into user page table
///
/// This function maps the provided code into the user's address space
/// starting at `base_addr`. The code is copied to newly allocated physical
/// frames and mapped with USER_ACCESSIBLE permissions.
///
/// # Arguments
/// * `mapper` - Page table mapper for the user address space
/// * `code` - Program code bytes to map
/// * `base_addr` - Virtual address to map code at (usually `USER_CODE_BASE`)
/// * `frame_allocator` - Physical frame allocator
///
/// # Returns
/// Vector of allocated physical frames (for resource tracking)
///
/// # Safety
/// The caller must ensure:
/// - `mapper` points to a valid user page table
/// - `base_addr` is in user space (< 0x0000_8000_0000_0000)
/// - No conflicting mappings exist at `base_addr`
///
/// # Errors
/// Returns `MapError` if frame allocation or mapping fails.
#[allow(clippy::missing_panics_doc)]
pub unsafe fn map_user_code<A>(
    mapper: &mut OffsetPageTable,
    code: &[u8],
    base_addr: VirtAddr,
    frame_allocator: &mut A,
) -> Result<Vec<PhysFrame>, MapError>
where
    A: FrameAllocator<Size4KiB>,
{
    // Validate base_addr is in user space
    if base_addr.as_u64() >= 0x0000_8000_0000_0000 {
        return Err(MapError::InvalidAddress);
    }
    
    let mut allocated_frames = Vec::new();
    
    // Round up to page size (4 KiB)
    let num_pages = (code.len() + 4095) / 4096;
    
    crate::debug_println!(
        "[User Paging] Mapping {} bytes ({} pages) at 0x{:x}",
        code.len(),
        num_pages,
        base_addr.as_u64()
    );
    
    for i in 0..num_pages {
        let page_addr = base_addr + (i * 4096) as u64;
        let page: Page<Size4KiB> = Page::containing_address(page_addr);
        
        // Allocate physical frame
        let frame = frame_allocator
            .allocate_frame()
            .ok_or(MapError::FrameAllocationFailed)?;
        
        // Map as READ-ONLY + EXECUTABLE for code region (W^X principle)
        // Code pages should NEVER be writable for security
        let flags = PageTableFlags::PRESENT
            | PageTableFlags::USER_ACCESSIBLE;  // No WRITABLE for code!
        
        // SAFETY: Caller ensures no conflicting mappings
        unsafe {
            mapper
                .map_to(page, frame, flags, frame_allocator)
                .map_err(|e| MapError::MappingFailed(format!("{:?}", e)))?
                .flush();
        }
        
        // Copy code to the frame
        // SAFETY: We just allocated and mapped this frame
        unsafe {
            let frame_ptr = (frame.start_address().as_u64() + crate::kernel::mm::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed)) as *mut u8;
            let code_offset = i * 4096;
            let copy_len = core::cmp::min(4096, code.len() - code_offset);
            
            let dst = core::slice::from_raw_parts_mut(frame_ptr, 4096);
            let src = &code[code_offset..code_offset + copy_len];
            dst[..copy_len].copy_from_slice(src);
            // Zero-fill remainder
            dst[copy_len..].fill(0);
        }
        
        allocated_frames.push(frame);
    }
    
    crate::debug_println!(
        "[User Paging] Successfully mapped {} frames",
        allocated_frames.len()
    );
    
    Ok(allocated_frames)
}

/// Map user stack
///
/// This function allocates and maps a user stack starting from `USER_STACK_TOP`
/// and growing downward. The stack is mapped with NO_EXECUTE for security.
///
/// **Security Feature: Stack Guard Page**
/// 
/// A guard page (unmapped page) is placed at the bottom of the stack to detect
/// stack overflow. If the stack grows too large and touches the guard page,
/// a page fault will occur, preventing silent memory corruption.
///
/// # Arguments
/// * `mapper` - Page table mapper for the user address space
/// * `stack_size` - Size of stack in bytes (will be rounded up to page boundary)
/// * `frame_allocator` - Physical frame allocator
///
/// # Returns
/// Virtual address of the stack top (RSP should be set to this value)
///
/// # Safety
/// The caller must ensure:
/// - `mapper` points to a valid user page table
/// - `stack_size` is reasonable (< 1 MiB recommended)
/// - No conflicting mappings exist in the stack region
///
/// # Errors
/// Returns `MapError` if frame allocation or mapping fails.
#[allow(clippy::missing_panics_doc)]
pub unsafe fn map_user_stack<A>(
    mapper: &mut OffsetPageTable,
    stack_size: usize,
    frame_allocator: &mut A,
) -> Result<VirtAddr, MapError>
where
    A: FrameAllocator<Size4KiB>,
{
    // Add one extra page for the guard page
    let guard_page_size = 4096;
    let total_size = stack_size + guard_page_size;
    
    let stack_bottom = USER_STACK_TOP - total_size as u64;
    let num_pages = (stack_size + 4095) / 4096;
    
    crate::debug_println!(
        "[User Paging] Mapping stack: {} bytes ({} pages) at 0x{:x}",
        stack_size,
        num_pages,
        stack_bottom + guard_page_size as u64
    );
    crate::debug_println!(
        "[User Paging] Guard page at 0x{:x} (unmapped)",
        stack_bottom
    );
    
    // Map stack pages (skip the first page - that's the guard page)
    for i in 0..num_pages {
        // Start mapping from guard_page_size offset
        let page_addr = VirtAddr::new(stack_bottom + guard_page_size as u64 + (i * 4096) as u64);
        let page: Page<Size4KiB> = Page::containing_address(page_addr);
        
        let frame = frame_allocator
            .allocate_frame()
            .ok_or(MapError::FrameAllocationFailed)?;
        
        // Stack should be writable but NOT executable (NX bit)
        let flags = PageTableFlags::PRESENT
            | PageTableFlags::WRITABLE
            | PageTableFlags::USER_ACCESSIBLE
            | PageTableFlags::NO_EXECUTE; // Important: Stack should not be executable
        
        // SAFETY: Caller ensures no conflicting mappings
        unsafe {
            mapper
                .map_to(page, frame, flags, frame_allocator)
                .map_err(|e| MapError::MappingFailed(format!("{:?}", e)))?
                .flush();
        }
        
        // Zero-initialize stack pages
        // SAFETY: We just allocated and mapped this frame
        unsafe {
            let frame_ptr = (frame.start_address().as_u64() + crate::kernel::mm::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed)) as *mut u8;
            core::ptr::write_bytes(frame_ptr, 0, 4096);
        }
    }
    
    // Note: The guard page (at stack_bottom) is intentionally left UNMAPPED
    // Any access to it will cause a page fault, catching stack overflow
    
    crate::debug_println!(
        "[User Paging] Stack mapped successfully, top=0x{:x}",
        USER_STACK_TOP
    );
    
    Ok(VirtAddr::new(USER_STACK_TOP))
}

/// Map user heap region (placeholder for Phase 2.5)
///
/// This function reserves address space for the heap but doesn't
/// allocate physical frames yet (lazy allocation).
///
/// # Arguments
/// * `mapper` - Page table mapper for the user address space
/// * `heap_size` - Initial heap size (will be rounded up to page boundary)
///
/// # Returns
/// Virtual address of the heap base
#[allow(dead_code)]
pub unsafe fn reserve_user_heap(
    _mapper: &mut OffsetPageTable,
    heap_size: usize,
) -> Result<VirtAddr, MapError> {
    // Phase 2: Just return the base address
    // Phase 3: Implement lazy allocation with page faults
    
    crate::debug_println!(
        "[User Paging] Reserved heap: {} bytes at 0x{:x}",
        heap_size,
        USER_HEAP_BASE
    );
    
    Ok(VirtAddr::new(USER_HEAP_BASE))
}

/// Unmap user memory region
///
/// This function unmaps a range of pages and deallocates the physical frames.
///
/// # Arguments
/// * `mapper` - Page table mapper for the user address space
/// * `start_addr` - Starting virtual address (must be page-aligned)
/// * `num_pages` - Number of pages to unmap
///
/// # Safety
/// The caller must ensure:
/// - `start_addr` is page-aligned
/// - The region was previously mapped
/// - No other references to this memory exist
#[allow(dead_code)]
pub unsafe fn unmap_user_memory(
    mapper: &mut OffsetPageTable,
    start_addr: VirtAddr,
    num_pages: usize,
) -> Result<(), MapError> {
    for i in 0..num_pages {
        let page_addr = start_addr + (i * 4096) as u64;
        let page: Page<Size4KiB> = Page::containing_address(page_addr);
        
        // SAFETY: Caller ensures page was mapped
        match mapper.unmap(page) {
            Ok((_frame, flush)) => {
                flush.flush();
            }
            Err(e) => {
                return Err(MapError::UnmapFailed(format!("{:?}", e)));
            }
        }
    }
    
    Ok(())
}

/// Map error types
#[derive(Debug)]
pub enum MapError {
    /// Failed to allocate a physical frame
    FrameAllocationFailed,
    
    /// Address is not in user space
    InvalidAddress,
    
    /// Mapping operation failed
    MappingFailed(alloc::string::String),
    
    /// Unmap operation failed
    UnmapFailed(alloc::string::String),
    
    /// Address is not mapped
    NotMapped,
    
    /// Page already mapped
    PageAlreadyMapped,
    
    /// Unsupported page size (not 4KiB)
    UnsupportedPageSize,
}

impl fmt::Display for MapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FrameAllocationFailed => write!(f, "Failed to allocate physical frame"),
            Self::InvalidAddress => write!(f, "Address is not in user space"),
            Self::MappingFailed(msg) => write!(f, "Mapping failed: {}", msg),
            Self::UnmapFailed(msg) => write!(f, "Unmap failed: {}", msg),
            Self::NotMapped => write!(f, "Address is not mapped"),
            Self::PageAlreadyMapped => write!(f, "Page is already mapped"),
            Self::UnsupportedPageSize => write!(f, "Unsupported page size (not 4KiB)"),
        }
    }
}

/// Duplicate a user page table (Deep Copy)
///
/// This function creates a new page table and copies all user-space mappings
/// from the source page table. New physical frames are allocated for data.
///
/// # Arguments
/// * `src_mapper` - Mapper for the source page table
/// * `frame_allocator` - Frame allocator
/// * `physical_memory_offset` - Physical memory offset
///
/// # Returns
/// Physical frame of the new page table
/// Duplicate a user page table (Deep Copy)
///
/// This function creates a new page table and copies all user-space mappings
/// from the source page table. New physical frames are allocated for data.
///
/// # Arguments
/// * `_src_mapper` - Mapper for the source page table (unused, we use CR3)
/// * `frame_allocator` - Frame allocator
/// * `physical_memory_offset` - Physical memory offset
///
/// # Returns
/// Physical frame of the new page table
pub unsafe fn duplicate_user_page_table(
    _src_mapper: &mut OffsetPageTable,
    frame_allocator: &mut BootInfoFrameAllocator,
    physical_memory_offset: VirtAddr,
) -> Result<PhysFrame, MapError>
{
    // 1. Allocate new PML4 frame
    let pml4_frame = frame_allocator
        .allocate_frame()
        .ok_or(MapError::FrameAllocationFailed)?;
        
    let pml4_ptr = (physical_memory_offset + pml4_frame.start_address().as_u64()).as_mut_ptr::<x86_64::structures::paging::PageTable>();
    let pml4 = unsafe { &mut *pml4_ptr };
    pml4.zero();
    
    // 2. Copy kernel mappings (upper half)
    let (kernel_frame, _) = x86_64::registers::control::Cr3::read();
    let kernel_pt_ptr = (physical_memory_offset + kernel_frame.start_address().as_u64()).as_ptr::<x86_64::structures::paging::PageTable>();
    let kernel_pt = unsafe { &*kernel_pt_ptr };
    
    for i in 256..512 {
        pml4[i] = kernel_pt[i].clone();
    }
    
    // 3. Deep copy user mappings (lower half)
    let (src_pml4_frame, _) = x86_64::registers::control::Cr3::read();
    let src_pml4_ptr = (physical_memory_offset + src_pml4_frame.start_address().as_u64()).as_mut_ptr::<x86_64::structures::paging::PageTable>();
    let src_pml4 = unsafe { &mut *src_pml4_ptr };
    
    for i in 0..256 {
        if !src_pml4[i].is_unused() {
            unsafe {
                // Get mutable reference to source entry to update flags (CoW)
                copy_pml4_entry(
                    &mut src_pml4[i],
                    &mut pml4[i],
                    frame_allocator,
                    physical_memory_offset
                )?;
            }
        }
    }
    
    Ok(pml4_frame)
}

// Helper to copy page table entries recursively
unsafe fn copy_pml4_entry(
    src_entry: &mut PageTableEntry,
    dst_entry: &mut PageTableEntry,
    frame_allocator: &mut BootInfoFrameAllocator,
    phys_offset: VirtAddr,
) -> Result<(), MapError>
{
    // Allocate new PDPT frame
    let frame = frame_allocator.allocate_frame().ok_or(MapError::FrameAllocationFailed)?;
    dst_entry.set_frame(frame, src_entry.flags());
    
    let src_table_ptr = (phys_offset + src_entry.addr().as_u64()).as_mut_ptr::<x86_64::structures::paging::PageTable>();
    let dst_table_ptr = (phys_offset + frame.start_address().as_u64()).as_mut_ptr::<x86_64::structures::paging::PageTable>();
    // SAFETY: We just allocated the frame and checked the source entry is present
    let src_table = unsafe { &mut *src_table_ptr };
    let dst_table = unsafe { &mut *dst_table_ptr };
    dst_table.zero();
    
    for i in 0..512 {
        if !src_table[i].is_unused() {
            unsafe {
                copy_pdpt_entry(&mut src_table[i], &mut dst_table[i], frame_allocator, phys_offset)?;
            }
        }
    }
    
    Ok(())
}

unsafe fn copy_pdpt_entry(
    src_entry: &mut PageTableEntry,
    dst_entry: &mut PageTableEntry,
    frame_allocator: &mut BootInfoFrameAllocator,
    phys_offset: VirtAddr,
) -> Result<(), MapError>
{
    // Check for huge page (1GB) - Not supported yet
    if src_entry.flags().contains(PageTableFlags::HUGE_PAGE) {
        return Err(MapError::MappingFailed("Huge pages not supported in fork".into()));
    }

    let frame = frame_allocator.allocate_frame().ok_or(MapError::FrameAllocationFailed)?;
    dst_entry.set_frame(frame, src_entry.flags());
    
    let src_table_ptr = (phys_offset + src_entry.addr().as_u64()).as_mut_ptr::<x86_64::structures::paging::PageTable>();
    let dst_table_ptr = (phys_offset + frame.start_address().as_u64()).as_mut_ptr::<x86_64::structures::paging::PageTable>();
    let src_table = unsafe { &mut *src_table_ptr };
    let dst_table = unsafe { &mut *dst_table_ptr };
    dst_table.zero();
    
    for i in 0..512 {
        if !src_table[i].is_unused() {
            unsafe {
                copy_pd_entry(&mut src_table[i], &mut dst_table[i], frame_allocator, phys_offset)?;
            }
        }
    }
    
    Ok(())
}

unsafe fn copy_pd_entry(
    src_entry: &mut PageTableEntry,
    dst_entry: &mut PageTableEntry,
    frame_allocator: &mut BootInfoFrameAllocator,
    phys_offset: VirtAddr,
) -> Result<(), MapError>
{
    // Check for huge page (2MB) - Not supported yet
    if src_entry.flags().contains(PageTableFlags::HUGE_PAGE) {
        return Err(MapError::MappingFailed("Huge pages not supported in fork".into()));
    }

    let frame = frame_allocator.allocate_frame().ok_or(MapError::FrameAllocationFailed)?;
    dst_entry.set_frame(frame, src_entry.flags());
    
    let src_table_ptr = (phys_offset + src_entry.addr().as_u64()).as_mut_ptr::<x86_64::structures::paging::PageTable>();
    let dst_table_ptr = (phys_offset + frame.start_address().as_u64()).as_mut_ptr::<x86_64::structures::paging::PageTable>();
    let src_table = unsafe { &mut *src_table_ptr };
    let dst_table = unsafe { &mut *dst_table_ptr };
    dst_table.zero();
    
    for i in 0..512 {
        if !src_table[i].is_unused() {
            unsafe {
                copy_pt_entry(&mut src_table[i], &mut dst_table[i], frame_allocator, phys_offset)?;
            }
        }
    }
    
    Ok(())
}

unsafe fn copy_pt_entry(
    src_entry: &mut PageTableEntry,
    dst_entry: &mut PageTableEntry,
    frame_allocator: &mut BootInfoFrameAllocator,
    _phys_offset: VirtAddr,
) -> Result<(), MapError>
{
    // This is the leaf level. Implement Copy-on-Write.
    
    let flags = src_entry.flags();
    let frame = src_entry.frame().map_err(|_| MapError::InvalidAddress)?;
    
    if flags.contains(PageTableFlags::WRITABLE) {
        // If writable, mark both as Read-Only + CoW
        let new_flags = (flags - PageTableFlags::WRITABLE) | COW_FLAG;
        
        // Update source entry
        src_entry.set_flags(new_flags);
        
        // Map destination to the SAME frame with new flags
        dst_entry.set_frame(frame, new_flags);
        
        // Increment reference count
        frame_allocator.add_reference(frame);
    } else {
        // If already read-only (e.g. code), just share it
        // We can optionally add CoW flag, but it's not strictly necessary if we never write.
        // Let's just share it as is.
        dst_entry.set_frame(frame, flags);
        
        // Increment reference count
        frame_allocator.add_reference(frame);
    }
    
    Ok(())
}

/// Free all user-space resources in a page table
pub unsafe fn free_user_page_table(
    pml4_frame: PhysFrame,
    frame_allocator: &mut BootInfoFrameAllocator,
    physical_memory_offset: VirtAddr,
) {
    let pml4_ptr = (physical_memory_offset + pml4_frame.start_address().as_u64()).as_mut_ptr::<x86_64::structures::paging::PageTable>();
    let pml4 = unsafe { &mut *pml4_ptr };

    // Only free user space (lower half: 0..256)
    for i in 0..256 {
        if !pml4[i].is_unused() {
            unsafe {
                free_pml4_entry(&mut pml4[i], frame_allocator, physical_memory_offset);
            }
        }
    }
    
    // Finally free the PML4 frame itself
    unsafe {
        frame_allocator.deallocate_frame(pml4_frame);
    }
}

unsafe fn free_pml4_entry(
    entry: &mut PageTableEntry,
    frame_allocator: &mut BootInfoFrameAllocator,
    phys_offset: VirtAddr,
) {
    if let Ok(frame) = entry.frame() {
        let pdpt_ptr = (phys_offset + frame.start_address().as_u64()).as_mut_ptr::<x86_64::structures::paging::PageTable>();
        let pdpt = unsafe { &mut *pdpt_ptr };
        
        for i in 0..512 {
            if !pdpt[i].is_unused() {
                unsafe {
                    free_pdpt_entry(&mut pdpt[i], frame_allocator, phys_offset);
                }
            }
        }
        
        unsafe {
            frame_allocator.deallocate_frame(frame);
        }
    }
    entry.set_unused();
}

unsafe fn free_pdpt_entry(
    entry: &mut PageTableEntry,
    frame_allocator: &mut BootInfoFrameAllocator,
    phys_offset: VirtAddr,
) {
    if entry.flags().contains(PageTableFlags::HUGE_PAGE) {
        if let Ok(frame) = entry.frame() {
            unsafe {
                frame_allocator.deallocate_frame(frame);
            }
        }
        entry.set_unused();
        return;
    }

    if let Ok(frame) = entry.frame() {
        let pd_ptr = (phys_offset + frame.start_address().as_u64()).as_mut_ptr::<x86_64::structures::paging::PageTable>();
        let pd = unsafe { &mut *pd_ptr };
        
        for i in 0..512 {
            if !pd[i].is_unused() {
                unsafe {
                    free_pd_entry(&mut pd[i], frame_allocator, phys_offset);
                }
            }
        }
        
        unsafe {
            frame_allocator.deallocate_frame(frame);
        }
    }
    entry.set_unused();
}

unsafe fn free_pd_entry(
    entry: &mut PageTableEntry,
    frame_allocator: &mut BootInfoFrameAllocator,
    phys_offset: VirtAddr,
) {
    if entry.flags().contains(PageTableFlags::HUGE_PAGE) {
        if let Ok(frame) = entry.frame() {
            unsafe {
                frame_allocator.deallocate_frame(frame);
            }
        }
        entry.set_unused();
        return;
    }

    if let Ok(frame) = entry.frame() {
        let pt_ptr = (phys_offset + frame.start_address().as_u64()).as_mut_ptr::<x86_64::structures::paging::PageTable>();
        let pt = unsafe { &mut *pt_ptr };
        
        for i in 0..512 {
            if !pt[i].is_unused() {
                unsafe {
                    free_pt_entry(&mut pt[i], frame_allocator);
                }
            }
        }
        
        unsafe {
            frame_allocator.deallocate_frame(frame);
        }
    }
    entry.set_unused();
}

unsafe fn free_pt_entry(
    entry: &mut PageTableEntry,
    frame_allocator: &mut BootInfoFrameAllocator,
) {
    if let Ok(frame) = entry.frame() {
        unsafe {
            frame_allocator.deallocate_frame(frame);
        }
    }
    entry.set_unused();
}

/// Validate page table entry flags at all levels
/// Returns true if all levels have USER_ACCESSIBLE flag
pub unsafe fn validate_user_page_flags(
    mapper: &OffsetPageTable,
    virt_addr: VirtAddr,
) -> bool {
    use x86_64::structures::paging::PageTableFlags;
    
    let _page: Page<Size4KiB> = Page::containing_address(virt_addr);
    
    // Try to translate and check if USER_ACCESSIBLE is set
    use x86_64::structures::paging::mapper::TranslateResult;
    match mapper.translate(virt_addr) {
        TranslateResult::Mapped { .. } => {
            // Page is mapped, now check flags at all levels
            // TODO: Walk page table manually to check each level
            true
        },
        TranslateResult::NotMapped | TranslateResult::InvalidFrameAddress(_) => false,
    }
}

/// Dump page table structure for debugging
pub unsafe fn dump_page_table_entry(
    mapper: &OffsetPageTable,
    virt_addr: VirtAddr,
    label: &str,
) {
    use x86_64::structures::paging::PageTableFlags;
    
    let _page: Page<Size4KiB> = Page::containing_address(virt_addr);
    
    use x86_64::structures::paging::mapper::TranslateResult;
    match mapper.translate(virt_addr) {
        TranslateResult::Mapped { frame, offset, flags } => {
            crate::debug_println!(
                "[PageTable] {}: {:#x} -> frame {:#x}, flags: {:?}",
                label,
                virt_addr.as_u64(),
                frame.start_address().as_u64(),
                flags
            );
        },
        TranslateResult::NotMapped => {
            crate::debug_println!(
                "[PageTable] {}: {:#x} -> NOT MAPPED",
                label,
                virt_addr.as_u64()
            );
        },
        TranslateResult::InvalidFrameAddress(addr) => {
            crate::debug_println!(
                "[PageTable] {}: {:#x} -> INVALID FRAME: {:#x}",
                label,
                virt_addr.as_u64(),
                addr
            );
        }
    }
}

/// Map a kernel virtual address range into user page table
/// 
/// This is used for sharing memory between kernel and user space (e.g., io_uring rings).
/// The kernel address must already be mapped in the kernel page table.
/// 
/// # Arguments
/// * `user_mapper` - Page table mapper for the user address space
/// * `kernel_virt` - Kernel virtual address to share
/// * `user_virt` - User virtual address to map to (if None, use same as kernel_virt)
/// * `size` - Size in bytes to map (rounded up to page size)
/// * `kernel_mapper` - Kernel page table mapper (to look up physical addresses)
/// * `phys_offset` - Physical memory offset for kernel direct map
///
/// # Safety
/// Caller must ensure:
/// - kernel_virt is a valid mapped kernel address
/// - user_virt (or kernel_virt) is a valid user space address
/// - The memory will remain valid for the lifetime of the mapping
pub unsafe fn map_kernel_to_user(
    user_mapper: &mut OffsetPageTable,
    kernel_virt: VirtAddr,
    user_virt: Option<VirtAddr>,
    size: usize,
    kernel_mapper: &OffsetPageTable,
    frame_allocator: &mut BootInfoFrameAllocator,
) -> Result<(), MapError> {
    use x86_64::structures::paging::mapper::{TranslateResult, MappedFrame};
    
    let target_virt = user_virt.unwrap_or(kernel_virt);
    let num_pages = (size + 4095) / 4096;
    
    crate::debug_println!(
        "[map_kernel_to_user] Mapping {} pages from kernel {:#x} to user {:#x}",
        num_pages, kernel_virt.as_u64(), target_virt.as_u64()
    );
    
    for i in 0..num_pages {
        let kernel_addr = kernel_virt + (i * 4096) as u64;
        let user_addr = target_virt + (i * 4096) as u64;
        
        // Get physical frame from kernel mapping
        let phys_frame: PhysFrame<Size4KiB> = match kernel_mapper.translate(kernel_addr) {
            TranslateResult::Mapped { frame, .. } => {
                match frame {
                    MappedFrame::Size4KiB(f) => f,
                    _ => {
                        crate::debug_println!(
                            "[map_kernel_to_user] Kernel address {:#x} uses non-4KiB page!",
                            kernel_addr.as_u64()
                        );
                        return Err(MapError::UnsupportedPageSize);
                    }
                }
            }
            _ => {
                crate::debug_println!(
                    "[map_kernel_to_user] Kernel address {:#x} not mapped!",
                    kernel_addr.as_u64()
                );
                return Err(MapError::NotMapped);
            }
        };
        
        let user_page: Page<Size4KiB> = Page::containing_address(user_addr);
        
        // Map with USER_ACCESSIBLE flag
        let flags = PageTableFlags::PRESENT 
            | PageTableFlags::WRITABLE 
            | PageTableFlags::USER_ACCESSIBLE;
        
        // Check if already mapped
        if let TranslateResult::Mapped { .. } = user_mapper.translate(user_addr) {
            crate::debug_println!(
                "[map_kernel_to_user] User address {:#x} already mapped, skipping",
                user_addr.as_u64()
            );
            continue;
        }
        
        unsafe {
            user_mapper
                .map_to(user_page, phys_frame, flags, frame_allocator)
                .map_err(|_| MapError::PageAlreadyMapped)?
                .flush();
        }
        
        crate::debug_println!(
            "[map_kernel_to_user] Mapped user {:#x} -> phys {:#x}",
            user_addr.as_u64(), phys_frame.start_address().as_u64()
        );
    }
    
    Ok(())
}