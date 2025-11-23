// src/kernel/process/mod.rs
//! Process management module
//!
//! This module provides process structure and lifecycle management
//! for user-mode processes.

use x86_64::structures::paging::{PhysFrame, PageTable, FrameAllocator, Size4KiB};
use x86_64::VirtAddr;
use alloc::vec::Vec;
use alloc::alloc::{alloc_zeroed, Layout};
use spin::Mutex;
use lazy_static::lazy_static;

pub mod lifecycle;
pub mod switch;

pub use lifecycle::{create_user_process, terminate_process};
pub use switch::context_switch;

/// Process ID type
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProcessId(u64);

impl ProcessId {
    /// Create a new process ID
    #[must_use]
    pub const fn new(id: u64) -> Self {
        Self(id)
    }
    
    /// Get the raw ID value
    #[must_use]
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

/// Process state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessState {
    /// Process is currently running
    Running,
    /// Process is ready抗to run
    Ready,
    /// Process is blocked (waiting for I/O, etc.)
    Blocked,
    /// Process has terminated
    Terminated,
}

/// Saved register state for context switching
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct RegisterState {
    /// General purpose registers
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rbp: u64,
    pub rsp: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    
    /// Program counter
    pub rip: u64,
    
    /// Flags register
    pub rflags: u64,
}

impl Default for RegisterState {
    fn default() -> Self {
        Self {
            rax: 0,
            rbx: 0,
            rcx: 0,
            rdx: 0,
            rsi: 0,
            rdi: 0,
            rbp: 0,
            rsp: 0,
            r8: 0,
            r9: 0,
            r10: 0,
            r11: 0,
            r12: 0,
            r13: 0,
            r14: 0,
            r15: 0,
            rip: 0,
            rflags: 0x202, // Default: IF (interrupt flag) set
        }
    }
}

/// Process control block
pub struct Process {
    /// Process ID
    pid: ProcessId,
    
    /// Current state
    state: ProcessState,
    
    /// Page table physical frame
    /// Note: We store the physical frame instead of a reference
    /// to avoid lifetime issues
    page_table_frame: PhysFrame,
    
    /// Kernel stack (for syscall handling)
    kernel_stack: VirtAddr,
    
    /// User stack  
    user_stack: VirtAddr,
    
    /// Saved CPU state for context switching
    saved_registers: RegisterState,

    /// Saved kernel stack pointer for context switching
    /// This holds the RSP when the process is switched out
    context_rsp: u64,

    /// Parent Process ID
    parent_pid: Option<ProcessId>,

    /// Exit code (if terminated)
    exit_code: Option<i32>,

    /// Top of mmap allocation (bump allocator)
    mmap_top: VirtAddr,
}

impl Drop for Process {
    fn drop(&mut self) {
        use crate::kernel::mm::allocator::BOOT_INFO_ALLOCATOR;
        // use x86_64::structures::paging::FrameDeallocator; // Trait imported but not used? Wait, deallocate_frame needs it.

        // Free page table frame
        let mut allocator_lock = BOOT_INFO_ALLOCATOR.lock();
        if let Some(frame_allocator) = allocator_lock.as_mut() {
            unsafe {
                frame_allocator.deallocate_frame(self.page_table_frame);
            }
        }
        
        // TODO: Free kernel stack
        // TODO: Free all user pages (requires walking the page table)
        
        crate::debug_println!("[Process] Dropped PID={} (Freed page table frame)", self.pid.as_u64());
    }
}

impl Process {
    /// Create a new process from a loaded program
    ///
    /// This is a convenience constructor that creates a process from
    /// a `LoadedProgram` returned by the loader.
    ///
    /// # Arguments
    /// * `pid` - Process ID
    /// * `page_table_frame` - Physical frame containing the process's page table
    /// * `kernel_stack` - Virtual address of the kernel stack
    /// * `loaded_program` - Information about the loaded program
    #[must_use]
    pub fn from_loaded_program(
        pid: ProcessId,
        page_table_frame: PhysFrame,
        kernel_stack: VirtAddr,
        loaded_program: &crate::kernel::loader::LoadedProgram,
    ) -> Self {
        Self::new(
            pid,
            page_table_frame,
            kernel_stack,
            loaded_program.stack_top,
            loaded_program.entry_point,
        )
    }
    
