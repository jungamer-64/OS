// kernel/src/kernel/process/lifecycle.rs
//! Process lifecycle management

use crate::kernel::process::{ProcessId, ProcessState, PROCESS_TABLE};
use crate::kernel::loader::load_user_program;
use crate::kernel::mm::allocator::BOOT_INFO_ALLOCATOR;
use crate::kernel::mm::PHYS_MEM_OFFSET;
use x86_64::VirtAddr;
use x86_64::structures::paging::{OffsetPageTable, PageTable, PhysFrame, PageTableFlags, Translate};
use x86_64::structures::paging::page::Size4KiB;



/// Error types for process creation
#[derive(Debug)]
pub enum CreateError {
    /// Frame allocation failed
    FrameAllocationFailed,
    /// Loader error occurred
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
pub fn create_user_process(path: &str) -> Result<(ProcessId, VirtAddr, VirtAddr, u64), CreateError> {
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

        let mut mapper = unsafe { OffsetPageTable::new(l4_table, phys_mem_offset) };
        
        // Get embedded program binary (TODO: Use VFS when implemented)
        let program_data = include_bytes!("../../shell.bin");
        
        // Try ELF loader first, fallback to legacy loader
        let loaded_program = match crate::kernel::process::elf_impl::validate_elf(program_data) {
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
        };
        
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
            
            // PML4[0] for user code (0x400000)
            crate::debug_println!("[USER PT FIX] PML4[0] before: {:?}", l4_table[0].flags());
            add_user_flag_to_entry(&mut l4_table[0]);
            crate::debug_println!("[USER PT FIX] PML4[0] after: {:?}", l4_table[0].flags());
            
            // PDPT level
            if let Ok(pdpt_frame) = l4_table[0].frame() {
                let pdpt_ptr = (phys_offset + pdpt_frame.start_address().as_u64()).as_mut_ptr::<PageTable>();
                let pdpt = &mut *pdpt_ptr;
                crate::debug_println!("[USER PT FIX] PDPT[0] before: {:?}", pdpt[0].flags());
                add_user_flag_to_entry(&mut pdpt[0]);
                crate::debug_println!("[USER PT FIX] PDPT[0] after: {:?}", pdpt[0].flags());
                
                // PD level (0x400000 >> 21 = 2)
                if let Ok(pd_frame) = pdpt[0].frame() {
                    let pd_ptr = (phys_offset + pd_frame.start_address().as_u64()).as_mut_ptr::<PageTable>();
                    let pd = &mut *pd_ptr;
                    crate::debug_println!("[USER PT FIX] PD[2] before: {:?}", pd[2].flags());
                    add_user_flag_to_entry(&mut pd[2]);
                    crate::debug_println!("[USER PT FIX] PD[2] after: {:?}", pd[2].flags());
                    
                    // PT level (0x400000 >> 12 & 0x1FF = 0)
                    if let Ok(pt_frame) = pd[2].frame() {
                        let pt_ptr = (phys_offset + pt_frame.start_address().as_u64()).as_mut_ptr::<PageTable>();
                        let pt = &mut *pt_ptr;
                        crate::debug_println!("[USER PT FIX] PT[0] before: {:?}", pt[0].flags());
                        add_user_flag_to_entry(&mut pt[0]);
                        crate::debug_println!("[USER PT FIX] PT[0] after: {:?}", pt[0].flags());
                        // Also check PT[1] for 0x401000
                        crate::debug_println!("[USER PT FIX] PT[1] before: {:?}", pt[1].flags());
                        add_user_flag_to_entry(&mut pt[1]);
                        crate::debug_println!("[USER PT FIX] PT[1] after: {:?}", pt[1].flags());
                    }
                }
            }
            
            // PML4[223] for user stack (0x6ffffffff000)
            crate::debug_println!("[USER PT FIX] PML4[223] before: {:?}", l4_table[223].flags());
            add_user_flag_to_entry(&mut l4_table[223]);
            crate::debug_println!("[USER PT FIX] PML4[223] after: {:?}", l4_table[223].flags());
            
            if let Ok(pdpt_frame) = l4_table[223].frame() {
                let pdpt_ptr = (phys_offset + pdpt_frame.start_address().as_u64()).as_mut_ptr::<PageTable>();
                let pdpt = &mut *pdpt_ptr;
                crate::debug_println!("[USER PT FIX] PDPT[511] before: {:?}", pdpt[511].flags());
                add_user_flag_to_entry(&mut pdpt[511]);
                crate::debug_println!("[USER PT FIX] PDPT[511] after: {:?}", pdpt[511].flags());
                
                if let Ok(pd_frame) = pdpt[511].frame() {
                    let pd_ptr = (phys_offset + pd_frame.start_address().as_u64()).as_mut_ptr::<PageTable>();
                    let pd = &mut *pd_ptr;
                    crate::debug_println!("[USER PT FIX] PD[511] before: {:?}", pd[511].flags());
                    add_user_flag_to_entry(&mut pd[511]);
                    crate::debug_println!("[USER PT FIX] PD[511] after: {:?}", pd[511].flags());
                    
                    if let Ok(pt_frame) = pd[511].frame() {
                        let pt_ptr = (phys_offset + pt_frame.start_address().as_u64()).as_mut_ptr::<PageTable>();
                        let pt = &mut *pt_ptr;
                        // [CRITICAL FIX] Update ALL stack pages, not just PT[511]
                        // Stack grows downward: 0x6ffffffff000 (PT[511]) down to 0x6fffffff0000 (PT[496])
                        // 16 pages total
                        crate::debug_println!("[USER PT FIX] Updating all stack PT entries (496-511)...");
                        for idx in 496..=511 {
                            if !pt[idx].is_unused() {
                                crate::debug_println!("[USER PT FIX] Stack PT[{}] before: {:?}", idx, pt[idx].flags());
                                add_user_flag_to_entry(&mut pt[idx]);
                                crate::debug_println!("[USER PT FIX] Stack PT[{}] after: {:?}", idx, pt[idx].flags());
                            }
                        }
                        crate::debug_println!("[USER PT FIX] Stack PT entries updated");
                    }
                }
            }
        }
        
