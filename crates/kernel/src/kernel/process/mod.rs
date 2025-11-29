// kernel/src/kernel/process/mod.rs
//! Process management module
//!
//! This module provides process structure and lifecycle management
//! for user-mode processes.

use x86_64::structures::paging::{PhysFrame, PageTable, FrameAllocator, Size4KiB};
use crate::debug_println;
use x86_64::VirtAddr;
use alloc::vec::Vec;
use alloc::sync::Arc;
use alloc::boxed::Box;
use alloc::alloc::{alloc_zeroed, Layout};
use spin::{Mutex, Lazy};
use crate::kernel::io_uring::IoUringContext;
use crate::kernel::capability::table::CapabilityTable;
use crate::arch::x86_64::syscall_ring::RingContext;

pub mod lifecycle;
pub mod switch;
pub mod elf_loader;
pub mod elf_impl;
pub mod binary_reader;

pub use lifecycle::{create_user_process, terminate_process};
pub use switch::switch_to_process;

/// Process ID type
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProcessId(u64);

impl ProcessId {
    #[must_use]
    pub const fn new(id: u64) -> Self {
        Self(id)
    }
    
    #[must_use]
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

/// Process state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessState {
    Running,
    Ready,
    Blocked,
    Terminated,
}

/// Saved register state for context switching
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct RegisterState {
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
    pub rip: u64,
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
            rflags: 0x202,
        }
    }
}

/// FPU/SSE/AVX state storage
#[repr(C, align(64))]
struct FpuState {
    data: [u8; 512],
}

impl Default for FpuState {
    fn default() -> Self {
        Self { data: [0; 512] }
    }
}

/// Process control block
pub struct Process {
    pid: ProcessId,
    state: ProcessState,
    page_table_frame: PhysFrame,
    kernel_stack: VirtAddr,
    user_stack: VirtAddr,
    saved_registers: RegisterState,
    context_rsp: u64,
    parent_pid: Option<ProcessId>,
    exit_code: Option<i32>,
    mmap_top: VirtAddr,
    fpu_state: FpuState,
    /// io_uring context for async I/O (optional, created on demand)
    io_uring_ctx: Option<Box<IoUringContext>>,
    /// Ring-based syscall context for async message passing (new architecture)
    ring_ctx: Option<Box<RingContext>>,
    /// Kernel virtual address of the mapped doorbell page for ring-based IO
    ring_doorbell_kern_ptr: Option<u64>,
    /// Capability table for V2 resource management (Next-gen)
    capability_table: CapabilityTable,
}

impl Drop for Process {
    fn drop(&mut self) {
        use crate::kernel::mm::allocator::BOOT_INFO_ALLOCATOR;
        use crate::kernel::mm::PHYS_MEM_OFFSET;
        use alloc::alloc::{dealloc, Layout};

        let mut allocator_lock = BOOT_INFO_ALLOCATOR.lock();
        if let Some(frame_allocator) = allocator_lock.as_mut() {
            let phys_mem_offset = VirtAddr::new(PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed));
            
            unsafe {
                crate::kernel::mm::user_paging::free_user_page_table(
                    self.page_table_frame,
                    frame_allocator,
                    phys_mem_offset
                );
            }
        }
        
        let layout = Layout::from_size_align(16 * 1024, 16).unwrap();
        unsafe {
            let stack_ptr = (self.kernel_stack.as_u64() - 16 * 1024) as *mut u8;
            dealloc(stack_ptr, layout);
        }
        
        // Clear capability table (drops all resources)
        self.capability_table.clear();
        
        // Free doorbell frame if allocated for ring-based syscall
        if let Some(kptr) = self.ring_doorbell_kern_ptr {
            let mut allocator_lock = BOOT_INFO_ALLOCATOR.lock();
            if let Some(frame_allocator) = allocator_lock.as_mut() {
                crate::kernel::io_uring::doorbell::manager().free(kptr as *const crate::kernel::io_uring::doorbell::Doorbell, frame_allocator);
            }
        }
        
        crate::debug_println!("[Process] Dropped PID={} (Freed resources)", self.pid.as_u64());
    }
}

impl Process {
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
    