    /// Create a new process
    ///
    /// # Arguments
    /// * `pid` - Process ID
    /// * `page_table_frame` - Physical frame containing the process's page table
    /// * `kernel_stack` - Virtual address of the kernel stack
    /// * `user_stack` - Virtual address of the user stack
    /// * `entry_point` - Virtual address where execution should start
    #[must_use]
    #[allow(clippy::field_reassign_with_default)] // Intentional: selective initialization
    pub fn new(
        pid: ProcessId,
        page_table_frame: PhysFrame,
        kernel_stack: VirtAddr,
        user_stack: VirtAddr,
        entry_point: VirtAddr,
    ) -> Self {
        let mut registers = RegisterState::default();
        registers.rip = entry_point.as_u64();
        registers.rsp = user_stack.as_u64();
        
        Self {
            pid,
            state: ProcessState::Ready,
            page_table_frame,
            kernel_stack,
            user_stack,
            saved_registers: registers,
            context_rsp: 0, // Will be set during context switch
            parent_pid: None,
            exit_code: None,
            mmap_top: VirtAddr::new(0x0000_0010_0000_0000), // Start mmap at 64GB
        }
    }
    
    /// Get process ID
    #[must_use]
    pub const fn pid(&self) -> ProcessId {
        self.pid
    }
    
    /// Get current state
    #[must_use]
    pub const fn state(&self) -> ProcessState {
        self.state
    }
    
    /// Set process state
    pub const fn set_state(&mut self, state: ProcessState) {
        self.state = state;
    }
    
    /// Get page table frame
    #[must_use]
    pub const fn page_table_frame(&self) -> PhysFrame {
        self.page_table_frame
    }
    
    /// Get kernel stack pointer
    #[must_use]
    pub const fn kernel_stack(&self) -> VirtAddr {
        self.kernel_stack
    }
    
    /// Get user stack pointer
    #[must_use]
    pub const fn user_stack(&self) -> VirtAddr {
        self.user_stack
    }
    
    /// Get saved registers
    #[must_use]
    pub const fn registers(&self) -> &RegisterState {
        &self.saved_registers
    }
    
    /// Get mutable saved registers
    pub const fn registers_mut(&mut self) -> &mut RegisterState {
        &mut self.saved_registers
    }

    /// Get mutable reference to context RSP
    pub fn context_rsp_mut(&mut self) -> &mut u64 {
        &mut self.context_rsp
    }

    /// Get context RSP
    pub const fn context_rsp(&self) -> u64 {
        self.context_rsp
    }

    /// Update process image (for exec)
    pub fn update_image(&mut self, page_table_frame: PhysFrame, user_stack: VirtAddr, _entry_point: VirtAddr) {
        self.page_table_frame = page_table_frame;
        self.user_stack = user_stack;
        // Note: entry_point is not stored in Process, it's in registers.rip
        // But we update it here for completeness if we add it later.
        // Actually, we update registers in exec_process.
    }

    /// Get parent PID
    pub fn parent_pid(&self) -> Option<ProcessId> {
        self.parent_pid
    }

    /// Set parent PID
    pub fn set_parent_pid(&mut self, pid: ProcessId) {
        self.parent_pid = Some(pid);
    }

    /// Get exit code
    pub fn exit_code(&self) -> Option<i32> {
        self.exit_code
    }

    /// Set exit code
    pub fn set_exit_code(&mut self, code: i32) {
        self.exit_code = Some(code);
    }

    /// Get mmap top
    pub fn mmap_top(&self) -> VirtAddr {
        self.mmap_top
    }

    /// Set mmap top
    pub fn set_mmap_top(&mut self, addr: VirtAddr) {
        self.mmap_top = addr;
    }
}

/// Process table - manages all processes in the system
pub struct ProcessTable {
    processes: Vec<Process>,
    next_pid: u64,
    current_pid: Option<ProcessId>,
}