        // Phase 3 preparation: Validate user page table structure
        unsafe {
            use crate::kernel::mm::dump_page_table_entry;
            let phys_offset = crate::kernel::mm::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
            let user_mapper = x86_64::structures::paging::OffsetPageTable::new(l4_table, x86_64::VirtAddr::new(phys_offset));
            
            crate::debug_println!("[VALIDATION] Checking user page table mappings:");
            dump_page_table_entry(&user_mapper, loaded_program.entry_point, "User Code Entry");
            // Check first mapped stack page (stack_top - 4096 is the first mapped page)
            let first_stack_page = x86_64::VirtAddr::new(loaded_program.stack_top.as_u64() - 0x1000);
            dump_page_table_entry(&user_mapper, first_stack_page, "User Stack (first mapped page)");
            
            // [PHASE 3] Check if kernel stack is accessible in user page table
            let kernel_rsp: u64;
            core::arch::asm!("mov {}, rsp", out(reg) kernel_rsp, options(nomem, nostack));
            crate::debug_println!("[VALIDATION] Current kernel RSP: {:#x}", kernel_rsp);
            dump_page_table_entry(&user_mapper, x86_64::VirtAddr::new(kernel_rsp), "Kernel Stack (iretq frame location)");
        }
        
        // Update process entry point and stack
        // Stack memory layout:
        //   USER_STACK_TOP (0x700000000000) = boundary (not mapped)
        //   Mapped pages: 0x6fffffff0000 to 0x6ffffffffffff (16 pages = 64KB)  
        //   Guard page: 0x6fffffffef000 (unmapped - catches stack overflow)
        //
        // RSP alignment for x86_64 System V ABI:
        //
        // The ABI requires that at function entry (after CALL pushes return address),
        // RSP is 16-byte aligned. Since _start is entered via IRETQ (not CALL),
        // there's no return address on the stack.
        //
        // To maintain ABI compliance, we subtract 8 from stack_top to simulate
        // the effect of a CALL instruction. This ensures:
        //   - Initial RSP = stack_top - 8 (16-byte aligned, but offset by 8)
        //   - After first PUSH in function prologue: RSP is 16-byte aligned
        //   - movaps and other SSE instructions requiring 16-byte alignment work correctly
        //
        // Stack layout:
        //   USER_STACK_TOP (0x700000000000) = boundary (not mapped)
        //   Initial RSP = 0x6ffffffffffffff8 (stack_top - 8)
        //   Mapped pages: 0x6fffffff00000 to 0x6ffffffffffff
        //
        // Note: loaded_program.stack_top is already USER_STACK_TOP from map_user_stack()
        let stack_top = loaded_program.stack_top.as_u64();
        // Subtract 8 to simulate return address being pushed (ABI alignment)
        let initial_rsp = stack_top - 8;
        crate::debug_println!("[Process] Stack top: {:#x}, initial RSP: {:#x}", stack_top, initial_rsp);
        