    #[must_use]
    #[allow(clippy::field_reassign_with_default)]
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
            context_rsp: 0,
            parent_pid: None,
            exit_code: None,
            mmap_top: VirtAddr::new(0x0000_6000_0000_0000),
            fpu_state: FpuState::default(),
            io_uring_ctx: None,
            ring_ctx: None,
            ring_doorbell_kern_ptr: None,
            capability_table: CapabilityTable::new(),
        }
    }
    
    #[must_use]
    pub const fn pid(&self) -> ProcessId {
        self.pid
    }
    
    #[must_use]
    pub const fn state(&self) -> ProcessState {
        self.state
    }
    
    pub fn set_state(&mut self, state: ProcessState) {
        self.state = state;
    }
    
    #[must_use]
    pub const fn page_table_frame(&self) -> PhysFrame {
        self.page_table_frame
    }
    
    #[must_use]
    pub const fn kernel_stack(&self) -> VirtAddr {
        self.kernel_stack
    }
    
    #[must_use]
    pub const fn user_stack(&self) -> VirtAddr {
        self.user_stack
    }
    
    #[must_use]
    pub const fn registers(&self) -> &RegisterState {
        &self.saved_registers
    }
    
    pub fn registers_mut(&mut self) -> &mut RegisterState {
        &mut self.saved_registers
    }

    pub fn context_rsp_mut(&mut self) -> &mut u64 {
        &mut self.context_rsp
    }

    pub const fn context_rsp(&self) -> u64 {
        self.context_rsp
    }
    
    #[must_use]
    pub fn page_table_phys_addr(&self) -> u64 {
        self.page_table_frame.start_address().as_u64()
    }

    pub fn update_image(&mut self, page_table_frame: PhysFrame, user_stack: VirtAddr, _entry_point: VirtAddr) {
        self.page_table_frame = page_table_frame;
        self.user_stack = user_stack;
    }

    pub fn parent_pid(&self) -> Option<ProcessId> {
        self.parent_pid
    }

    pub fn set_parent_pid(&mut self, pid: ProcessId) {
        self.parent_pid = Some(pid);
    }

    pub fn exit_code(&self) -> Option<i32> {
        self.exit_code
    }

    pub fn set_exit_code(&mut self, code: i32) {
        self.exit_code = Some(code);
    }

    pub fn mmap_top(&self) -> VirtAddr {
        self.mmap_top
    }

    pub fn set_mmap_top(&mut self, addr: VirtAddr) {
        self.mmap_top = addr;
    }

    /// Get mutable pointer to FPU state data for saving
    pub(crate) fn fpu_state_mut_ptr(&mut self) -> *mut u8 {
        self.fpu_state.data.as_mut_ptr()
    }

    /// Get const pointer to FPU state data for restoring
    pub(crate) fn fpu_state_ptr(&self) -> *const u8 {
        self.fpu_state.data.as_ptr()
    }

    // ========================================================================
    // io_uring methods
    // ========================================================================
    
    /// Initialize or get the io_uring context
    /// 
    /// Creates a new io_uring context if one doesn't exist.
    /// Returns mutable reference to the context, or None if allocation fails.
    ///
    /// # Arguments
    /// * `allocator` - Frame allocator for page-aligned memory allocation
    pub fn io_uring_setup(
        &mut self,
        allocator: &mut crate::kernel::mm::BootInfoFrameAllocator,
    ) -> Option<&mut IoUringContext> {
        if self.io_uring_ctx.is_none() {
            let ctx = IoUringContext::new_with_allocator(allocator)?;
            self.io_uring_ctx = Some(Box::new(ctx));
            crate::debug_println!("[Process] Created io_uring context for PID={}", self.pid.as_u64());
        }
        Some(self.io_uring_ctx.as_mut().unwrap())
    }
    
    /// Get the io_uring context if it exists
    #[must_use]
    pub fn io_uring(&self) -> Option<&IoUringContext> {
        self.io_uring_ctx.as_ref().map(|b| &**b)
    }
    
    /// Get mutable io_uring context if it exists
    pub fn io_uring_mut(&mut self) -> Option<&mut IoUringContext> {
        self.io_uring_ctx.as_mut().map(|b| &mut **b)
    }
    
    /// Get mutable io_uring context and capability table reference simultaneously
    ///
    /// This is needed for io_uring enter operations which need to modify the
    /// context while also reading the capability table for I/O operations.
    pub fn io_uring_with_capabilities(&mut self) -> Option<(&mut IoUringContext, &CapabilityTable)> {
        match &mut self.io_uring_ctx {
            Some(ctx) => Some((&mut **ctx, &self.capability_table)),
            None => None,
        }
    }
    
    /// Check if io_uring is initialized
    #[must_use]
    pub fn has_io_uring(&self) -> bool {
        self.io_uring_ctx.is_some()
    }
    
    // ========================================================================
    // Ring-based syscall context methods (New Architecture)
    // ========================================================================
    
    /// Initialize the ring context for async message passing
    ///
    /// Creates a new ring context and maps it to user space.
    /// This is the foundation for the new syscall-less I/O architecture.
    ///
    /// # Arguments
    /// * `enable_sqpoll` - Enable kernel-side polling (SQPOLL mode)
    ///
    /// # Returns
    /// * `Some(user_address)` - User-space address of the RingContext
    /// * `None` - Failed to allocate or map
    pub fn ring_setup(&mut self, enable_sqpoll: bool) -> Option<&mut RingContext> {
        if self.ring_ctx.is_none() {
            let ctx = crate::arch::x86_64::syscall_ring::init_ring_for_process(enable_sqpoll)?;
            self.ring_ctx = Some(ctx);
            crate::debug_println!("[Process] Created ring context for PID={} (SQPOLL={})", 
                self.pid.as_u64(), enable_sqpoll);
        }
        Some(self.ring_ctx.as_mut().unwrap())
    }
    
    /// Initialize and map ring context to user space
    ///
    /// This is the complete setup function that:
    /// 1. Creates the RingContext
    /// 2. Maps it into the user's address space
    /// 3. Returns the user-space address
    ///
    /// # Arguments
    /// * `enable_sqpoll` - Enable kernel-side polling
    /// * `frame_allocator` - Frame allocator for page table entries
    /// * `phys_offset` - Physical memory offset
    ///
    /// # Returns
    /// * `Ok(user_address)` - User-space address where RingContext is mapped
    /// * `Err(error_code)` - Error code on failure
    pub fn ring_setup_with_mapping(
        &mut self,
        enable_sqpoll: bool,
        frame_allocator: &mut crate::kernel::mm::BootInfoFrameAllocator,
        phys_offset: x86_64::VirtAddr,
    ) -> Result<u64, i64> {
        // 1. Create ring context if not exists
        if self.ring_ctx.is_none() {
            let ctx = crate::arch::x86_64::syscall_ring::init_ring_for_process(enable_sqpoll)
                .ok_or(-12_i64)?; // ENOMEM
            self.ring_ctx = Some(ctx);
            crate::debug_println!("[Process] Created ring context for PID={} (SQPOLL={})", 
                self.pid.as_u64(), enable_sqpoll);
        }
        
        // 2. Get mapper for user page table
        let (l4_frame, _) = x86_64::registers::control::Cr3::read();
        let l4_ptr = (phys_offset + l4_frame.start_address().as_u64()).as_mut_ptr();
        let l4_table = unsafe { &mut *l4_ptr };
        let mut mapper = unsafe { 
            x86_64::structures::paging::OffsetPageTable::new(l4_table, phys_offset) 
        };
        
        // 3. Map ring context to user space
        let ctx = self.ring_ctx.as_ref().unwrap();
        let user_addr = unsafe {
            crate::arch::x86_64::syscall_ring::map_ring_to_user(
                ctx,
                &mut mapper,
                frame_allocator,
                phys_offset,
            )?
        };
        
        crate::debug_println!(
            "[Process] Ring context mapped to user space at {:#x} for PID={}",
            user_addr, self.pid.as_u64()
        );

        // -----------------------------------------------------------------
        // Allocate and map the doorbell page for this ring (zero-syscall mode)
        // -----------------------------------------------------------------
        // use crate::kernel::io_uring::doorbell::manager as doorbell_manager;
        use crate::kernel::mm::user_paging::USER_RING_CONTEXT_BASE;
        use x86_64::structures::paging::{Page, PageTableFlags, Mapper, PhysFrame, Size4KiB};
        use x86_64::VirtAddr as X64VirtAddr;
        use x86_64::PhysAddr as X64PhysAddr;

        const DOORBELL_OFFSET: u64 = 0x7000;

        // Allocate a doorbell page (backed by a physical frame) only if we
        // do not already have one. If ring context existed previously but
        // was created without SQPOLL, then this path enables SQPOLL.
        let kernel_doorbell_ptr = if let Some(kptr) = self.ring_doorbell_kern_ptr {
            kptr as *mut crate::kernel::io_uring::doorbell::Doorbell
        } else {
            match crate::kernel::io_uring::doorbell::manager().allocate(frame_allocator) {
                Some((_id, kptr)) => kptr,
                None => {
                    debug_println!("[Process] Failed to allocate doorbell frame for PID={}", self.pid.as_u64());
                    return Err(-12);
                }
            }
        };

        let user_doorbell_addr = USER_RING_CONTEXT_BASE + DOORBELL_OFFSET;

        // Map the kernel doorbell page into the user's address space
        let phys_addr = (kernel_doorbell_ptr as u64).wrapping_sub(phys_offset.as_u64());
        let phys_frame = x86_64::structures::paging::PhysFrame::containing_address(X64PhysAddr::new(phys_addr));
        let user_page = Page::<Size4KiB>::containing_address(X64VirtAddr::new(user_doorbell_addr));
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE;

        // If mapping fails, free the allocated doorbell frame and return ENOMEM
        unsafe {
            match mapper.map_to(user_page, phys_frame, flags, frame_allocator) {
                Ok(flush) => {
                    flush.flush();
                }
                Err(e) => {
                    debug_println!("[Process] Failed to map doorbell to user (PID={}, err={:?})", self.pid.as_u64(), e);
                    // Free the allocated physical frame
                    crate::kernel::io_uring::doorbell::manager().free(kernel_doorbell_ptr, frame_allocator);
                    return Err(-12);
                }
            }
        }

        // Store kernel doorbell pointer for cleanup and for SQPOLL registration
        self.ring_doorbell_kern_ptr = Some(kernel_doorbell_ptr as u64);

        // Register ring with SQPOLL if requested
        if enable_sqpoll {
            // Compute kernel sq tail address (sq_header + 4)
            let sq_header_addr = (&**ctx) as *const crate::arch::x86_64::syscall_ring::RingContext as u64;
            let sq_tail_addr = sq_header_addr + 4;
            crate::kernel::io_uring::sqpoll::register_ring(self.pid(), 0, sq_tail_addr, kernel_doorbell_ptr as u64);
            // Ensure SQPOLL background task is started
            crate::kernel::scheduler::start_sqpoll_async();
        }
        
        Ok(user_addr)
    }
    
    /// Get the user-space address of the ring context
    ///
    /// Returns the fixed user-space address where the ring context is mapped.
    /// This can be used to pass the address to user programs.
    #[must_use]
    pub fn ring_user_address(&self) -> Option<u64> {
        if self.ring_ctx.is_some() {
            Some(crate::kernel::mm::user_paging::USER_RING_CONTEXT_BASE)
        } else {
            None
        }
    }
    
    /// Get the ring context if it exists
    #[must_use]
    pub fn ring_context(&self) -> Option<&RingContext> {
        self.ring_ctx.as_ref().map(|b| &**b)
    }
    
    /// Get mutable ring context if it exists
    pub fn ring_context_mut(&mut self) -> Option<&mut RingContext> {
        self.ring_ctx.as_mut().map(|b| &mut **b)
    }
    
    /// Check if ring context is initialized
    #[must_use]
    pub fn has_ring_context(&self) -> bool {
        self.ring_ctx.is_some()
    }
    
    /// Poll the ring buffer and process pending operations
    ///
    /// Returns the number of completions generated.
    pub fn ring_poll(&mut self) -> u32 {
        self.ring_ctx.as_mut().map(|ctx| ctx.poll()).unwrap_or(0)
    }
    
    /// Check if exit was requested via ring
    pub fn ring_exit_requested(&self) -> bool {
        self.ring_ctx.as_ref().map(|ctx| ctx.exit_requested()).unwrap_or(false)
    }
    
    /// Get exit code from ring (if exit was requested)
    pub fn ring_exit_code(&self) -> Option<i32> {
        self.ring_ctx.as_ref()
            .filter(|ctx| ctx.exit_requested())
            .map(|ctx| ctx.exit_code())
    }

    /// Get kernel pointer to the ring's doorbell, if mapped
    #[must_use]
    pub fn ring_doorbell_kern_ptr(&self) -> Option<u64> {
        self.ring_doorbell_kern_ptr
    }

    // ========================================================================
    // Capability Table methods (V2 Resource Management)
    // ========================================================================

    /// Get reference to the capability table
    #[must_use]
    pub fn capability_table(&self) -> &CapabilityTable {
        &self.capability_table
    }

    /// Get mutable reference to the capability table
    pub fn capability_table_mut(&mut self) -> &mut CapabilityTable {
        &mut self.capability_table
    }

    /// Insert a capability into the process's table
    ///
    /// Returns a handle to the capability, or an error if the table is full.
    pub fn insert_capability<R: crate::kernel::capability::ResourceKind, T: core::any::Any + Send + Sync>(
        &self,
        resource: Arc<T>,
        rights: crate::kernel::capability::Rights,
    ) -> Result<crate::kernel::capability::Handle<R>, crate::abi::error::SyscallError> {
        self.capability_table.insert::<R, T>(resource, rights)
    }

    /// Get a capability entry by handle
    pub fn get_capability<R: crate::kernel::capability::ResourceKind>(
        &self,
        handle: &crate::kernel::capability::Handle<R>,
    ) -> Result<&crate::kernel::capability::table::CapabilityEntry, crate::abi::error::SyscallError> {
        self.capability_table.get(handle)
    }

    /// Get a capability with rights verification
    pub fn get_capability_with_rights<R: crate::kernel::capability::ResourceKind>(
        &self,
        handle: &crate::kernel::capability::Handle<R>,
        required: crate::kernel::capability::Rights,
    ) -> Result<&crate::kernel::capability::table::CapabilityEntry, crate::abi::error::SyscallError> {
        self.capability_table.get_with_rights(handle, required)
    }

    /// Remove a capability from the process's table
    pub fn remove_capability<R: crate::kernel::capability::ResourceKind>(
        &self,
        handle: crate::kernel::capability::Handle<R>,
    ) -> Result<alloc::boxed::Box<crate::kernel::capability::table::CapabilityEntry>, crate::abi::error::SyscallError> {
        self.capability_table.remove(handle)
    }

    /// Get the number of active capabilities
    #[must_use]
    pub fn capability_count(&self) -> u32 {
        self.capability_table.count()
    }

    /// Set the capability table (used during fork)
    ///
    /// Replaces the process's capability table with the provided one.
    /// This is used during fork to copy the parent's capabilities to the child.
    pub fn set_capability_table(&mut self, cap_table: CapabilityTable) {
        self.capability_table = cap_table;
    }

    /// Initialize standard I/O capabilities (stdin, stdout, stderr)
    ///
    /// This method registers stdin (ID=0), stdout (ID=1), and stderr (ID=2)
    /// as capabilities in the process's capability table. These are automatically
    /// registered when a new process is created.
    ///
    /// The capabilities are inserted at fixed indices with generation=0, allowing
    /// user programs to use simple integer values (0, 1, 2) as capability IDs.
    ///
    /// # Returns
    /// `Ok(())` if successful, or an error if capability registration fails.
    pub fn init_stdio_capabilities(&self) -> Result<(), crate::abi::error::SyscallError> {
        use crate::kernel::fs::{Stdin, Stdout, Stderr};
        use crate::kernel::capability::{FileResource, Rights};
        
        // stdin (ID=0) - read only
        let stdin_handle = self.capability_table.insert_at_index::<FileResource, _>(
            0,  // Fixed index for stdin
            Stdin::as_vfs_file(),
            Rights::READ,
        )?;
        crate::debug_println!(
            "[Process] Registered stdin capability: index={}, gen={}",
            stdin_handle.index(), stdin_handle.generation()
        );
        core::mem::forget(stdin_handle); // Don't drop, keep in table
        
        // stdout (ID=1) - write only
        let stdout_handle = self.capability_table.insert_at_index::<FileResource, _>(
            1,  // Fixed index for stdout
            Stdout::as_vfs_file(),
            Rights::WRITE,
        )?;
        crate::debug_println!(
            "[Process] Registered stdout capability: index={}, gen={}",
            stdout_handle.index(), stdout_handle.generation()
        );
        core::mem::forget(stdout_handle);
        
        // stderr (ID=2) - write only
        let stderr_handle = self.capability_table.insert_at_index::<FileResource, _>(
            2,  // Fixed index for stderr
            Stderr::as_vfs_file(),
            Rights::WRITE,
        )?;
        crate::debug_println!(
            "[Process] Registered stderr capability: index={}, gen={}",
            stderr_handle.index(), stderr_handle.generation()
        );
        core::mem::forget(stderr_handle);
        
        Ok(())
    }
}

