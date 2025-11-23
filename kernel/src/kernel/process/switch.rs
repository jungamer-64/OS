//! Context switching

use crate::kernel::process::Process;
use crate::arch::x86_64::syscall::set_kernel_stack;
use x86_64::registers::control::Cr3;

/// Assembly implementation of context switch
/// 
/// Saves callee-saved registers (RBX, RBP, R12-R15) and switches the stack pointer.
/// 
/// # C ABI
/// - RDI: prev_ctx (*mut u64)
/// - RSI: next_ctx (u64)
#[unsafe(naked)]
pub unsafe extern "C" fn switch_context_asm(prev_ctx: *mut u64, next_ctx: u64) {
    core::arch::naked_asm!(
        // 1. Save callee-saved registers of the current process
        "push rbx",
        "push rbp",
        "push r12",
        "push r13",
        "push r14",
        "push r15",
        
        // 2. Save current RSP to prev_ctx (RDI)
        "mov [rdi], rsp",
        
        // 3. Load new RSP from next_ctx (RSI)
        "mov rsp, rsi",
        
        // 4. Restore callee-saved registers of the next process
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop rbp",
        "pop rbx",
        
        // 5. Return to the saved return address (popped from new stack)
        "ret",
    );
}

/// Trampoline function for new processes
/// 
/// This is the first code executed when a new process is switched to.
/// It performs the final transition to user mode.
unsafe extern "C" fn process_entry_trampoline() -> ! {
    // 1. Get current process information
    let (entry_point, user_stack, user_cr3) = {
        let table = crate::kernel::process::PROCESS_TABLE.lock();
        let process = table.current_process().expect("[Trampoline] No current process");
        (
            x86_64::VirtAddr::new(process.registers().rip),
            x86_64::VirtAddr::new(process.registers().rsp),
            process.page_table_phys_addr()
        )
    }; // Lock released here
    
    crate::debug_println!("[Trampoline] Jumping to user mode at 0x{:x}", entry_point.as_u64());
    
    // 2. Jump to user mode
    unsafe {
        crate::kernel::process::jump_to_usermode(entry_point, user_stack, user_cr3);
    }
}

/// Setup the initial context for a new process
///
/// Writes a fake stack frame to the process's kernel stack so that
/// `context_switch` can "return" to `process_entry_trampoline`.
///
/// # Arguments
/// * `process` - The process to initialize
pub fn setup_process_context(process: &mut Process) {
    let stack_top = process.kernel_stack().as_u64();
    
    // Stack layout (top to bottom, matching switch_context_asm pops):
    // [Return Address] -> process_entry_trampoline
    // [RBX]
    // [RBP]
    // [R12]
    // [R13]
    // [R14]
    // [R15]
    
    let stack_ptr = stack_top as *mut u64;
    
    unsafe {
        // We write 7 values (return addr + 6 registers)
        // Stack grows down, so index -1 is top value
        
        // 1. Return Address
        *stack_ptr.offset(-1) = process_entry_trampoline as *const () as usize as u64;
        
        // 2. Callee-saved registers (values don't matter for new process, use 0)
        *stack_ptr.offset(-2) = 0; // RBX
        *stack_ptr.offset(-3) = 0; // RBP
        *stack_ptr.offset(-4) = 0; // R12
        *stack_ptr.offset(-5) = 0; // R13
        *stack_ptr.offset(-6) = 0; // R14
        *stack_ptr.offset(-7) = 0; // R15
    }
    
    // Set context_rsp to point to the top of our fake frame
    let context_rsp = stack_top - (7 * 8);
    *process.context_rsp_mut() = context_rsp;
}

/// Switch to a different process
/// 
/// This performs a full context switch:
/// 1. Save current process state (callee-saved registers)
/// 2. Switch kernel stack
/// 3. Switch page tables
/// 4. Restore new process state
///
/// # Safety
/// This function is unsafe because it changes the address space and stack.
pub unsafe fn context_switch(from: &mut Process, to: &Process) {
    // 1. Update TSS kernel stack for syscalls/interrupts
    // This ensures that if an interrupt/syscall happens in the NEW process,
    // it uses the correct kernel stack top.
    set_kernel_stack(to.kernel_stack());
    
    // 2. Switch page table (if different)
    let (current_frame, flags) = Cr3::read();
    if current_frame != to.page_table_frame() {
        unsafe {
            Cr3::write(to.page_table_frame(), flags);
        }
    }
    
    // 3. Perform the actual register and stack switch
    let prev_ctx = from.context_rsp_mut() as *mut u64;
    let next_ctx = to.context_rsp();
    
    // Ensure the target process has a valid context
    // (Should be set up by setup_process_context for new processes)
    if next_ctx == 0 {
         panic!("Target process (PID={}) has invalid context_rsp (0). Did you call setup_process_context?", to.pid().as_u64());
    }
    
    crate::debug_println!(
        "[Context Switch] {} (RSP={:x}) -> {} (RSP={:x})",
        from.pid().as_u64(),
        unsafe { *prev_ctx }, // Dereference unsafe
        to.pid().as_u64(),
        next_ctx
    );

    unsafe {
        switch_context_asm(prev_ctx, next_ctx);
    }
}
