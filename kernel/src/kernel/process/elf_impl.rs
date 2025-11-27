//! ELF loading implementation
//!
//! This module extends the ELF structures with actual loading logic.

use super::elf_loader::*;
use x86_64::{VirtAddr, PhysAddr, structures::paging::{OffsetPageTable, FrameAllocator, Size4KiB, Page, Mapper, Translate, PhysFrame}};
use x86_64::structures::paging::mapper::TranslateResult;

/// Information about a loaded ELF program
#[derive(Debug)]
pub struct LoadedProgram {
    /// Entry point address
    pub entry: VirtAddr,
    /// Top of the stack
    pub stack_top: VirtAddr,
    /// Base address where program was loaded
    pub base_addr: VirtAddr,
    /// Total size of program in memory
    pub size: u64,
}

/// Load an ELF binary into memory
///
/// # Arguments
/// * `elf_data` - Raw ELF file bytes
/// * `mapper` - Page table mapper
/// * `frame_allocator` - Frame allocator
///
/// # Returns
/// Information about the loaded program
///
/// # Errors
/// Returns `ElfError` if:
/// - ELF file is invalid
/// - Segments cannot be mapped
/// - Out of memory
pub fn load_elf<A>(
    elf_data: &[u8],
    mapper: &mut OffsetPageTable,
    frame_allocator: &mut A,
) -> Result<LoadedProgram, ElfError>
where
    A: FrameAllocator<Size4KiB>,
{
    // 1. Parse and validate ELF header
    let header = unsafe { Elf64Header::from_bytes(elf_data)? };
    
    crate::debug_println!("[ELF] Loading ELF binary, entry=0x{:x}", header.e_entry);
    
    // 2. Get program headers
    let phdrs = unsafe { header.program_headers(elf_data)? };
    
    crate::debug_println!("[ELF] Found {} program headers", phdrs.len());
    
    // 3. Calculate memory requirements
    let (min_addr, max_addr) = calculate_memory_range(phdrs)?;
    let total_size = max_addr - min_addr;
    
    crate::debug_println!("[ELF] Memory range: 0x{:x} - 0x{:x} ({} bytes)", 
        min_addr, max_addr, total_size);
    
    // 4. Load each LOAD segment
    for (i, phdr) in phdrs.iter().enumerate() {
        if phdr.is_load() {
            load_segment(phdr, elf_data, mapper, frame_allocator, i)?;
        }
    }
    
    // 5. Setup stack (using configured default size, growing downward from high address)
    use crate::kernel::mm::user_paging::DEFAULT_USER_STACK_SIZE;
    
    let stack_top = unsafe {
        crate::kernel::mm::user_paging::map_user_stack(
            mapper,
            DEFAULT_USER_STACK_SIZE,
            frame_allocator,
        ).map_err(|_| ElfError::MapFailed)?
    };
    
    Ok(LoadedProgram {
        entry: VirtAddr::new(header.e_entry),
        stack_top,
        base_addr: VirtAddr::new(min_addr),
        size: total_size,
    })
}