/// Process table
pub struct ProcessTable {
    processes: Vec<Process>,
    next_pid: u64,
    current_pid: Option<ProcessId>,
}

impl ProcessTable {
    #[must_use]
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {
            processes: Vec::new(),
            next_pid: 1,
            current_pid: None,
        }
    }
    
    pub fn add_process(&mut self, process: Process) -> ProcessId {
        let pid = process.pid();
        self.processes.push(process);
        pid
    }
    
    pub fn allocate_pid(&mut self) -> ProcessId {
        let pid = ProcessId::new(self.next_pid);
        self.next_pid += 1;
        pid
    }
    
    #[must_use]
    pub fn get_process(&self, pid: ProcessId) -> Option<&Process> {
        self.processes.iter().find(|p| p.pid() == pid)
    }
    
    pub fn get_process_mut(&mut self, pid: ProcessId) -> Option<&mut Process> {
        self.processes.iter_mut().find(|p| p.pid() == pid)
    }
    
    #[must_use]
    pub fn current_process(&self) -> Option<&Process> {
        self.current_pid.and_then(|pid| self.get_process(pid))
    }
    
    pub fn current_process_mut(&mut self) -> Option<&mut Process> {
        self.current_pid.and_then(|pid| self.get_process_mut(pid))
    }
    
    pub fn set_current(&mut self, pid: ProcessId) {
        self.current_pid = Some(pid);
    }
    
    pub fn ready_processes(&self) -> impl Iterator<Item = &Process> {
        self.processes.iter().filter(|p| p.state() == ProcessState::Ready)
    }

    pub fn find_terminated_child(&self, parent_pid: ProcessId) -> Option<(ProcessId, i32)> {
        self.processes.iter()
            .find(|p| p.parent_pid() == Some(parent_pid) && p.state() == ProcessState::Terminated)
            .map(|p| (p.pid(), p.exit_code().unwrap_or(0)))
    }

    pub fn has_children(&self, parent_pid: ProcessId) -> bool {
        self.processes.iter().any(|p| p.parent_pid() == Some(parent_pid))
    }
    
    pub fn remove_process(&mut self, pid: ProcessId) {
        if let Some(idx) = self.processes.iter().position(|p| p.pid() == pid) {
            self.processes.remove(idx);
        }
    }
}

