// src/kernel/process/mod.rs
//! Process management module
//!
//! This module provides process structure and lifecycle management
//! for user-mode processes.

use x86_64::structures::paging::{PhysFrame, PageTable, FrameAllocator, Size4KiB};
use x86_64::VirtAddr;
use alloc::vec::Vec;
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::alloc::{alloc_zeroed, Layout};
use spin::Mutex;
use lazy_static::lazy_static;
use crate::kernel::fs::FileDescriptor;

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
    file_descriptors: BTreeMap<u64, Arc<Mutex<dyn FileDescriptor>>>,
    next_fd: u64,
    fpu_state: FpuState,
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
        
        let fd_keys: alloc::vec::Vec<u64> = self.file_descriptors.keys().copied().collect();
        for fd_num in fd_keys {
            if let Some(fd) = self.file_descriptors.remove(&fd_num) {
                let mut fd_lock = fd.lock();
                let _ = fd_lock.close();
                drop(fd_lock);
                crate::debug_println!("[Process] Closed FD {} for PID={}", fd_num, self.pid.as_u64());
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
            file_descriptors: BTreeMap::new(),
            next_fd: 0,
            fpu_state: FpuState::default(),
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

    pub fn add_file_descriptor(&mut self, fd: Arc<Mutex<dyn FileDescriptor>>) -> u64 {
        let id = self.next_fd;
        self.next_fd += 1;
        self.file_descriptors.insert(id, fd);
        id
    }

    pub fn get_file_descriptor(&self, fd: u64) -> Option<Arc<Mutex<dyn FileDescriptor>>> {
        self.file_descriptors.get(&fd).cloned()
    }

    pub fn remove_file_descriptor(&mut self, fd: u64) -> Option<Arc<Mutex<dyn FileDescriptor>>> {
        self.file_descriptors.remove(&fd)
    }

    pub fn clone_file_descriptors(&self) -> (BTreeMap<u64, Arc<Mutex<dyn FileDescriptor>>>, u64) {
        (self.file_descriptors.clone(), self.next_fd)
    }

    pub fn set_file_descriptors(&mut self, fds: BTreeMap<u64, Arc<Mutex<dyn FileDescriptor>>>, next_fd: u64) {
        self.file_descriptors = fds;
        self.next_fd = next_fd;
    }

    pub fn close_all_fds(&mut self) {
        let fd_keys: alloc::vec::Vec<u64> = self.file_descriptors.keys().copied().collect();
        for fd_num in fd_keys {
            if let Some(fd) = self.file_descriptors.remove(&fd_num) {
                let mut fd_lock = fd.lock();
                let _ = fd_lock.close();
            }
        }
    }

    /// Get mutable pointer to FPU state data for saving
    pub(crate) fn fpu_state_mut_ptr(&mut self) -> *mut u8 {
        self.fpu_state.data.as_mut_ptr()
    }

    /// Get const pointer to FPU state data for restoring
    pub(crate) fn fpu_state_ptr(&self) -> *const u8 {
        self.fpu_state.data.as_ptr()
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

lazy_static! {
    pub static ref PROCESS_TABLE: Mutex<ProcessTable> = Mutex::new(ProcessTable::new());
}

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
    
    crate::debug_println!("[create_user_page_table] Copying kernel entries (1-511)");
    let mut copied_entries = 0;
    for i in 1..512 {
        if !kernel_pt[i].is_unused() {
            page_table[i] = kernel_pt[i].clone();
            copied_entries += 1;
            if i == 511 || (i >= 2 && i <= 6) {
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
    
    const USER_DATA_SELECTOR: u64 = 0x23;
    const USER_CODE_SELECTOR: u64 = 0x1B;
    
    let rflags: u64 = 0x202;
    
    crate::debug_println!("[jump_to_usermode] About to switch CR3 and iretq:");
    crate::debug_println!("  RIP={:#x}, RSP={:#x}, RFLAGS={:#x}", entry_point.as_u64(), user_stack.as_u64(), rflags);
    crate::debug_println!("  USER_CODE=0x1B, USER_DATA=0x23, CR3={:#x}", user_cr3);
    
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
    
    crate::debug_println!("[DEBUG] Building iretq frame:");
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
        crate::debug_println!("[WARNING] SS is NULL! Setting to kernel data selector 0x10");
        unsafe {
            core::arch::asm!(
                "mov ax, 0x10",
                "mov ss, ax",
                options(nomem, nostack)
            );
        }
    }
    
    unsafe extern "C" {
        fn jump_to_usermode_asm(entry_point: u64, user_stack: u64, user_cr3: u64, rflags: u64) -> !;
    }
    
    crate::debug_println!("[jump_to_usermode] Using external NASM function (with DS/ES/FS/GS setup)");
    unsafe {
        jump_to_usermode_asm(
            entry_point.as_u64(),
            user_stack.as_u64(),
            user_cr3,
            rflags
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