/// Load a single segment into memory
fn load_segment<A>(
    phdr: &Elf64ProgramHeader,
    elf_data: &[u8],
    mapper: &mut OffsetPageTable,
    frame_allocator: &mut A,
    index: usize,
) -> Result<(), ElfError>
where
    A: FrameAllocator<Size4KiB>,
{
    let (read, write, exec) = phdr.permissions();
    
    crate::debug_println!(
        "[ELF] Loading segment {}: vaddr=0x{:x}, size=0x{:x}, flags={}{}{}",
        index,
        phdr.p_vaddr,
        phdr.p_memsz,
        if read { "R" } else { "-" },
        if write { "W" } else { "-" },
        if exec { "X" } else { "-" }
    );
    
    // Security: Verify segment is in user space
    if phdr.p_vaddr >= 0x0000_8000_0000_0000 {
        return Err(ElfError::InvalidProgramHeader);
    }
    
    // Calculate page-aligned range
    let start_addr = VirtAddr::new(phdr.p_vaddr);
    let start_page = Page::<Size4KiB>::containing_address(start_addr);
    
    let end_addr = VirtAddr::new(phdr.p_vaddr + phdr.p_memsz);
    let end_page = Page::<Size4KiB>::containing_address(end_addr);
    
    // Get page flags from ELF permissions
    let flags = phdr.to_page_flags();
    
    // Get physical memory offset for direct frame access
    let phys_mem_offset = crate::kernel::mm::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
    
    // Map pages for this segment and keep track of frames for data copy
    // Handle already-mapped pages (e.g., when multiple segments share a page)
    let mut frames: alloc::vec::Vec<(Page<Size4KiB>, PhysFrame<Size4KiB>)> = alloc::vec::Vec::new();
    
    for page in Page::range_inclusive(start_page, end_page) {
        // Check if page is already mapped
        use x86_64::structures::paging::Translate;
        if let TranslateResult::Mapped { frame, .. } = mapper.translate(page.start_address()) {
            // Page already mapped, use existing frame
            let phys_frame = PhysFrame::containing_address(frame.start_address());
            frames.push((page, phys_frame));
            crate::debug_println!("[ELF] Page 0x{:x} already mapped, reusing frame", page.start_address().as_u64());
        } else {
            // Allocate new frame
            let frame = frame_allocator
                .allocate_frame()
                .ok_or(ElfError::MapFailed)?;
            
            frames.push((page, frame));
            
            unsafe {
                mapper
                    .map_to(page, frame, flags, frame_allocator)
                    .map_err(|_| ElfError::MapFailed)?
                    .flush();
            }
        }
    }
    
    // Copy data from ELF file to memory via physical addresses
    if phdr.p_filesz > 0 {
        let file_offset = phdr.p_offset as usize;
        let file_size = phdr.p_filesz as usize;
        
        if file_offset + file_size > elf_data.len() {
            return Err(ElfError::FileTooSmall);
        }
        
        let src = &elf_data[file_offset..file_offset + file_size];
        
        // Copy data to each page via physical address
        let mut bytes_copied = 0usize;
        let segment_offset = (phdr.p_vaddr & 0xFFF) as usize; // Offset within first page
        
        for (i, (page, frame)) in frames.iter().enumerate() {
            let page_phys_addr = frame.start_address().as_u64();
            let page_virt_addr = phys_mem_offset + page_phys_addr;
            
            // Calculate copy range for this page
            let page_start = if i == 0 { segment_offset } else { 0 };
            let remaining = file_size - bytes_copied;
            let page_bytes = core::cmp::min(4096 - page_start, remaining);
            
            if page_bytes == 0 {
                break;
            }
            
            // SAFETY: We access the frame via physical memory offset
            unsafe {
                let dst = core::slice::from_raw_parts_mut(
                    (page_virt_addr + page_start as u64) as *mut u8,
                    page_bytes
                );
                dst.copy_from_slice(&src[bytes_copied..bytes_copied + page_bytes]);
            }
            
            bytes_copied += page_bytes;
        }
        
        crate::debug_println!("[ELF] Copied {} bytes to segment {}", bytes_copied, index);
    }
    
    // Zero-fill BSS (uninitialized data)
    if phdr.p_memsz > phdr.p_filesz {
        let bss_start = phdr.p_vaddr + phdr.p_filesz;
        let bss_size = (phdr.p_memsz - phdr.p_filesz) as usize;
        
        crate::debug_println!("[ELF] Zero-filling BSS: 0x{:x} ({} bytes)", bss_start, bss_size);
        
        // Zero-fill via physical addresses
        let mut bytes_zeroed = 0usize;
        let first_bss_page = Page::<Size4KiB>::containing_address(VirtAddr::new(bss_start));
        
        for (page, frame) in frames.iter() {
            if *page < first_bss_page {
                continue;
            }
            
            let page_phys_addr = frame.start_address().as_u64();
            let page_virt_addr = phys_mem_offset + page_phys_addr;
            
            // Calculate zero-fill range for this page
            let page_offset = if *page == first_bss_page {
                (bss_start & 0xFFF) as usize
            } else {
                0
            };
            
            let remaining = bss_size - bytes_zeroed;
            let zero_bytes = core::cmp::min(4096 - page_offset, remaining);
            
            if zero_bytes == 0 {
                break;
            }
            
            unsafe {
                core::ptr::write_bytes(
                    (page_virt_addr + page_offset as u64) as *mut u8,
                    0,
                    zero_bytes
                );
            }
            
            bytes_zeroed += zero_bytes;
        }
    }
    
    Ok(())
}