pub static PROCESS_TABLE: Lazy<Mutex<ProcessTable>> = Lazy::new(|| Mutex::new(ProcessTable::new()));

const USER_STACK_SIZE: usize = 64 * 1024;
const KERNEL_STACK_SIZE: usize = 16 * 1024;
#[allow(dead_code)]
const USER_STACK_TOP: u64 = 0x0000_7FFF_FFFF_F000;
#[allow(dead_code)]
const KERNEL_STACK_BASE: u64 = 0xFFFF_8000_0000_0000;

fn allocate_user_stack() -> VirtAddr {
    let layout = Layout::from_size_align(USER_STACK_SIZE, 16)
        .expect("Invalid stack layout");
    let ptr = unsafe { alloc_zeroed(layout) };
    assert!(!ptr.is_null(), "Failed to allocate user stack");
    VirtAddr::new(ptr as u64 + USER_STACK_SIZE as u64)
}

fn allocate_kernel_stack() -> VirtAddr {
    let layout = Layout::from_size_align(KERNEL_STACK_SIZE, 16)
        .expect("Invalid stack layout");
    let ptr = unsafe { alloc_zeroed(layout) };
    assert!(!ptr.is_null(), "Failed to allocate kernel stack");
    VirtAddr::new(ptr as u64 + KERNEL_STACK_SIZE as u64)
}

