// kernel/src/kernel/process/lifecycle.rs
//! Process lifecycle management

use alloc::string::String;
use alloc::vec::Vec;
use alloc::sync::Arc;
use x86_64::structures::paging::{
    PageTable, OffsetPageTable, FrameAllocator, PhysFrame, Size4KiB, PageTableFlags,
    Mapper, Translate, Page
};
use x86_64::structures::paging::mapper::TranslateResult;
use x86_64::{VirtAddr, PhysAddr};
use crate::kernel::process::{Process, ProcessId, ProcessState, PROCESS_TABLE};
use crate::kernel::loader::load_user_program;
use crate::kernel::mm::allocator::BOOT_INFO_ALLOCATOR;
use crate::kernel::mm::PHYS_MEM_OFFSET;

/// Error creating a process
#[derive(Debug)]
pub enum CreateError {
    /// Frame allocation failed
    FrameAllocationFailed,
    LoaderError(crate::kernel::loader::LoadError),
    /// Page table creation error
    PageTableCreationError(&'static str),
    /// File not found
    FileNotFound,
}

impl From<crate::kernel::loader::LoadError> for CreateError {
    fn from(e: crate::kernel::loader::LoadError) -> Self {
        CreateError::LoaderError(e)
    }
}

/// Create a new user process
/// 
/// This is the main entry point for creating processes in Phase 2.
/// It creates a new process, loads the program from the filesystem, and adds it to the process table.
pub fn create_user_process(path: &str, args: &[&str]) -> Result<(ProcessId, VirtAddr, VirtAddr, u64), CreateError> {
    let mut allocator_lock = BOOT_INFO_ALLOCATOR.lock();
    let frame_allocator = allocator_lock.as_mut().ok_or(CreateError::FrameAllocationFailed)?;
    
    let phys_mem_offset = VirtAddr::new(PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed));
    
    // 1. Create process structure (allocates page table and stacks)
    // We use a dummy entry point initially
    let mut process = crate::kernel::process::create_process_with_context(
        VirtAddr::new(0),
        frame_allocator,
        phys_mem_offset
    ).map_err(CreateError::PageTableCreationError)?;
    
    let pid = process.pid();
    