/// Calculate the memory range needed for all LOAD segments
fn calculate_memory_range(phdrs: &[Elf64ProgramHeader]) -> Result<(u64, u64), ElfError> {
    let mut min_addr = u64::MAX;
    let mut max_addr = 0u64;
    
    for phdr in phdrs {
        if phdr.is_load() {
            let start = phdr.p_vaddr;
            let end = phdr.p_vaddr + phdr.p_memsz;
            
            min_addr = min_addr.min(start);
            max_addr = max_addr.max(end);
        }
    }
    
    if min_addr == u64::MAX {
        return Err(ElfError::InvalidProgramHeader);
    }
    
    Ok((min_addr, max_addr))
}

/// Validate ELF file format with detailed checks
pub fn validate_elf(elf_data: &[u8]) -> Result<(), ElfError> {
    let header = unsafe { Elf64Header::from_bytes(elf_data)? };
    let _phdrs = unsafe { header.program_headers(elf_data)? };
    
    // Extended validation
    header.validate_extended()?;
    
    Ok(())
}

/// Get detailed ELF information for debugging
pub fn get_elf_info(elf_data: &[u8]) -> Result<ElfInfo, ElfError> {
    let header = unsafe { Elf64Header::from_bytes(elf_data)? };
    let phdrs = unsafe { header.program_headers(elf_data)? };
    
    let mut loadable_segments = 0;
    let mut total_memory_size = 0u64;
    
    for phdr in phdrs {
        if phdr.is_load() {
            loadable_segments += 1;
            total_memory_size += phdr.p_memsz;
        }
    }
    
    Ok(ElfInfo {
        entry_point: header.e_entry,
        program_header_count: header.e_phnum,
        section_header_count: header.e_shnum,
        loadable_segments,
        total_memory_size,
        file_type: header.e_type,
    })
}

/// Detailed ELF information
#[derive(Debug)]
pub struct ElfInfo {
    pub entry_point: u64,
    pub program_header_count: u16,
    pub section_header_count: u16,
    pub loadable_segments: usize,
    pub total_memory_size: u64,
    pub file_type: u16,
}

/// Print detailed ELF information
pub fn print_elf_info(elf_data: &[u8]) -> Result<(), ElfError> {
    let info = get_elf_info(elf_data)?;
    
    crate::debug_println!("=== ELF File Information ===");
    crate::debug_println!("Entry point: 0x{:x}", info.entry_point);
    crate::debug_println!("Program headers: {}", info.program_header_count);
    crate::debug_println!("Section headers: {}", info.section_header_count);
    crate::debug_println!("Loadable segments: {}", info.loadable_segments);
    crate::debug_println!("Total memory: {} bytes", info.total_memory_size);
    crate::debug_println!("File type: 0x{:x}", info.file_type);
    crate::debug_println!("============================");
    
    Ok(())
}

/// Get entry point from ELF file
pub fn get_entry_point(elf_data: &[u8]) -> Result<VirtAddr, ElfError> {
    let header = unsafe { Elf64Header::from_bytes(elf_data)? };
    Ok(VirtAddr::new(header.e_entry))
}

/// Verify W^X (Write XOR Execute) property
pub fn verify_wx_separation(elf_data: &[u8]) -> Result<bool, ElfError> {
    let header = unsafe { Elf64Header::from_bytes(elf_data)? };
    let phdrs = unsafe { header.program_headers(elf_data)? };
    
    for phdr in phdrs {
        if phdr.is_load() {
            let (_, write, exec) = phdr.permissions();
            if write && exec {
                crate::debug_println!("[SECURITY] WARNING: Segment at 0x{:x} is both writable and executable!", phdr.p_vaddr);
                return Ok(false);
            }
        }
    }
    
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_calculate_memory_range() {
        let mut phdrs = [Elf64ProgramHeader {
            p_type: ProgramHeaderType::Load as u32,
            p_flags: phdr_flags::PF_R | phdr_flags::PF_X,
            p_offset: 0,
            p_vaddr: 0x400000,
            p_paddr: 0x400000,
            p_filesz: 0x1000,
            p_memsz: 0x1000,
            p_align: 0x1000,
        }];
        
        let (min, max) = calculate_memory_range(&phdrs).unwrap();
        assert_eq!(min, 0x400000);
        assert_eq!(max, 0x401000);
    }
}