fn create_user_page_table<A>(
    frame_allocator: &mut A,
    physical_memory_offset: VirtAddr,
) -> Result<PhysFrame, &'static str>
where
    A: FrameAllocator<Size4KiB>,
{
    let frame = frame_allocator
        .allocate_frame()
        .ok_or("Failed to allocate frame for page table")?;
    
    let page_table_ptr = (physical_memory_offset + frame.start_address().as_u64()).as_mut_ptr::<PageTable>();
    let page_table = unsafe { &mut *page_table_ptr };
    
    page_table.zero();
    
    let kernel_pt_frame = x86_64::registers::control::Cr3::read().0;
    let kernel_pt_ptr = (physical_memory_offset + kernel_pt_frame.start_address().as_u64()).as_ptr::<PageTable>();
    let kernel_pt = unsafe { &*kernel_pt_ptr };
    
    crate::debug_println!("[create_user_page_table] Scanning ALL kernel entries (0-511)...");
    let mut count = 0;
    for i in 0..512 {
        if !kernel_pt[i].is_unused() {
            crate::debug_println!("  Kernel entry {}: addr={:#x}, flags={:?}", 
                i, kernel_pt[i].addr().as_u64(), kernel_pt[i].flags());
            count += 1;
        }
    }
    crate::debug_println!("[create_user_page_table] Found {} kernel entries", count);
    
    crate::debug_println!("[create_user_page_table] Copying kernel entries (0-511)");
    let mut copied_entries = 0;
    // Include entry 0 for kernel code during iretq transition
    for i in 0..512 {
        if !kernel_pt[i].is_unused() {
            page_table[i] = kernel_pt[i].clone();
            copied_entries += 1;
            if i <= 6 || i == 511 {
                crate::debug_println!(
                    "  [COPY] Entry {}: {:#x} -> flags: {:?}",
                    i,
                    kernel_pt[i].addr().as_u64(),
                    kernel_pt[i].flags()
                );
            }
        }
    }
    crate::debug_println!("[create_user_page_table] Copied {} entries", copied_entries);
    
    crate::debug_println!("[create_user_page_table] Copy completed, frame={:#x}", frame.start_address().as_u64());
    
    crate::debug_println!("[create_user_page_table] Verifying copied entries:");
    for i in 0..512 {
        if !page_table[i].is_unused() {
            crate::debug_println!("  User Entry {}: addr={:#x}, flags={:?}", 
                i, page_table[i].addr().as_u64(), page_table[i].flags());
        }
    }
    
    Ok(frame)
}

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
    
    let page_table_frame = create_user_page_table(frame_allocator, physical_memory_offset)?;
    
    let kernel_stack = allocate_kernel_stack();
    let user_stack = allocate_user_stack();
    
    let process = Process::new(pid, page_table_frame, kernel_stack, user_stack, entry_point);
    table.add_process(process);
    
    Ok(pid)
}

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
    
    let page_table_frame = create_user_page_table(frame_allocator, physical_memory_offset)?;
    
    crate::debug_println!("[create_process] PID={}, page_table_frame={:#x}", 
        pid.as_u64(), page_table_frame.start_address().as_u64());
    
    let kernel_stack = allocate_kernel_stack();
    let user_stack = allocate_user_stack();
    
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
    
    Ok(Process::new(pid, page_table_frame, kernel_stack, user_stack, entry_point))
}