impl ProcessTable {
    /// Create a new empty process table
    #[must_use]
    #[allow(clippy::new_without_default)] // Intentional: explicit new() for clarity
    pub const fn new() -> Self {
        Self {
            processes: Vec::new(),
            next_pid: 1, // PID 0 is reserved for the kernel
            current_pid: None,
        }
    }
    
    /// Add a new process to the table
    pub fn add_process(&mut self, process: Process) -> ProcessId {
        let pid = process.pid();
        self.processes.push(process);
        pid
    }
    
    /// Allocate a new process ID
    pub const fn allocate_pid(&mut self) -> ProcessId {
        let pid = ProcessId::new(self.next_pid);
        self.next_pid += 1;
        pid
    }
    
    /// Get a process by ID
    #[must_use]
    pub fn get_process(&self, pid: ProcessId) -> Option<&Process> {
        self.processes.iter().find(|p| p.pid() == pid)
    }
    
    /// Get a mutable process by ID
    pub fn get_process_mut(&mut self, pid: ProcessId) -> Option<&mut Process> {
        self.processes.iter_mut().find(|p| p.pid() == pid)
    }
    
    /// Get the currently running process
    #[must_use]
    pub fn current_process(&self) -> Option<&Process> {
        self.current_pid.and_then(|pid| self.get_process(pid))
    }
    
    /// Get the currently running process (mutable)
    pub fn current_process_mut(&mut self) -> Option<&mut Process> {
        self.current_pid.and_then(|pid| self.get_process_mut(pid))
    }
    
    /// Set the current process
    pub const fn set_current(&mut self, pid: ProcessId) {
        self.current_pid = Some(pid);
    }
    
    /// Get all ready processes
    pub fn ready_processes(&self) -> impl Iterator<Item = &Process> {
        self.processes.iter().filter(|p| p.state() == ProcessState::Ready)
    }

    /// Find a terminated child of the given parent
    /// Returns (child_pid, exit_code) if found
    pub fn find_terminated_child(&self, parent_pid: ProcessId) -> Option<(ProcessId, i32)> {
        self.processes.iter()
            .find(|p| p.parent_pid() == Some(parent_pid) && p.state() == ProcessState::Terminated)
            .map(|p| (p.pid(), p.exit_code().unwrap_or(0)))
    }

    /// Check if a process has any children
    pub fn has_children(&self, parent_pid: ProcessId) -> bool {
        self.processes.iter().any(|p| p.parent_pid() == Some(parent_pid))
    }
    
    /// Remove a process from the table (reap)
    pub fn remove_process(&mut self, pid: ProcessId) {
        if let Some(idx) = self.processes.iter().position(|p| p.pid() == pid) {
            self.processes.remove(idx);
        }
    }
}

lazy_static! {
    /// Global process table
    pub static ref PROCESS_TABLE: Mutex<ProcessTable> = Mutex::new(ProcessTable::new());
}

/// Memory layout constants
const USER_STACK_SIZE: usize = 64 * 1024; // 64 KiB user stack
const KERNEL_STACK_SIZE: usize = 16 * 1024; // 16 KiB kernel stack per process
#[allow(dead_code)]
const USER_STACK_TOP: u64 = 0x0000_7FFF_FFFF_F000; // Top of user stack (below canonical boundary)
#[allow(dead_code)]
const KERNEL_STACK_BASE: u64 = 0xFFFF_8000_0000_0000; // Kernel stacks start here

/// Allocate a user stack for a process
///
/// Allocates `USER_STACK_SIZE` bytes and returns the top address
fn allocate_user_stack() -> VirtAddr {
    let layout = Layout::from_size_align(USER_STACK_SIZE, 16)
        .expect("Invalid stack layout");
    let ptr = unsafe { alloc_zeroed(layout) };
    assert!(!ptr.is_null(), "Failed to allocate user stack");
    // Return the top of the stack (grows downward)
    VirtAddr::new(ptr as u64 + USER_STACK_SIZE as u64)
}