    // 2. Load program into the process's address space
    // We need to temporarily access the process's page table
    {
        let l4_table_ptr = (phys_mem_offset + process.page_table_frame().start_address().as_u64())
            .as_mut_ptr::<PageTable>();
            
        let l4_table = unsafe { &mut *l4_table_ptr };

        crate::debug_println!("[create_user_process] PML4 Entry 0 before load: {:?}", l4_table[0]);

        // Scope 1: Load program
        let loaded_program = {
            let mut mapper = unsafe { OffsetPageTable::new(l4_table, phys_mem_offset) };
            
            // Get embedded program binary (TODO: Use VFS when implemented)
            let program_data = include_bytes!("../../shell.bin");
            
            // Try ELF loader first, fallback to legacy loader
            match crate::kernel::process::elf_impl::validate_elf(program_data) {
                Ok(_) => {
                    crate::debug_println!("[create] Using ELF loader");
                    
                    // Print detailed ELF information
                    let _ = crate::kernel::process::elf_impl::print_elf_info(program_data);
                    
                    // Verify W^X property
                    match crate::kernel::process::elf_impl::verify_wx_separation(program_data) {
                        Ok(true) => crate::debug_println!("[SECURITY] W^X verification: PASSED"),
                        Ok(false) => crate::debug_println!("[SECURITY] W^X verification: FAILED"),
                        Err(_) => crate::debug_println!("[SECURITY] W^X verification: ERROR"),
                    }
                    
                    let loaded = crate::kernel::process::elf_impl::load_elf(
                        program_data,
                        &mut mapper,
                        frame_allocator,
                    ).map_err(|_| CreateError::PageTableCreationError("ELF load failed"))?;
                    
                    // Convert to LoadedProgram format
                    crate::kernel::loader::LoadedProgram {
                        entry_point: loaded.entry,
                        stack_top: loaded.stack_top,
                    }
                },
                Err(_) => {
                    crate::debug_println!("[create] Using legacy flat binary loader");
                    load_user_program(program_data, &mut mapper, frame_allocator)?
                }
            }
        }; // mapper dropped here
        
        crate::debug_println!("[create_user_process] PML4 Entry 0 after load: {:?}", l4_table[0]);
        
        // [PHASE 3 CRITICAL] Set USER_ACCESSIBLE on user page table hierarchy for 0x400000
        // This is needed because the ELF loader uses existing kernel PDPT/PD entries
        // which don't have USER_ACCESSIBLE flag
        unsafe {
            use x86_64::structures::paging::PageTableFlags;
            
            fn add_user_flag_to_entry(entry: &mut x86_64::structures::paging::page_table::PageTableEntry) {
                if !entry.is_unused() {
                    let old_flags = entry.flags();
                    if !old_flags.contains(PageTableFlags::USER_ACCESSIBLE) {
                        if let Ok(frame) = entry.frame() {
                            let new_flags = old_flags | PageTableFlags::USER_ACCESSIBLE;
                            entry.set_addr(frame.start_address(), new_flags);
                        }
                    }
                }
            }
            
            let phys_offset = VirtAddr::new(PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed));
            
            crate::debug_println!("[USER PT FIX] Setting USER_ACCESSIBLE on user page table hierarchy...");
            
            // PML4[1] for user code (0x8000000000)
            add_user_flag_to_entry(&mut l4_table[1]);
            
            // PDPT level
            if let Ok(pdpt_frame) = l4_table[1].frame() {
                let pdpt_ptr = (phys_offset + pdpt_frame.start_address().as_u64()).as_mut_ptr::<PageTable>();
                let pdpt = &mut *pdpt_ptr;
                add_user_flag_to_entry(&mut pdpt[0]);
                
                // PD level
                if let Ok(pd_frame) = pdpt[0].frame() {
                    let pd_ptr = (phys_offset + pd_frame.start_address().as_u64()).as_mut_ptr::<PageTable>();
                    let pd = &mut *pd_ptr;
                    add_user_flag_to_entry(&mut pd[0]); // 0x8000000000 is at start of PD
                    
                    // PT level
                    if let Ok(pt_frame) = pd[0].frame() {
                        let pt_ptr = (phys_offset + pt_frame.start_address().as_u64()).as_mut_ptr::<PageTable>();
                        let pt = &mut *pt_ptr;
                        // Map first few pages
                        for i in 0..16 {
                             add_user_flag_to_entry(&mut pt[i]);
                        }
                    }
                }
            }
            
            // PML4[223] for user stack (0x6ffffffff000)
            add_user_flag_to_entry(&mut l4_table[223]);
            
            if let Ok(pdpt_frame) = l4_table[223].frame() {
                let pdpt_ptr = (phys_offset + pdpt_frame.start_address().as_u64()).as_mut_ptr::<PageTable>();
                let pdpt = &mut *pdpt_ptr;
                add_user_flag_to_entry(&mut pdpt[511]);
                
                if let Ok(pd_frame) = pdpt[511].frame() {
                    let pd_ptr = (phys_offset + pd_frame.start_address().as_u64()).as_mut_ptr::<PageTable>();
                    let pd = &mut *pd_ptr;
                    add_user_flag_to_entry(&mut pd[511]);
                    
                    if let Ok(pt_frame) = pd[511].frame() {
                        let pt_ptr = (phys_offset + pt_frame.start_address().as_u64()).as_mut_ptr::<PageTable>();
                        let pt = &mut *pt_ptr;
                        // [CRITICAL FIX] Update ALL stack pages, not just PT[511]
                        for idx in 496..=511 {
                            if !pt[idx].is_unused() {
                                add_user_flag_to_entry(&mut pt[idx]);
                            }
                        }
                    }
                }
            }
        }
        
        // Update process entry point and stack
        let stack_top = loaded_program.stack_top.as_u64();
        
        // Setup arguments on stack
        // Stack layout (System V ABIish):
        // [ ... ]
        // [ str2 ]
        // [ str1 ]
        // [ str0 ]
        // [ argv[2] ] (null)
        // [ argv[1] ]
        // [ argv[0] ]
        // [ argc ]
        // RSP -> [ argc ]
        
        // 1. Calculate size needed
        let mut strings_size = 0;
        for arg in args {
            strings_size += arg.len() + 1; // +1 for null terminator
        }
        let argv_size = (args.len() + 1) * 8; // +1 for null terminator
        let total_size = 8 + argv_size + strings_size; // 8 for argc
        
        // Align stack to 16 bytes
        let mut current_rsp = stack_top;
        current_rsp -= total_size as u64;
        current_rsp &= !0xF; // Align down to 16 bytes
        