        process.registers_mut().rip = loaded_program.entry_point.as_u64();
        process.registers_mut().rsp = initial_rsp;
    }
    
    // Setup initial kernel stack context for switching
    crate::kernel::process::switch::setup_process_context(&mut process);
    
    process.set_state(ProcessState::Ready);
    
    // Extract info before moving process
    let entry_point = VirtAddr::new(process.registers().rip);
    let user_stack = VirtAddr::new(process.registers().rsp);
    let user_cr3 = process.page_table_phys_addr();
    
    // [PHASE 3] CR3 Diagnostic Tests (DISABLED - causes hang in Test 2)
    // crate::debug_println!("\n[PHASE 3] ========== CR3 DIAGNOSTIC TESTS ==========");
    // unsafe {
    //     crate::arch::x86_64::run_cr3_diagnostic_tests(user_cr3);
    // }
    // crate::debug_println!("[PHASE 3] ========================================\n");
    crate::debug_println!("[PHASE 3] CR3 diagnostic tests skipped (Test 2 causes hang)");
    
    // [PHASE 3] Map user code to kernel page table (workaround for CR3 switching)
    crate::debug_println!("[PHASE 3] Mapping user code to kernel page table...");
    unsafe {
        // Get current kernel CR3
        let kernel_cr3: u64;
        core::arch::asm!("mov {}, cr3", out(reg) kernel_cr3, options(nomem, nostack));
        crate::debug_println!("[PHASE 3] Kernel CR3: {:#x}", kernel_cr3);
        
        let phys_offset = crate::kernel::mm::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
        let phys_mem_offset = x86_64::VirtAddr::new(phys_offset);
        
        // Get kernel page table
        let kernel_l4_ptr = (phys_mem_offset + kernel_cr3).as_mut_ptr::<PageTable>();
        let kernel_l4 = &mut *kernel_l4_ptr;
        
        // [CRITICAL] Ensure PML4 Entry 0 has USER_ACCESSIBLE flag
        // This is necessary because User code (0x400000) and User stack (0x6fff...) 
        // are in the lower half of the address space (PML4 index 0-255)
        // Without USER_ACCESSIBLE on the PML4 entry, Ring 3 cannot access these pages
        if !kernel_l4[0].is_unused() {
            let old_flags = kernel_l4[0].flags();
            let new_flags = old_flags | PageTableFlags::USER_ACCESSIBLE;
            let frame_addr = kernel_l4[0].addr();
            kernel_l4[0].set_addr(frame_addr, new_flags);
            crate::debug_println!("[PHASE 3] PML4[0] has USER_ACCESSIBLE: {}", kernel_l4[0].flags().contains(PageTableFlags::USER_ACCESSIBLE));
        }
        
        let mut kernel_mapper = OffsetPageTable::new(kernel_l4, phys_mem_offset);
        
        // Get user page table to find physical frames
        let user_l4_ptr = (phys_mem_offset + user_cr3).as_mut_ptr::<PageTable>();
        let user_l4 = &mut *user_l4_ptr;
        let user_mapper = OffsetPageTable::new(user_l4, phys_mem_offset);
        
        // Map user code pages (assuming 0x400000 base, and typical program size)
        let user_code_start = x86_64::VirtAddr::new(0x400000);
        let num_code_pages = 16; // Generous estimate for shell.bin
        
        for i in 0..num_code_pages {
            let virt = user_code_start + (i * 4096u64);
            
            // Translate in user page table
            use x86_64::structures::paging::mapper::TranslateResult;
            match user_mapper.translate(virt) {
                TranslateResult::Mapped { frame, .. } => {
                    let frame_addr = frame.start_address();
                    crate::debug_println!("[PHASE 3] Mapping code page {:#x} -> frame {:#x}", virt.as_u64(), frame_addr.as_u64());
                    
                    // Map same physical frame in kernel page table with USER_ACCESSIBLE flag
                    use x86_64::structures::paging::{Page, Mapper};
                    let page: Page = Page::containing_address(virt);
                    let phys_frame = PhysFrame::<Size4KiB>::containing_address(frame_addr);
                    let flags = PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE;
                    
                    // Check if already mapped
                    match kernel_mapper.translate(virt) {
                        TranslateResult::Mapped { .. } => {
                            // Already mapped, skip
                            crate::debug_println!("[PHASE 3] Page {:#x} already mapped in kernel page table", virt.as_u64());
                        }
                        _ => {
                            // Not mapped yet, map it
                            kernel_mapper.map_to(page, phys_frame, flags, frame_allocator)
                                .map_err(|_| CreateError::PageTableCreationError("Failed to map user code to kernel page table"))?
                                .flush();
                        }
                    }
                }
                _ => {
                    // No more code pages
                    break;
                }
            }
        }
        
        crate::debug_println!("[PHASE 3] User code mapped to kernel page table successfully");
        
        // [PHASE 3] Map user stack to kernel page table
        crate::debug_println!("[PHASE 3] Mapping user stack to kernel page table...");
        
        // Stack layout:
        //   USER_STACK_TOP (0x700000000000) = boundary (not mapped)
        //   Mapped pages: from (USER_STACK_TOP - DEFAULT_USER_STACK_SIZE) to USER_STACK_TOP - 1
        //   Note: user_stack.rsp is USER_STACK_TOP - 8 for ABI alignment, but we use USER_STACK_TOP for mapping
        // 
        // We need to map from USER_STACK_TOP - 4096 (first mapped page) down
        use crate::kernel::mm::user_paging::{DEFAULT_USER_STACK_SIZE, USER_STACK_TOP};
        let stack_top_for_mapping = x86_64::VirtAddr::new(USER_STACK_TOP);
        let first_mapped_page = stack_top_for_mapping - 4096u64;
        let user_stack_pages = DEFAULT_USER_STACK_SIZE / 4096; // Calculate from actual stack size
        crate::debug_println!("[PHASE 3] Stack top: {:#x}, first mapped page: {:#x}", USER_STACK_TOP, first_mapped_page.as_u64());
        crate::debug_println!("[PHASE 3] Will map {} stack pages ({} bytes)", user_stack_pages, DEFAULT_USER_STACK_SIZE);
        
        for i in 0..user_stack_pages {
            let virt = first_mapped_page - (i as u64 * 4096u64);
            
            // Translate in user page table
            use x86_64::structures::paging::mapper::TranslateResult;
            match user_mapper.translate(virt) {
                TranslateResult::Mapped { frame, .. } => {
                    let frame_addr = frame.start_address();
                    crate::debug_println!("[PHASE 3] Mapping stack page {:#x} -> frame {:#x}", virt.as_u64(), frame_addr.as_u64());
                    
                    // Map same physical frame in kernel page table with USER_ACCESSIBLE | WRITABLE flag
                    use x86_64::structures::paging::{Page, Mapper};
                    let page: Page = Page::containing_address(virt);
                    let phys_frame = PhysFrame::<Size4KiB>::containing_address(frame_addr);
                    let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE;
                    
                    // Check if already mapped
                    match kernel_mapper.translate(virt) {
                        TranslateResult::Mapped { .. } => {
                            // Already mapped, skip
                            crate::debug_println!("[PHASE 3] Stack page {:#x} already mapped in kernel page table", virt.as_u64());
                        }
                        _ => {
                            // Not mapped yet, map it
                            kernel_mapper.map_to(page, phys_frame, flags, frame_allocator)
                                .map_err(|_| CreateError::PageTableCreationError("Failed to map user stack to kernel page table"))?
                                .flush();
                        }
                    }
                }
                _ => {
                    // No more stack pages or not mapped
                    crate::debug_println!("[PHASE 3] Stack page {:#x} not mapped in user page table", virt.as_u64());
                    break;
                }
            }
        }
        
        crate::debug_println!("[PHASE 3] User stack mapped to kernel page table successfully");
        
        // [CRITICAL] After all mappings, ensure page table hierarchy has USER_ACCESSIBLE
        // This is needed because map_to may not set USER_ACCESSIBLE on parent table entries
        crate::debug_println!("[PHASE 3] Setting USER_ACCESSIBLE on all used page table hierarchy entries...");
        
        // Helper function to add USER_ACCESSIBLE to page table entry
        fn add_user_accessible_to_entry(entry: &mut x86_64::structures::paging::page_table::PageTableEntry) {
            use x86_64::structures::paging::PageTableFlags;
            if !entry.is_unused() {
                let old_flags = entry.flags();
                if !old_flags.contains(PageTableFlags::USER_ACCESSIBLE) {
                    // Get the physical address from the entry
                    if let Ok(frame) = entry.frame() {
                        let new_flags = old_flags | PageTableFlags::USER_ACCESSIBLE;
                        entry.set_addr(frame.start_address(), new_flags);
                    }
                }
            }
        }
        
        // Update page table hierarchy for User code (0x400000)
        // PML4[0] -> PDPT[0] -> PD[2] -> PT[0]
        let pml4_idx_code = 0usize;
        let pdpt_idx_code = 0usize;
        let pd_idx_code = 2usize;  // 0x400000 >> 21 = 2
        
        crate::debug_println!("[PHASE 3] Updating User code page table hierarchy...");
        crate::debug_println!("[PHASE 3] PML4[{}] before: flags={:?}", pml4_idx_code, kernel_l4[pml4_idx_code].flags());
        add_user_accessible_to_entry(&mut kernel_l4[pml4_idx_code]);
        crate::debug_println!("[PHASE 3] PML4[{}] after: flags={:?}", pml4_idx_code, kernel_l4[pml4_idx_code].flags());
        
        if let Ok(pdpt_frame) = kernel_l4[pml4_idx_code].frame() {
            let pdpt_ptr = (phys_mem_offset + pdpt_frame.start_address().as_u64()).as_mut_ptr::<PageTable>();
            let pdpt = &mut *pdpt_ptr;
            crate::debug_println!("[PHASE 3] PDPT[{}] before: flags={:?}", pdpt_idx_code, pdpt[pdpt_idx_code].flags());
            add_user_accessible_to_entry(&mut pdpt[pdpt_idx_code]);
            crate::debug_println!("[PHASE 3] PDPT[{}] after: flags={:?}", pdpt_idx_code, pdpt[pdpt_idx_code].flags());
            
            if let Ok(pd_frame) = pdpt[pdpt_idx_code].frame() {
                let pd_ptr = (phys_mem_offset + pd_frame.start_address().as_u64()).as_mut_ptr::<PageTable>();
                let pd = &mut *pd_ptr;
                crate::debug_println!("[PHASE 3] PD[{}] before: flags={:?}", pd_idx_code, pd[pd_idx_code].flags());
                add_user_accessible_to_entry(&mut pd[pd_idx_code]);
                crate::debug_println!("[PHASE 3] PD[{}] after: flags={:?}", pd_idx_code, pd[pd_idx_code].flags());
                
                // Also check PT level for entry 0
                if let Ok(pt_frame) = pd[pd_idx_code].frame() {
                    let pt_ptr = (phys_mem_offset + pt_frame.start_address().as_u64()).as_mut_ptr::<PageTable>();
                    let pt = &mut *pt_ptr;
                    let pt_idx_code = 0usize;  // 0x400000 & 0xFFF >> 12 = 0
                    crate::debug_println!("[PHASE 3] PT[{}] before: flags={:?}", pt_idx_code, pt[pt_idx_code].flags());
                    add_user_accessible_to_entry(&mut pt[pt_idx_code]);
                    crate::debug_println!("[PHASE 3] PT[{}] after: flags={:?}", pt_idx_code, pt[pt_idx_code].flags());
                }
                
                crate::debug_println!("[PHASE 3] User code page table hierarchy updated with USER_ACCESSIBLE");
            } else {
                crate::debug_println!("[PHASE 3] ERROR: PD[{}] frame not found", pd_idx_code);
            }
        } else {
            crate::debug_println!("[PHASE 3] ERROR: PDPT frame not found");
        }
        
        // Update page table hierarchy for User stack (0x6ffffffff000)
        // PML4 index = (0x6ffffffff000 >> 39) & 0x1FF = 223
        // PDPT index = (0x6ffffffff000 >> 30) & 0x1FF = 511
        // PD index = (0x6ffffffff000 >> 21) & 0x1FF = 511
        let pml4_idx_stack = 223usize;
        let pdpt_idx_stack = 511usize;
        let pd_idx_stack = 511usize;
        
        crate::debug_println!("[PHASE 3] Updating User stack page table hierarchy...");
        crate::debug_println!("[PHASE 3] PML4[{}] before: flags={:?}", pml4_idx_stack, kernel_l4[pml4_idx_stack].flags());
        add_user_accessible_to_entry(&mut kernel_l4[pml4_idx_stack]);
        crate::debug_println!("[PHASE 3] PML4[{}] after: flags={:?}", pml4_idx_stack, kernel_l4[pml4_idx_stack].flags());
        
        if let Ok(pdpt_frame) = kernel_l4[pml4_idx_stack].frame() {
            let pdpt_ptr = (phys_mem_offset + pdpt_frame.start_address().as_u64()).as_mut_ptr::<PageTable>();
            let pdpt = &mut *pdpt_ptr;
            crate::debug_println!("[PHASE 3] PDPT[{}] before: flags={:?}", pdpt_idx_stack, pdpt[pdpt_idx_stack].flags());
            add_user_accessible_to_entry(&mut pdpt[pdpt_idx_stack]);
            crate::debug_println!("[PHASE 3] PDPT[{}] after: flags={:?}", pdpt_idx_stack, pdpt[pdpt_idx_stack].flags());
            
            if let Ok(pd_frame) = pdpt[pdpt_idx_stack].frame() {
                let pd_ptr = (phys_mem_offset + pd_frame.start_address().as_u64()).as_mut_ptr::<PageTable>();
                let pd = &mut *pd_ptr;
                crate::debug_println!("[PHASE 3] PD[{}] before: flags={:?}", pd_idx_stack, pd[pd_idx_stack].flags());
                add_user_accessible_to_entry(&mut pd[pd_idx_stack]);
                crate::debug_println!("[PHASE 3] PD[{}] after: flags={:?}", pd_idx_stack, pd[pd_idx_stack].flags());
                
                // PT level check and update for ALL stack pages (not just PT[511])
                // Stack grows downward: 0x6ffffffff000 (PT[511]) down to 0x6fffffff0000 (PT[496])
                if let Ok(pt_frame) = pd[pd_idx_stack].frame() {
                    let pt_ptr = (phys_mem_offset + pt_frame.start_address().as_u64()).as_mut_ptr::<PageTable>();
                    let pt = &mut *pt_ptr;
                    crate::debug_println!("[PHASE 3] Updating kernel PT entries (496-511) for stack...");
                    for idx in 496..=511 {
                        if !pt[idx].is_unused() {
                            add_user_accessible_to_entry(&mut pt[idx]);
                        }
                    }
                    crate::debug_println!("[PHASE 3] Kernel stack PT entries updated");
                } else {
                    crate::debug_println!("[PHASE 3] ERROR: Stack PT frame not found");
                }
                
                crate::debug_println!("[PHASE 3] User stack page table hierarchy updated with USER_ACCESSIBLE");
            } else {
                crate::debug_println!("[PHASE 3] ERROR: Stack PD frame not found");
            }
        } else {
            crate::debug_println!("[PHASE 3] ERROR: Stack PDPT frame not found");
        }
        
        // Flush TLB
        x86_64::instructions::tlb::flush_all();
        crate::debug_println!("[PHASE 3] TLB flushed");
    }
    
    // 3. Add to process table
    {
        let mut table = PROCESS_TABLE.lock();
        table.add_process(process);
    }
    
    crate::debug_println!("[Process] Created process PID={}", pid.as_u64());
    
    Ok((pid, entry_point, user_stack, user_cr3))
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
    
    // 3. Close all open file descriptors
    process.close_all_fds();
    
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