/// Allocate a kernel stack for syscall handling
///
/// Allocates `KERNEL_STACK_SIZE` bytes and returns the top address
fn allocate_kernel_stack() -> VirtAddr {
    let layout = Layout::from_size_align(KERNEL_STACK_SIZE, 16)
        .expect("Invalid stack layout");
    let ptr = unsafe { alloc_zeroed(layout) };
    assert!(!ptr.is_null(), "Failed to allocate kernel stack");
    // Return the top of the stack (grows downward)
    VirtAddr::new(ptr as u64 + KERNEL_STACK_SIZE as u64)
}

/// Create a new user page table
///
/// Creates a minimal page table that maps:
/// - Kernel space (upper half: entries 256-511) copied from current page table
///   This allows kernel code/data to be accessible during syscalls
/// - User space (lower half: entries 0-255) initially empty
///   User code/data will be mapped as needed
///
/// # Phase 2 Implementation
/// - Copies kernel mappings for syscall handling
/// - Isolates user address space per process
/// - Enables per-process memory protection
///
/// # Arguments
/// * `frame_allocator` - Frame allocator for new page tables
/// * `physical_memory_offset` - Offset to access physical memory
///
/// # Returns
/// Physical frame containing the new page table, or error message
fn create_user_page_table<A>(
    frame_allocator: &mut A,
    physical_memory_offset: VirtAddr,
) -> Result<PhysFrame, &'static str>
where
    A: FrameAllocator<Size4KiB>,
{
    // Allocate a frame for the new page table
    let frame = frame_allocator
        .allocate_frame()
        .ok_or("Failed to allocate frame for page table")?;
    
    // Get a mutable reference to the page table
    let page_table_ptr = (physical_memory_offset + frame.start_address().as_u64()).as_mut_ptr::<PageTable>();
    let page_table = unsafe { &mut *page_table_ptr };
    
    // Zero out the page table (clear all entries)
    page_table.zero();
    
    // Copy kernel mappings (upper half: 0xFFFF_8000_0000_0000 and above)
    // This is required for:
    // - Syscall handler code execution
    // - Kernel stack access during syscalls
    // - Interrupt handlers (if interrupts occur in user mode)
    let kernel_pt_frame = x86_64::registers::control::Cr3::read().0;
    let kernel_pt_ptr = (physical_memory_offset + kernel_pt_frame.start_address().as_u64()).as_ptr::<PageTable>();
    let kernel_pt = unsafe { &*kernel_pt_ptr };
    
    // Copy entries 256-511 (kernel space in canonical addressing)
    // Entry 256 maps: 0xFFFF_8000_0000_0000 - 0xFFFF_807F_FFFF_FFFF (512 GiB)
    // Entry 511 maps: 0xFFFF_FF80_0000_0000 - 0xFFFF_FFFF_FFFF_FFFF (512 GiB)
    for i in 256..512 {
        page_table[i] = kernel_pt[i].clone();
    }
    
    Ok(frame)
}

/// Create a new process
///
/// This is a high-level function that allocates a PID and adds the process
/// to the global process table. It also allocates necessary resources like
/// page tables and stacks.
///
/// # Arguments
/// * `entry_point` - Virtual address where execution should start
/// * `frame_allocator` - Frame allocator for page table creation
/// * `physical_memory_offset` - Physical memory offset for page table access
///
/// # Returns
/// Process ID on success, or error message on failure
/// 
/// # Errors
/// Returns error if:
/// - Frame allocation fails for page table
/// - Stack allocation fails (panics, not error)
pub fn create_process<A>(
    entry_point: VirtAddr,
    frame_allocator: &mut A,
    physical_memory_offset: VirtAddr,
) -> Result<ProcessId, &'static str>
where
    A: FrameAllocator<Size4KiB>,
{
    let mut table = PROCESS_TABLE.lock();
    let pid = table.allocate_pid();
    
    // Create user page table
    let page_table_frame = create_user_page_table(frame_allocator, physical_memory_offset)?;
    
    // Allocate stacks
    let kernel_stack = allocate_kernel_stack();
    let user_stack = allocate_user_stack();
    
    // Create process
    let process = Process::new(pid, page_table_frame, kernel_stack, user_stack, entry_point);
    table.add_process(process);
    
    Ok(pid)
}

