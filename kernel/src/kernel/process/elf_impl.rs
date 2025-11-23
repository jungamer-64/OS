//! ELF loading implementation
//!
//! This module extends the ELF structures with actual loading logic.

use super::elf_loader::*;
use x86_64::{VirtAddr, structures::paging::{OffsetPageTable, FrameAllocator, Size4KiB, Page, Mapper}};

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
    
    // 5. Setup stack (64KB, growing downward from high address)
    const STACK_SIZE: u64 = 64 * 1024;
    const STACK_TOP: u64 = 0x0000_7000_0000_0000; // 128 TiB
    
    let stack_top = unsafe {
        crate::kernel::mm::user_paging::map_user_stack(
            mapper,
            STACK_SIZE as usize,
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
    
    // Map pages for this segment
    for page in Page::range_inclusive(start_page, end_page) {
        let frame = frame_allocator
            .allocate_frame()
            .ok_or(ElfError::MapFailed)?;
        
        unsafe {
            mapper
                .map_to(page, frame, flags, frame_allocator)
                .map_err(|_| ElfError::MapFailed)?
                .flush();
        }
    }
    
    // Copy data from ELF file to memory
    if phdr.p_filesz > 0 {
        let file_offset = phdr.p_offset as usize;
        let file_size = phdr.p_filesz as usize;
        
        if file_offset + file_size > elf_data.len() {
            return Err(ElfError::FileTooSmall);
        }
        
        let src = &elf_data[file_offset..file_offset + file_size];
        
        // SAFETY: We just mapped these pages
        unsafe {
            let dst = core::slice::from_raw_parts_mut(
                phdr.p_vaddr as *mut u8,
                file_size
            );
            dst.copy_from_slice(src);
        }
    }
    
    // Zero-fill BSS (uninitialized data)
    if phdr.p_memsz > phdr.p_filesz {
        let bss_start = phdr.p_vaddr + phdr.p_filesz;
        let bss_size = phdr.p_memsz - phdr.p_filesz;
        
        crate::debug_println!("[ELF] Zero-filling BSS: 0x{:x} ({} bytes)", bss_start, bss_size);
        
        unsafe {
            core::ptr::write_bytes(bss_start as *mut u8, 0, bss_size as usize);
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

/// Validate ELF file format
pub fn validate_elf(elf_data: &[u8]) -> Result<(), ElfError> {
    let header = unsafe { Elf64Header::from_bytes(elf_data)? };
    let _phdrs = unsafe { header.program_headers(elf_data)? };
    Ok(())
}

/// Get entry point from ELF file
pub fn get_entry_point(elf_data: &[u8]) -> Result<VirtAddr, ElfError> {
    let header = unsafe { Elf64Header::from_bytes(elf_data)? };
    Ok(VirtAddr::new(header.e_entry))
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