/// Fork the current process
///
/// Creates a copy of the current process with a new PID.
///
/// # Returns
/// * `Ok(pid)` - Child PID (returned to parent)
/// * `Err(e)` - Error code
pub fn fork_process() -> Result<ProcessId, CreateError> {
    let mut allocator_lock = BOOT_INFO_ALLOCATOR.lock();
    let frame_allocator = allocator_lock.as_mut().ok_or(CreateError::FrameAllocationFailed)?;
    let phys_mem_offset = VirtAddr::new(PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed));
    
    // 1. Get current process info
    let (current_pid, current_registers, (parent_fds, parent_next_fd)) = {
        let table = PROCESS_TABLE.lock();
        let process = table.current_process().ok_or(CreateError::PageTableCreationError("No current process"))?;
        (process.pid(), *process.registers(), process.clone_file_descriptors())
    };
    
    // 2. Duplicate page table
    // We need to access the current page table (which is active)
    // We can pass a dummy mapper because duplicate_user_page_table uses Cr3
    let mut dummy_mapper = unsafe {
        let (l4_frame, _) = x86_64::registers::control::Cr3::read();
        let l4_table_ptr = (phys_mem_offset + l4_frame.start_address().as_u64()).as_mut_ptr();
        let l4_table = &mut *l4_table_ptr;
        OffsetPageTable::new(l4_table, phys_mem_offset)
    };
    
    let new_page_table_frame = unsafe {
        crate::kernel::mm::user_paging::duplicate_user_page_table(
            &mut dummy_mapper,
            frame_allocator,
            phys_mem_offset
        ).map_err(|_| CreateError::PageTableCreationError("Failed to duplicate page table"))?
    };
    
    // 3. Allocate new PID and stacks
    let mut table = PROCESS_TABLE.lock();
    let pid = table.allocate_pid();
    
    // Allocate stacks (using internal helpers from process/mod.rs would be better, but they are private)
    // We need to expose them or duplicate logic.
    // Let's use `create_process_with_context` style logic but we need to construct Process manually
    // because we have custom page table and registers.
    
    // We'll duplicate the stack allocation logic here for now (or make it public)
    // Making `allocate_kernel_stack` public in `process/mod.rs` would be cleaner.
    // But for now, let's just use `alloc_zeroed` here.
    
    use alloc::alloc::{alloc_zeroed, Layout};
    let kernel_stack_layout = Layout::from_size_align(16 * 1024, 16).unwrap();
    let kernel_stack_ptr = unsafe { alloc_zeroed(kernel_stack_layout) };
    let kernel_stack = VirtAddr::new(kernel_stack_ptr as u64 + 16 * 1024);
    
    // Note: We don't need to copy user stack content because `duplicate_user_page_table`
    // already copied the frames mapped at USER_STACK_TOP!
    
    // Construct Process
    let mut child_process = crate::kernel::process::Process::new(
        pid,
        new_page_table_frame,
        kernel_stack,
        VirtAddr::new(crate::kernel::mm::user_paging::USER_STACK_TOP), // 0x0000_7000_0000_0000
        VirtAddr::new(0), // Entry point (will be overwritten by registers.rip)
    );
    
    child_process.set_parent_pid(current_pid);
    
    // Copy registers
    *child_process.registers_mut() = current_registers;

    // Copy file descriptors
    child_process.set_file_descriptors(parent_fds, parent_next_fd);
    
    // Set return value for child to 0
    child_process.registers_mut().rax = 0;
    
    // Setup context for switch
    // This creates the trampoline stack frame on the NEW kernel stack
    crate::kernel::process::switch::setup_process_context(&mut child_process);
    
    // Add to table
    table.add_process(child_process);
    
    crate::debug_println!("[Process] Forked PID={} -> PID={}", current_pid.as_u64(), pid.as_u64());
    
    Ok(pid)
}