/// Create a new process and return the Process object
///
/// Similar to `create_process()`, but returns the process directly
/// instead of just the PID. Useful for immediate execution.
///
/// # Arguments
/// * `entry_point` - Virtual address where execution should start
/// * `frame_allocator` - Frame allocator for page table creation  
/// * `physical_memory_offset` - Physical memory offset for page table access
///
/// # Returns
/// Process object on success, or error message on failure
/// 
/// # Errors
/// Returns error if:
/// - Frame allocation fails for page table
/// - Stack allocation fails (panics, not error)
pub fn create_process_with_context<A>(
    entry_point: VirtAddr,
    frame_allocator: &mut A,
    physical_memory_offset: VirtAddr,
) -> Result<Process, &'static str>
where
    A: FrameAllocator<Size4KiB>,
{
    let mut table = PROCESS_TABLE.lock();
    let pid = table.allocate_pid();
    
    // Create user page table
    let page_table_frame = create_user_page_table(frame_allocator, physical_memory_offset)?;
    
    // Allocate stacks (16-byte aligned as verified by syscall.rs)
    let kernel_stack = allocate_kernel_stack();
    let user_stack = allocate_user_stack();
    
    // Verify stack alignment (critical for syscall mechanism)
    debug_assert!(
        kernel_stack.as_u64().is_multiple_of(16),
        "Kernel stack not 16-byte aligned: 0x{:x}",
        kernel_stack.as_u64()
    );
    debug_assert!(
        user_stack.as_u64().is_multiple_of(16),
        "User stack not 16-byte aligned: 0x{:x}",
        user_stack.as_u64()
    );
    
    // Create process
    Ok(Process::new(pid, page_table_frame, kernel_stack, user_stack, entry_point))
}

/// Switch to a process (context switch)
///
/// Updates the kernel stack pointer for syscall handling and switches
/// the page table to the process's address space.
///
/// This should be called before jumping to user mode or when switching
/// between processes.
///
/// # Arguments
/// * `process` - Process to switch to
///
/// # Safety
/// Caller must ensure:
/// - Process has valid page table
/// - Process stacks are properly initialized
/// - No outstanding references to old address space
pub unsafe fn switch_to_process(process: &Process) {
    use x86_64::registers::control::Cr3;
    
    // Update kernel stack for syscall handling
    // This is critical: syscall_entry() will load from CURRENT_KERNEL_STACK
    crate::arch::x86_64::syscall::set_kernel_stack(process.kernel_stack());
    
    // Switch page table (if different from current)
    let (current_frame, flags) = Cr3::read();
    if current_frame != process.page_table_frame() {
        unsafe {
            Cr3::write(process.page_table_frame(), flags);
        }
    }
}

/// Jump to user mode with the given process context
///
/// This function:
/// 1. Switches to the process's address space
/// 2. Sets up the kernel stack for syscall handling  
/// 3. Transitions to Ring 3 and begins execution
///
/// # Safety
/// This function is unsafe because:
/// - It directly manipulates CPU registers and privilege levels
/// - The process must have valid executable code at entry point
/// - The process must have valid stacks
/// - Interrupts must be properly configured
///
/// # Arguments
/// * `process` - Process to execute
#[allow(dead_code)]
pub unsafe fn jump_to_usermode_with_process(process: &Process) -> ! {
    // Switch to process context (page table + kernel stack)
    unsafe {
        switch_to_process(process);
    }
    
    // Mark as current process
    PROCESS_TABLE.lock().set_current(process.pid());
    
    // Jump to user mode
    let entry = VirtAddr::new(process.registers().rip);
    unsafe {
        jump_to_usermode(entry, process.user_stack())
    }
}



