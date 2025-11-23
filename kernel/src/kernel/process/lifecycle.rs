// kernel/src/kernel/process/lifecycle.rs
//! Process lifecycle management

use crate::kernel::process::{ProcessId, ProcessState, PROCESS_TABLE};
use crate::kernel::loader::load_user_program;
use crate::kernel::mm::allocator::BOOT_INFO_ALLOCATOR;
use crate::kernel::mm::PHYS_MEM_OFFSET;
use x86_64::VirtAddr;
use x86_64::structures::paging::{OffsetPageTable, PageTable};


/// Error types for process creation
#[derive(Debug)]
pub enum CreateError {
    /// Frame allocation failed
    FrameAllocationFailed,
    /// Loader error occurred
    LoaderError(crate::kernel::loader::LoadError),
    /// Page table creation error
    PageTableCreationError(&'static str),
}

impl From<crate::kernel::loader::LoadError> for CreateError {
    fn from(e: crate::kernel::loader::LoadError) -> Self {
        CreateError::LoaderError(e)
    }
}

/// Create a new user process
/// 
/// This is the main entry point for creating processes in Phase 2.
/// It creates a new process, loads the embedded user program, and adds it to the process table.
pub fn create_user_process() -> Result<(ProcessId, VirtAddr, VirtAddr, u64), CreateError> {
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

        // 修正: Mapper を作成する前にデバッグ出力を行う
        // これで借用エラー (E0502) を回避
        crate::debug_println!("[create_user_process] PML4 Entry 0 before load: {:?}", l4_table[0]);

        let mut mapper = unsafe { OffsetPageTable::new(l4_table, phys_mem_offset) };
        
        let loaded_program = load_user_program(&mut mapper, frame_allocator)?;
        
        // DEBUG: After loading user program
        // Note: l4_table access here technically might be unsafe if mapper is still alive, 
        // but since mapper is not used after this point, the compiler might allow it due to NLL (Non-Lexical Lifetimes).
        // If it still complains, drop(mapper) explicitly before this print.
        // For safety, let's assume mapper is done.
        
        crate::debug_println!("[create_user_process] PML4 Entry 0 after load: {:?}", l4_table[0]);
        // ... (以下のデバッグ出力はそのまま)
        
        // Update process entry point and stack
        process.registers_mut().rip = loaded_program.entry_point.as_u64();
        process.registers_mut().rsp = loaded_program.stack_top.as_u64();
    }
    
    // Setup initial kernel stack context for switching
    crate::kernel::process::switch::setup_process_context(&mut process);
    
    process.set_state(ProcessState::Ready);
    
    // Extract info before moving process
    let entry_point = VirtAddr::new(process.registers().rip);
    let user_stack = VirtAddr::new(process.registers().rsp);
    let user_cr3 = process.page_table_phys_addr();
    
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
        
        // Wake up parent if it's blocked
        if let Some(ppid) = parent_pid {
            if let Some(parent) = table.get_process_mut(ppid) {
                if parent.state() == ProcessState::Blocked {
                    parent.set_state(ProcessState::Ready);
                }
            }
        }
        
        // TODO: Free process resources (page table, stacks, etc.)
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
/// Note: Currently only reloads the embedded user program.
///
/// # Returns
/// * `Ok(0)` - Success (new program starts)
/// * `Err(e)` - Error code
pub fn exec_process() -> Result<u64, CreateError> {
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
    
    // 2. Load program into new page table
    let (entry_point, stack_top) = {
        let l4_table_ptr = (phys_mem_offset + new_page_table_frame.start_address().as_u64()).as_mut_ptr();
        let l4_table = unsafe { &mut *l4_table_ptr };
        let mut mapper = unsafe { OffsetPageTable::new(l4_table, phys_mem_offset) };
        
        let loaded_program = load_user_program(&mut mapper, frame_allocator)?;
        (loaded_program.entry_point, loaded_program.stack_top)
    };
    
    // 3. Update process structure and switch
    {
        let mut table = PROCESS_TABLE.lock();
        let process = table.current_process_mut().ok_or(CreateError::PageTableCreationError("No current process"))?;
        
        process.update_image(new_page_table_frame, stack_top, entry_point);
        
        // 4. Switch to new page table
        unsafe {
            crate::kernel::process::switch_to_process(process);
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