        // Scope 2: Stack setup (Re-create mapper)
        {
            let mut mapper = unsafe { OffsetPageTable::new(l4_table, phys_mem_offset) };
            
            // Write data to stack
            // We need a helper to write to user stack physical memory
            let write_to_user_stack = |addr: u64, data: &[u8]| {
                use x86_64::structures::paging::mapper::TranslateResult;
                use x86_64::structures::paging::Translate;
                
                let mut remaining = data.len();
                let mut data_offset = 0;
                let mut current_addr = addr;
                
                while remaining > 0 {
                    let page_addr = current_addr & !0xFFF;
                    let page_offset = current_addr & 0xFFF;
                    let space_in_page = 4096 - page_offset;
                    let chunk_size = core::cmp::min(remaining, space_in_page as usize);
                    
                    let virt = VirtAddr::new(page_addr);
                    match mapper.translate(virt) {
                        TranslateResult::Mapped { frame, .. } => {
                            let phys = frame.start_address().as_u64() + page_offset;
                            let dest_ptr = (phys_mem_offset.as_u64() + phys) as *mut u8;
                            unsafe {
                                core::ptr::copy_nonoverlapping(
                                    data.as_ptr().add(data_offset),
                                    dest_ptr,
                                    chunk_size
                                );
                            }
                        },
                        _ => {
                            crate::debug_println!("[create] Error: Stack page not mapped for arg writing at {:#x}", page_addr);
                            return; // Should error properly
                        }
                    }
                    
                    remaining -= chunk_size;
                    data_offset += chunk_size;
                    current_addr += chunk_size as u64;
                }
            };
            
            // Write strings
            let mut string_addrs = alloc::vec::Vec::new();
            let mut string_cursor = current_rsp + 8 + argv_size as u64;
            
            for arg in args {
                write_to_user_stack(string_cursor, arg.as_bytes());
                write_to_user_stack(string_cursor + arg.len() as u64, &[0]); // Null terminator
                string_addrs.push(string_cursor);
                string_cursor += (arg.len() + 1) as u64;
            }
            
            // Write argc
            let argc = args.len() as u64;
            write_to_user_stack(current_rsp, &argc.to_ne_bytes());
            
            // Write argv
            let mut argv_cursor = current_rsp + 8;
            for addr in string_addrs {
                write_to_user_stack(argv_cursor, &addr.to_ne_bytes());
                argv_cursor += 8;
            }
            // Write null pointer for argv end
            write_to_user_stack(argv_cursor, &[0u8; 8]);
            
            // Set registers
            // System V ABI: RDI=argc, RSI=argv
            process.registers_mut().rdi = argc;
            process.registers_mut().rsi = current_rsp + 8; // argv points to the array of pointers
            
            process.registers_mut().rip = loaded_program.entry_point.as_u64();
            process.registers_mut().rsp = current_rsp;
            
            crate::debug_println!("[Process] Stack setup: RSP={:#x}, argc={}, argv={:#x}", current_rsp, argc, current_rsp + 8);
        }
    }
    
    // Setup initial kernel stack context for switching
    crate::kernel::process::switch::setup_process_context(&mut process);
    
    process.set_state(ProcessState::Ready);
    
    // Extract info before moving process
    let entry_point = VirtAddr::new(process.registers().rip);
    let user_stack = VirtAddr::new(process.registers().rsp);
    let user_cr3 = process.page_table_phys_addr();
    
    // [PHASE 3] Map user code to kernel page table (workaround for CR3 switching)
    unsafe {
        let kernel_cr3: u64;
        core::arch::asm!("mov {}, cr3", out(reg) kernel_cr3, options(nomem, nostack));
        
        let phys_offset = crate::kernel::mm::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
        let phys_mem_offset = x86_64::VirtAddr::new(phys_offset);
        
        let kernel_l4_ptr = (phys_mem_offset + kernel_cr3).as_mut_ptr::<PageTable>();
        let kernel_l4 = &mut *kernel_l4_ptr;
        
        // Ensure PML4[0] has USER_ACCESSIBLE
        if !kernel_l4[0].is_unused() {
            let old_flags = kernel_l4[0].flags();
            let new_flags = old_flags | PageTableFlags::USER_ACCESSIBLE;
            let frame_addr = kernel_l4[0].addr();
            kernel_l4[0].set_addr(frame_addr, new_flags);
        }
        
        let mut kernel_mapper = OffsetPageTable::new(kernel_l4, phys_mem_offset);
        
        let user_l4_ptr = (phys_mem_offset + user_cr3).as_mut_ptr::<PageTable>();
        let user_l4 = &mut *user_l4_ptr;
        let user_mapper = OffsetPageTable::new(user_l4, phys_mem_offset);
        
        // Map user code pages
        let user_code_start = x86_64::VirtAddr::new(0x8000000000);
        let num_code_pages = 16;
        
        for i in 0..num_code_pages {
            let virt = user_code_start + (i * 4096u64);
            
            match user_mapper.translate(virt) {
                TranslateResult::Mapped { frame, .. } => {
                    let frame_addr = frame.start_address();
                    let page: Page = Page::containing_address(virt);
                    let phys_frame = PhysFrame::<Size4KiB>::containing_address(frame_addr);
                    let flags = PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE;
                    
                    if let TranslateResult::Mapped { .. } = kernel_mapper.translate(virt) {
                        // Already mapped
                    } else {
                        let _ = kernel_mapper.map_to(page, phys_frame, flags, frame_allocator)
                            .map(|f| f.flush());
                    }
                }
                _ => break,
            }
        }
        
        // Map user stack pages
        use crate::kernel::mm::user_paging::{DEFAULT_USER_STACK_SIZE, USER_STACK_TOP};
        let stack_top_for_mapping = x86_64::VirtAddr::new(USER_STACK_TOP);
        let first_mapped_page = stack_top_for_mapping - 4096u64;
        let user_stack_pages = DEFAULT_USER_STACK_SIZE / 4096;
        
        for i in 0..user_stack_pages {
            let virt = first_mapped_page - (i as u64 * 4096u64);
            
            match user_mapper.translate(virt) {
                TranslateResult::Mapped { frame, .. } => {
                    let frame_addr = frame.start_address();
                    let page: Page = Page::containing_address(virt);
                    let phys_frame = PhysFrame::<Size4KiB>::containing_address(frame_addr);
                    let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE;
                    
                    if let TranslateResult::Mapped { .. } = kernel_mapper.translate(virt) {
                        // Already mapped
                    } else {
                        let _ = kernel_mapper.map_to(page, phys_frame, flags, frame_allocator)
                            .map(|f| f.flush());
                    }
                }
                _ => break,
            }
        }
        
        // Helper function to add USER_ACCESSIBLE to page table entry
        fn add_user_accessible_to_entry(entry: &mut x86_64::structures::paging::page_table::PageTableEntry) {
            use x86_64::structures::paging::PageTableFlags;
            if !entry.is_unused() {
                let old_flags = entry.flags();
                if !old_flags.contains(PageTableFlags::USER_ACCESSIBLE) {
                    if let Ok(frame) = entry.frame() {
                        let new_flags = old_flags | PageTableFlags::USER_ACCESSIBLE;
                        entry.set_addr(frame.start_address(), new_flags);
                    }
                }
            }
        }
        
        // Update page table hierarchy for User code (0x400000)
        let pml4_idx_code = 0usize;
        let pdpt_idx_code = 0usize;
        let pd_idx_code = 2usize;
        
        add_user_accessible_to_entry(&mut kernel_l4[pml4_idx_code]);
        if let Ok(pdpt_frame) = kernel_l4[pml4_idx_code].frame() {
            let pdpt_ptr = (phys_mem_offset + pdpt_frame.start_address().as_u64()).as_mut_ptr::<PageTable>();
            let pdpt = &mut *pdpt_ptr;
            add_user_accessible_to_entry(&mut pdpt[pdpt_idx_code]);
            if let Ok(pd_frame) = pdpt[pdpt_idx_code].frame() {
                let pd_ptr = (phys_mem_offset + pd_frame.start_address().as_u64()).as_mut_ptr::<PageTable>();
                let pd = &mut *pd_ptr;
                add_user_accessible_to_entry(&mut pd[pd_idx_code]);
                if let Ok(pt_frame) = pd[pd_idx_code].frame() {
                    let pt_ptr = (phys_mem_offset + pt_frame.start_address().as_u64()).as_mut_ptr::<PageTable>();
                    let pt = &mut *pt_ptr;
                    add_user_accessible_to_entry(&mut pt[0]);
                }
            }
        }
        
        // Update page table hierarchy for User stack (0x6ffffffff000)
        let pml4_idx_stack = 223usize;
        let pdpt_idx_stack = 511usize;
        let pd_idx_stack = 511usize;
        
        add_user_accessible_to_entry(&mut kernel_l4[pml4_idx_stack]);
        if let Ok(pdpt_frame) = kernel_l4[pml4_idx_stack].frame() {
            let pdpt_ptr = (phys_mem_offset + pdpt_frame.start_address().as_u64()).as_mut_ptr::<PageTable>();
            let pdpt = &mut *pdpt_ptr;
            add_user_accessible_to_entry(&mut pdpt[pdpt_idx_stack]);
            if let Ok(pd_frame) = pdpt[pdpt_idx_stack].frame() {
                let pd_ptr = (phys_mem_offset + pd_frame.start_address().as_u64()).as_mut_ptr::<PageTable>();
                let pd = &mut *pd_ptr;
                add_user_accessible_to_entry(&mut pd[pd_idx_stack]);
                if let Ok(pt_frame) = pd[pd_idx_stack].frame() {
                    let pt_ptr = (phys_mem_offset + pt_frame.start_address().as_u64()).as_mut_ptr::<PageTable>();
                    let pt = &mut *pt_ptr;
                    for idx in 496..=511 {
                        if !pt[idx].is_unused() {
                            add_user_accessible_to_entry(&mut pt[idx]);
                        }
                    }
                }
            }
        }
        
        x86_64::instructions::tlb::flush_all();
    }
    
    // Initialize standard I/O capabilities (stdin=0, stdout=1, stderr=2)
    if let Err(e) = process.init_stdio_capabilities() {
        crate::debug_println!("[Process] Warning: Failed to init stdio capabilities: {:?}", e);
    }
    
    // 3. Add to process table
    {
        let mut table = PROCESS_TABLE.lock();
        table.add_process(process);
    }
    
    crate::debug_println!("[Process] Created process PID={}", pid.as_u64());
    
    Ok((pid, entry_point, user_stack, user_cr3))
}