/// Schedule the next process and switch to it
///
/// This function:
/// 1. Picks the next process using the scheduler
/// 2. Releases the process table lock (critical for avoiding deadlocks)
/// 3. Performs the context switch
///
/// If no other process is ready, it returns immediately (if current is ready)
/// or loops/halts (if current is blocked).
pub fn schedule_next() {
    use crate::kernel::scheduler::SCHEDULER;
    
    // 1. Pick next process and prepare for switch
    let switch_info = {
        let mut table = PROCESS_TABLE.lock();
        let mut scheduler = SCHEDULER.lock();
        
        let current_pid = table.current_pid;
        
        // If current process is running, it should be in Ready state (unless it blocked itself)
        // The scheduler will pick it up if it's Ready.
        
        if let Some(next_pid) = scheduler.schedule() {
            if Some(next_pid) == current_pid {
                // Same process, no switch needed
                None
            } else {
                // Switch needed
                let current = table.current_process_mut().expect("Current process invalid");
                let current_ctx_ptr = current.context_rsp_mut() as *mut u64;
                
                let next = table.get_process(next_pid).expect("Next process invalid");
                let next_ctx_val = next.context_rsp();
                
                // Update current PID
                table.set_current(next_pid);
                
                Some((current_ctx_ptr, next_ctx_val))
            }
        } else {
            // No ready processes.
            // If current is blocked, we have a problem (deadlock/idle).
            // For now, we assume there's always an idle process or we just return.
            // But if we return and we are Blocked, we will just loop in sys_wait?
            // Ideally we should enable interrupts and halt.
            None
        }
    }; // Locks released
    
    // 2. Perform switch if needed
    if let Some((current_ctx_ptr, next_ctx_val)) = switch_info {
        unsafe {
            crate::kernel::process::switch::switch_context_asm(current_ctx_ptr, next_ctx_val);
        }
    }
}

/// Switch to user mode and jump to the specified entry point
///
/// This is a low-level function that performs the actual Ring 0 -> Ring 3
/// transition. For most use cases, use `jump_to_usermode_with_process()` instead.
///
/// # Safety
/// This function is unsafe because:
/// - It directly manipulates CPU registers and privilege levels
/// - The `entry_point` must point to valid executable code
/// - The `user_stack` must point to a valid, writable memory region
/// - Interrupts must be properly configured before calling
/// - Caller must ensure kernel stack is set via `switch_to_process()` or `set_kernel_stack()`
///
/// # Arguments
/// * `entry_point` - Virtual address of user code to execute
/// * `user_stack` - Virtual address of the top of user stack
#[allow(dead_code)]
pub unsafe fn jump_to_usermode(entry_point: VirtAddr, user_stack: VirtAddr) -> ! {
    use x86_64::registers::rflags::RFlags;
    
    // GDT selector values (must match your GDT setup)
    // Typically: USER_DATA_SELECTOR = 0x20 | 3, USER_CODE_SELECTOR = 0x18 | 3
    const USER_DATA_SELECTOR: u64 = 0x23; // Ring 3 data segment (0x20 | 3)
    const USER_CODE_SELECTOR: u64 = 0x1B; // Ring 3 code segment (0x18 | 3)
    
    // Prepare RFLAGS: enable interrupts (IF=1)
    let rflags = (RFlags::INTERRUPT_FLAG).bits();
    
    // Use sysretq instruction to return to user mode
    // Stack layout for iretq:
    // [SS, RSP, RFLAGS, CS, RIP]
    unsafe {
        core::arch::asm!(
            "cli",                    // Disable interrupts during transition
            "mov ds, {0:x}",          // Set data segments
            "mov es, {0:x}",
            "mov fs, {0:x}",
            "mov gs, {0:x}",
            
            // Push iretq frame
            "push {0}",               // SS (stack segment)
            "push {1}",               // RSP (user stack pointer)
            "push {2}",               // RFLAGS
            "push {3}",               // CS (code segment)
            "push {4}",               // RIP (entry point)
            
            "iretq",                  // Return to user mode
            
            in(reg) USER_DATA_SELECTOR,
            in(reg) user_stack.as_u64(),
            in(reg) rflags,
            in(reg) USER_CODE_SELECTOR,
            in(reg) entry_point.as_u64(),
            options(noreturn)
        )
    }
}

/// Get the current process ID
#[must_use]
pub fn current_pid() -> Option<ProcessId> {
    PROCESS_TABLE.lock().current_pid
}
