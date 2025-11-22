//! Process management module
//!
//! This module provides process structure and lifecycle management
//! for user-mode processes.

use x86_64::structures::paging::PhysFrame;
use x86_64::VirtAddr;
use alloc::vec::Vec;
use spin::Mutex;
use lazy_static::lazy_static;

/// Process ID type
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProcessId(u64);

impl ProcessId {
    /// Create a new process ID
    pub const fn new(id: u64) -> Self {
        ProcessId(id)
    }
    
    /// Get the raw ID value
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
}

impl Process {
    /// Create a new process
    ///
    /// # Arguments
    /// * `pid` - Process ID
    /// * `page_table_frame` - Physical frame containing the process's page table
    /// * `kernel_stack` - Virtual address of the kernel stack
    /// * `user_stack` - Virtual address of the user stack
    /// * `entry_point` - Virtual address where execution should start
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
        }
    }
    
    /// Get process ID
    pub fn pid(&self) -> ProcessId {
        self.pid
    }
    
    /// Get current state
    pub fn state(&self) -> ProcessState {
        self.state
    }
    
    /// Set process state
    pub fn set_state(&mut self, state: ProcessState) {
        self.state = state;
    }
    
    /// Get page table frame
    pub fn page_table_frame(&self) -> PhysFrame {
        self.page_table_frame
    }
    
    /// Get kernel stack pointer
    pub fn kernel_stack(&self) -> VirtAddr {
        self.kernel_stack
    }
    
    /// Get user stack pointer
    pub fn user_stack(&self) -> VirtAddr {
        self.user_stack
    }
    
    /// Get saved registers
    pub fn registers(&self) -> &RegisterState {
        &self.saved_registers
    }
    
    /// Get mutable saved registers
    pub fn registers_mut(&mut self) -> &mut RegisterState {
        &mut self.saved_registers
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
    pub fn allocate_pid(&mut self) -> ProcessId {
        let pid = ProcessId::new(self.next_pid);
        self.next_pid += 1;
        pid
    }
    
    /// Get a process by ID
    pub fn get_process(&self, pid: ProcessId) -> Option<&Process> {
        self.processes.iter().find(|p| p.pid() == pid)
    }
    
    /// Get a mutable process by ID
    pub fn get_process_mut(&mut self, pid: ProcessId) -> Option<&mut Process> {
        self.processes.iter_mut().find(|p| p.pid() == pid)
    }
    
    /// Get the currently running process
    pub fn current_process(&self) -> Option<&Process> {
        self.current_pid.and_then(|pid| self.get_process(pid))
    }
    
    /// Get the currently running process (mutable)
    pub fn current_process_mut(&mut self) -> Option<&mut Process> {
        self.current_pid.and_then(|pid| self.get_process_mut(pid))
    }
    
    /// Set the current process
    pub fn set_current(&mut self, pid: ProcessId) {
        self.current_pid = Some(pid);
    }
    
    /// Get all ready processes
    pub fn ready_processes(&self) -> impl Iterator<Item = &Process> {
        self.processes.iter().filter(|p| p.state() == ProcessState::Ready)
    }
}

lazy_static! {
    /// Global process table
    pub static ref PROCESS_TABLE: Mutex<ProcessTable> = Mutex::new(ProcessTable::new());
}

/// Create a new process
///
/// This is a high-level function that allocates a PID and adds the process
/// to the global process table.
///
/// # Arguments
/// * `page_table_frame` - Physical frame containing the process's page table
/// * `kernel_stack` - Virtual address of the kernel stack
/// * `user_stack` - Virtual address of the user stack
/// * `entry_point` - Virtual address where execution should start
pub fn create_process(
    page_table_frame: PhysFrame,
    kernel_stack: VirtAddr,
    user_stack: VirtAddr,
    entry_point: VirtAddr,
) -> ProcessId {
    let mut table = PROCESS_TABLE.lock();
    let pid = table.allocate_pid();
    let process = Process::new(pid, page_table_frame, kernel_stack, user_stack, entry_point);
    table.add_process(process);
    pid
}

/// Get the current process ID
pub fn current_pid() -> Option<ProcessId> {
    PROCESS_TABLE.lock().current_pid
}