/// Spawn a new process (syscall interface)
pub fn spawn_process(path: &str, args: &[&str]) -> Result<ProcessId, CreateError> {
    let (pid, _, _, _) = create_user_process(path, args)?;
    Ok(pid)
}

/// Terminate a process
pub fn terminate_process(pid: ProcessId, exit_code: i32) {
    // First, update process state and notify parent
    let parent_pid = {
        let mut table = PROCESS_TABLE.lock();
        
        if let Some(process) = table.get_process_mut(pid) {
            process.set_state(ProcessState::Terminated);
            process.set_exit_code(exit_code);
            
            let parent_pid = process.parent_pid();
            
            crate::debug_println!(
                "[Process] Terminated PID={} with code={}",
                pid.as_u64(),
                exit_code
            );
            
            parent_pid
        } else {
            return;
        }
    };
    
    // Wake up parent if it's blocked (in a separate scope to avoid double borrow)
    if let Some(ppid) = parent_pid {
        let mut table = PROCESS_TABLE.lock();
        if let Some(parent) = table.get_process_mut(ppid) {
            if parent.state() == ProcessState::Blocked {
                parent.set_state(ProcessState::Ready);
            }
        }
    }
    
    // Free process resources
    {
        let mut table = PROCESS_TABLE.lock();
        if let Some(process) = table.get_process_mut(pid) {
            free_process_resources(process);
        }
    }
}