#[allow(dead_code)]
pub unsafe fn switch_to_single_process(process: &Process) {
    use x86_64::registers::control::{Cr3, Cr3Flags};
    let flags = Cr3Flags::empty();
    unsafe {
        Cr3::write(process.page_table_frame(), flags);
    }
}

#[allow(dead_code)]
pub unsafe fn jump_to_usermode_with_process(process: &Process) -> ! {
    unsafe {
        switch_to_single_process(process);
    }
    
    PROCESS_TABLE.lock().set_current(process.pid());
    
    let entry = VirtAddr::new(process.registers().rip);
    let user_cr3 = process.page_table_phys_addr();
    unsafe {
        jump_to_usermode(entry, process.user_stack(), user_cr3)
    }
}

pub fn schedule_next() {
    use crate::kernel::scheduler::SCHEDULER;
    
    let switch_info = {
        let mut table = PROCESS_TABLE.lock();
        let mut scheduler = SCHEDULER.lock();
        
        let current_pid = table.current_pid;
        
        if let Some(next_pid) = scheduler.schedule() {
            if Some(next_pid) == current_pid {
                None
            } else {
                let current = table.current_process_mut().expect("Current process invalid");
                let current_ctx_ptr = current.context_rsp_mut() as *mut u64;
                
                let next = table.get_process(next_pid).expect("Next process invalid");
                let next_ctx_val = next.context_rsp();
                
                table.set_current(next_pid);
                
                Some((current_ctx_ptr, next_ctx_val))
            }
        } else {
            None
        }
    };
    
    if let Some((current_ctx_ptr, next_ctx_val)) = switch_info {
        unsafe {
            crate::kernel::process::switch::switch_context_asm(current_ctx_ptr, next_ctx_val);
        }
    }
}