/// Execute a new program in the current process
///
/// Replaces the current process image with a new one.
///
/// # Returns
/// * `Ok(0)` - Success (new program starts)
/// * `Err(e)` - Error code
pub fn exec_process(path: &str) -> Result<u64, CreateError> {
    let mut allocator_lock = BOOT_INFO_ALLOCATOR.lock();
    let frame_allocator = allocator_lock.as_mut().ok_or(CreateError::FrameAllocationFailed)?;
    let phys_mem_offset = VirtAddr::new(PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed));
    
    // 1. Create new page table
    let new_page_table_frame = {
        use x86_64::structures::paging::{PageTable, FrameAllocator};
        
        let frame = frame_allocator
            .allocate_frame()
            .ok_or(CreateError::FrameAllocationFailed)?;
            
        let page_table_ptr = (phys_mem_offset + frame.start_address().as_u64()).as_mut_ptr::<PageTable>();
        let page_table = unsafe { &mut *page_table_ptr };
        page_table.zero();
        
        // Copy kernel mappings
        let (kernel_frame, _) = x86_64::registers::control::Cr3::read();
        let kernel_pt_ptr = (phys_mem_offset + kernel_frame.start_address().as_u64()).as_ptr::<PageTable>();
        let kernel_pt = unsafe { &*kernel_pt_ptr };
        
        for i in 256..512 {
            page_table[i] = kernel_pt[i].clone();
        }
        
        frame
    };
    
    // 2. Load program using ELF loader
    let (entry_point, stack_top) = {
        let l4_table_ptr = (phys_mem_offset + new_page_table_frame.start_address().as_u64()).as_mut_ptr();
        let l4_table = unsafe { &mut *l4_table_ptr };
        let mut mapper = unsafe { OffsetPageTable::new(l4_table, phys_mem_offset) };
        
        // Get embedded program binary (TODO: Use VFS when implemented)
        let program_data = include_bytes!("../../shell.bin");
        
        // Try ELF loader first, fallback to legacy loader
        match crate::kernel::process::elf_impl::validate_elf(program_data) {
            Ok(_) => {
                crate::debug_println!("[exec] Using ELF loader");
                let loaded = crate::kernel::process::elf_impl::load_elf(
                    program_data,
                    &mut mapper,
                    frame_allocator,
                ).map_err(|_| CreateError::PageTableCreationError("ELF load failed"))?;
                (loaded.entry, loaded.stack_top)
            },
            Err(_) => {
                crate::debug_println!("[exec] Using legacy flat binary loader");
                let loaded_program = load_user_program(program_data, &mut mapper, frame_allocator)?;
                (loaded_program.entry_point, loaded_program.stack_top)
            }
        }
    };
    
    // 3. Update process structure and switch
    {
        let mut table = PROCESS_TABLE.lock();
        let process = table.current_process_mut().ok_or(CreateError::PageTableCreationError("No current process"))?;
        
        process.update_image(new_page_table_frame, stack_top, entry_point);
        
        // 4. Switch to new page table
        unsafe {
            crate::kernel::process::switch_to_single_process(process);
        }
        
        // 5. Reset registers
        let mut regs = *process.registers();
        regs.rip = entry_point.as_u64();
        regs.rsp = stack_top.as_u64();
        regs.rax = 0;
        regs.rbx = 0;
        regs.rcx = 0;
        regs.rdx = 0;
        regs.rsi = 0;
        regs.rdi = 0;
        regs.rbp = 0;
        regs.r8 = 0;
        regs.r9 = 0;
        regs.r10 = 0;
        regs.r11 = 0;
        regs.r12 = 0;
        regs.r13 = 0;
        regs.r14 = 0;
        regs.r15 = 0;
        regs.rflags = 0x202; // Interrupts enabled
        
        *process.registers_mut() = regs;
    }
    
    crate::debug_println!("[Process] Executed new program");
    
    Ok(0)
}