/// Free resources associated with a terminated process
///
/// This includes:
/// - User page table and all user-space pages
/// - Kernel stack
/// - File descriptors
fn free_process_resources(process: &mut crate::kernel::process::Process) {
    let phys_mem_offset = VirtAddr::new(PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed));
    
    // 1. Free user page table and all mapped user pages
    let page_table_frame = process.page_table_frame();
    
    unsafe {
        // Access the L4 page table
        let l4_table_ptr = (phys_mem_offset + page_table_frame.start_address().as_u64()).as_mut_ptr::<PageTable>();
        let l4_table = &mut *l4_table_ptr;
        
        // Recursively free all user-space entries (indices 0-255)
        // This walks through all levels (L4 -> L3 -> L2 -> L1) and frees both
        // the page table frames and the actual data frames
        for l4_index in 0..256 {
            if !l4_table[l4_index].is_unused() {
                let l3_frame = l4_table[l4_index].frame().unwrap();
                free_l3_table(l3_frame, phys_mem_offset);
                l4_table[l4_index].set_unused();
                
                // Free the L3 table frame itself
                if let Some(mut allocator) = BOOT_INFO_ALLOCATOR.try_lock() {
                    if let Some(ref mut alloc) = *allocator {
                        unsafe {
                            alloc.deallocate_frame(l3_frame);
                        }
                    }
                }
            }
        }
        
        // Free the L4 table frame itself
        if let Some(mut allocator) = BOOT_INFO_ALLOCATOR.try_lock() {
            if let Some(ref mut alloc) = *allocator {
                unsafe {
                    alloc.deallocate_frame(page_table_frame);
                }
            }
        }
    }
    
    // 2. Free kernel stack
    // Note: The kernel stack was allocated from the global allocator
    let kernel_stack_top = process.kernel_stack;
    let kernel_stack_size = 16 * 1024; // 16KB stack
    let kernel_stack_bottom = kernel_stack_top.as_u64() - kernel_stack_size;
    
    unsafe {
        use alloc::alloc::{dealloc, Layout};
        if let Ok(kernel_stack_layout) = Layout::from_size_align(kernel_stack_size as usize, 16) {
            dealloc(kernel_stack_bottom as *mut u8, kernel_stack_layout);
        }
    }
    
    // 3. Clear capability table (closes all resources)
    process.capability_table().clear();
    
    crate::debug_println!("[Process] Freed resources for PID={}", process.pid().as_u64());
}