#[allow(dead_code)]
pub unsafe fn jump_to_usermode(entry_point: VirtAddr, user_stack: VirtAddr, user_cr3: u64) -> ! {
    if user_cr3 == 0 {
        crate::debug_println!("[ERROR] user_cr3 is NULL! Cannot switch page tables.");
        loop { x86_64::instructions::hlt(); }
    }
    if user_cr3 & 0xFFF != 0 {
        crate::debug_println!("[ERROR] user_cr3 not page-aligned: {:#x}", user_cr3);
        loop { x86_64::instructions::hlt(); }
    }
    
    // New SYSRET-compatible GDT layout:
    //   0x08: kernel_code
    //   0x10: user_data (with RPL=3 -> 0x13)
    //   0x18: user_code (with RPL=3 -> 0x1B)
    //   0x20: kernel_data
    const USER_DATA_SELECTOR: u64 = 0x13;
    const USER_CODE_SELECTOR: u64 = 0x1B;
    
    let rflags: u64 = 0x202;
    
    crate::debug_println!("[jump_to_usermode] About to use IRETQ:");
    crate::debug_println!("  RIP={:#x}, RSP={:#x}, RFLAGS={:#x}", entry_point.as_u64(), user_stack.as_u64(), rflags);
    crate::debug_println!("  USER_CODE=0x1B, USER_DATA=0x13, CR3={:#x}", user_cr3);
    
    use x86_64::instructions::tables::sgdt;
    let gdtr = sgdt();
    crate::debug_println!("[DEBUG] GDT base: {:#x}, limit: {:#x}", 
        gdtr.base.as_u64(), gdtr.limit);
    
    let current_rsp: u64;
    unsafe {
        core::arch::asm!(
            "mov {}, rsp",
            out(reg) current_rsp,
        );
    }
    crate::debug_println!("[DEBUG] Current kernel RSP: {:#x}", current_rsp);
    
    crate::debug_println!("[DEBUG] Building IRETQ frame:");
    crate::debug_println!("[DEBUG]   entry_point (RIP): {:#x}", entry_point.as_u64());
    crate::debug_println!("[DEBUG]   user_stack (RSP): {:#x}", user_stack.as_u64());
    crate::debug_println!("[DEBUG]   user_cr3: {:#x}", user_cr3);
    crate::debug_println!("[DEBUG]   rflags: {:#x}", rflags);
    
    let current_ss: u16;
    let current_ds: u16;
    let current_es: u16;
    unsafe {
        core::arch::asm!(
            "mov {0:x}, ss",
            "mov {1:x}, ds", 
            "mov {2:x}, es",
            out(reg) current_ss,
            out(reg) current_ds,
            out(reg) current_es,
        );
    }
    crate::debug_println!("[DEBUG] Current SS: {:#x}, DS: {:#x}, ES: {:#x}", current_ss, current_ds, current_es);
    
    if current_ss == 0 {
        crate::debug_println!("[WARNING] SS is NULL! Setting to kernel data selector 0x20");
        unsafe {
            core::arch::asm!(
                "mov ax, 0x20",
                "mov ss, ax",
                options(nomem, nostack)
            );
        }
    }
    
    unsafe extern "C" {
        fn jump_to_usermode_asm(entry_point: u64, user_stack: u64, user_cr3: u64, rflags: u64, ring_context_addr: u64) -> !;
    }
    
    crate::debug_println!("[jump_to_usermode] Using external NASM function with IRETQ");
    unsafe {
        jump_to_usermode_asm(
            entry_point.as_u64(),
            user_stack.as_u64(),
            user_cr3,
            rflags,
            0  // ring_context_addr = 0 (not using ring mode in this call)
        )
    }
}