/// Recursively free an L3 page table and all its children
unsafe fn free_l3_table(l3_frame: PhysFrame<Size4KiB>, phys_mem_offset: VirtAddr) {
    unsafe {
        let l3_table_ptr = (phys_mem_offset + l3_frame.start_address().as_u64()).as_mut_ptr::<PageTable>();
        let l3_table = &mut *l3_table_ptr;
        
        for l3_index in 0..512 {
            if !l3_table[l3_index].is_unused() {
                let l2_frame = l3_table[l3_index].frame().unwrap();
                free_l2_table(l2_frame, phys_mem_offset);
                l3_table[l3_index].set_unused();
                
                // Free the L2 table frame itself
                if let Some(mut allocator) = BOOT_INFO_ALLOCATOR.try_lock() {
                    if let Some(ref mut alloc) = *allocator {
                        alloc.deallocate_frame(l2_frame);
                    }
                }
            }
        }
    }
}

/// Recursively free an L2 page table and all its children
unsafe fn free_l2_table(l2_frame: PhysFrame<Size4KiB>, phys_mem_offset: VirtAddr) {
    unsafe {
        let l2_table_ptr = (phys_mem_offset + l2_frame.start_address().as_u64()).as_mut_ptr::<PageTable>();
        let l2_table = &mut *l2_table_ptr;
        
        for l2_index in 0..512 {
            if !l2_table[l2_index].is_unused() {
                // Check if this is a huge page (2MB)
                let flags = l2_table[l2_index].flags();
                if flags.contains(PageTableFlags::HUGE_PAGE) {
                    // This is a 2MB huge page, free it directly
                    let frame = l2_table[l2_index].frame().unwrap();
                    if let Some(mut allocator) = BOOT_INFO_ALLOCATOR.try_lock() {
                        if let Some(ref mut alloc) = *allocator {
                            alloc.deallocate_frame(frame);
                        }
                    }
                } else {
                    // This is a pointer to an L1 table
                    let l1_frame = l2_table[l2_index].frame().unwrap();
                    free_l1_table(l1_frame, phys_mem_offset);
                    
                    // Free the L1 table frame itself
                    if let Some(mut allocator) = BOOT_INFO_ALLOCATOR.try_lock() {
                        if let Some(ref mut alloc) = *allocator {
                            alloc.deallocate_frame(l1_frame);
                        }
                    }
                }
                l2_table[l2_index].set_unused();
            }
        }
    }
}

/// Free an L1 page table and all mapped pages
unsafe fn free_l1_table(l1_frame: PhysFrame<Size4KiB>, phys_mem_offset: VirtAddr) {
    unsafe {
        let l1_table_ptr = (phys_mem_offset + l1_frame.start_address().as_u64()).as_mut_ptr::<PageTable>();
        let l1_table = &mut *l1_table_ptr;
        
        for l1_index in 0..512 {
            if !l1_table[l1_index].is_unused() {
                // This is an actual data page, free it
                let frame = l1_table[l1_index].frame().unwrap();
                if let Some(mut allocator) = BOOT_INFO_ALLOCATOR.try_lock() {
                    if let Some(ref mut alloc) = *allocator {
                        alloc.deallocate_frame(frame);
                    }
                }
                l1_table[l1_index].set_unused();
            }
        }
    }
}