/// Jump to user mode with Ring context address
///
/// This variant passes the RingContext address to the user program in RDI.
#[allow(dead_code)]
pub unsafe fn jump_to_usermode_with_ring(
    entry_point: VirtAddr,
    user_stack: VirtAddr,
    user_cr3: u64,
    ring_context_addr: u64,
) -> ! {
    if user_cr3 == 0 {
        crate::debug_println!("[ERROR] user_cr3 is NULL! Cannot switch page tables.");
        loop { x86_64::instructions::hlt(); }
    }
    
    let rflags: u64 = 0x202;
    
    crate::debug_println!("[jump_to_usermode_with_ring] Jumping to user mode:");
    crate::debug_println!("  RIP={:#x}, RSP={:#x}", entry_point.as_u64(), user_stack.as_u64());
    crate::debug_println!("  CR3={:#x}, ring_ctx={:#x}", user_cr3, ring_context_addr);
    
    unsafe extern "C" {
        fn jump_to_usermode_asm(entry_point: u64, user_stack: u64, user_cr3: u64, rflags: u64, ring_context_addr: u64) -> !;
    }
    
    unsafe {
        jump_to_usermode_asm(
            entry_point.as_u64(),
            user_stack.as_u64(),
            user_cr3,
            rflags,
            ring_context_addr,
        )
    }
}

#[must_use]
pub fn current_pid() -> Option<ProcessId> {
    PROCESS_TABLE.lock().current_pid
}

pub unsafe fn jump_to_usermode_wrapper(entry: u64, stack: u64, cr3: u64) -> ! {
    let entry_addr = VirtAddr::new(entry);
    let stack_addr = VirtAddr::new(stack);
    unsafe {
        jump_to_usermode(entry_addr, stack_addr, cr3)
    }
}