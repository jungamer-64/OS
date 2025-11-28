// crates/kernel/src/arch/x86_64/syscall_ring.rs
//! Ring Buffer Based Syscall System
//!
//! This module implements a revolutionary syscall mechanism that completely
//! replaces the traditional System V ABI approach with io_uring-style
//! asynchronous message passing.
//!
//! # Architecture Philosophy
//!
//! Traditional syscalls are "function calls" - arguments in registers, blocking
//! wait, return value in RAX. This module treats syscalls as "doorbells" -
//! a simple notification that data is ready in a shared memory ring buffer.
//!
//! ## Key Innovations
//!
//! 1. **Doorbell-Only Syscall**: No register arguments needed
//! 2. **Registered Buffers**: Pre-validated memory regions eliminate per-call checks
//! 3. **Handle-Based I/O**: Buffer ID + offset instead of raw pointers
//! 4. **SQPOLL Mode**: Kernel polling eliminates syscalls entirely
//!
//! ## Performance Benefits
//!
//! - Syscall entry/exit reduced from ~50 instructions to ~15
//! - No register shuffling for System V ABI
//! - No per-call pointer validation
//! - Batched operations amortize overhead

#![allow(dead_code)]
#![allow(unsafe_op_in_unsafe_fn)]

use core::sync::atomic::{AtomicU32, AtomicU64, AtomicBool, AtomicUsize, Ordering};
use alloc::vec::Vec;
use alloc::boxed::Box;
use spin::Mutex;
use x86_64::VirtAddr;
use crate::debug_println;

// =============================================================================
// Constants
// =============================================================================

/// Ring buffer size (must be power of 2)
pub const RING_SIZE: usize = 256;
pub const RING_MASK: u32 = (RING_SIZE - 1) as u32;

/// Maximum number of registered buffers per process
pub const MAX_REGISTERED_BUFFERS: usize = 64;

/// Cache line size for alignment
pub const CACHE_LINE_SIZE: usize = 64;

/// Syscall number for ring-based operations
pub const SYSCALL_RING_ENTER: u64 = 2000;
/// Syscall number for buffer registration
pub const SYSCALL_RING_REGISTER: u64 = 2001;
/// Syscall number for ring setup
pub const SYSCALL_RING_SETUP: u64 = 2002;

// =============================================================================
// Operation Codes (Opcodes)
// =============================================================================

/// Operation codes for the ring buffer
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RingOpcode {
    /// No operation
    Nop = 0,
    /// Write to file descriptor (uses registered buffer)
    Write = 1,
    /// Read from file descriptor (uses registered buffer)
    Read = 2,
    /// Get current timestamp
    GetTime = 3,
    /// Get process ID
    GetPid = 4,
    /// Yield CPU
    Yield = 5,
    /// Memory fence
    Fence = 6,
    /// Exit process
    Exit = 7,
    /// Register a buffer
    RegisterBuffer = 8,
    /// Unregister a buffer
    UnregisterBuffer = 9,
    /// Allocate memory
    Alloc = 10,
    /// Deallocate memory
    Dealloc = 11,
    /// Fork process
    Fork = 12,
    /// Execute program
    Exec = 13,
    /// Wait for child
    Wait = 14,
    /// Console write (direct string)
    ConsoleWrite = 15,
    /// Console read
    ConsoleRead = 16,
    /// Open file
    Open = 17,
    /// Close file descriptor
    Close = 18,
    /// Seek in file
    Seek = 19,
    /// Get file status
    Stat = 20,
    /// Reserved for custom operations
    Custom = 255,
}

impl From<u8> for RingOpcode {
    fn from(val: u8) -> Self {
        match val {
            0 => RingOpcode::Nop,
            1 => RingOpcode::Write,
            2 => RingOpcode::Read,
            3 => RingOpcode::GetTime,
            4 => RingOpcode::GetPid,
            5 => RingOpcode::Yield,
            6 => RingOpcode::Fence,
            7 => RingOpcode::Exit,
            8 => RingOpcode::RegisterBuffer,
            9 => RingOpcode::UnregisterBuffer,
            10 => RingOpcode::Alloc,
            11 => RingOpcode::Dealloc,
            12 => RingOpcode::Fork,
            13 => RingOpcode::Exec,
            14 => RingOpcode::Wait,
            15 => RingOpcode::ConsoleWrite,
            16 => RingOpcode::ConsoleRead,
            17 => RingOpcode::Open,
            18 => RingOpcode::Close,
            19 => RingOpcode::Seek,
            20 => RingOpcode::Stat,
            _ => RingOpcode::Custom,
        }
    }
}

// =============================================================================
// Submission Queue Entry (SQE) - Ideal Handle-Based Design
// =============================================================================

/// Ideal Submission Queue Entry
///
/// This structure completely eliminates raw pointers in favor of
/// pre-registered buffer handles. Each entry is cache-line aligned.
///
/// # Design Philosophy
///
/// Instead of:
/// ```text
/// write(fd, pointer, length)  // Requires pointer validation every call
/// ```
///
/// We use:
/// ```text
/// write(fd, buf_index, buf_offset, length)  // Pre-validated buffer handle
/// ```
#[repr(C, align(64))]
#[derive(Debug, Clone, Copy)]
pub struct IdealSqe {
    /// Operation code
    pub opcode: u8,
    /// Operation flags
    pub flags: u8,
    /// I/O priority
    pub ioprio: u16,
    /// File descriptor
    pub fd: u32,
    
    // ---- Handle-based addressing (no raw pointers!) ----
    
    /// Registered buffer index (instead of pointer)
    pub buf_index: u16,
    /// Offset within the registered buffer
    pub buf_offset: u32,
    /// Operation length
    pub len: u32,
    
    // ---- Additional arguments ----
    
    /// Generic argument 1 (operation-specific)
    pub arg1: u64,
    /// Generic argument 2 (operation-specific)
    pub arg2: u64,
    
    // ---- Identification ----
    
    /// User data (passed back in completion)
    pub user_data: u64,
    
    /// Padding to cache line
    _padding: [u8; 14],
}

impl IdealSqe {
    /// Create an empty SQE
    pub const fn empty() -> Self {
        Self {
            opcode: 0,
            flags: 0,
            ioprio: 0,
            fd: 0,
            buf_index: 0,
            buf_offset: 0,
            len: 0,
            arg1: 0,
            arg2: 0,
            user_data: 0,
            _padding: [0; 14],
        }
    }
    
    /// Create a write operation
    pub const fn write(fd: u32, buf_index: u16, buf_offset: u32, len: u32, user_data: u64) -> Self {
        Self {
            opcode: RingOpcode::Write as u8,
            flags: 0,
            ioprio: 0,
            fd,
            buf_index,
            buf_offset,
            len,
            arg1: 0,
            arg2: 0,
            user_data,
            _padding: [0; 14],
        }
    }
    
    /// Create a read operation
    pub const fn read(fd: u32, buf_index: u16, buf_offset: u32, len: u32, user_data: u64) -> Self {
        Self {
            opcode: RingOpcode::Read as u8,
            flags: 0,
            ioprio: 0,
            fd,
            buf_index,
            buf_offset,
            len,
            arg1: 0,
            arg2: 0,
            user_data,
            _padding: [0; 14],
        }
    }
    
    /// Create a getpid operation
    pub const fn getpid(user_data: u64) -> Self {
        Self {
            opcode: RingOpcode::GetPid as u8,
            flags: 0,
            ioprio: 0,
            fd: 0,
            buf_index: 0,
            buf_offset: 0,
            len: 0,
            arg1: 0,
            arg2: 0,
            user_data,
            _padding: [0; 14],
        }
    }
    
    /// Create an exit operation
    pub const fn exit(exit_code: i32, user_data: u64) -> Self {
        Self {
            opcode: RingOpcode::Exit as u8,
            flags: 0,
            ioprio: 0,
            fd: 0,
            buf_index: 0,
            buf_offset: 0,
            len: 0,
            arg1: exit_code as u64,
            arg2: 0,
            user_data,
            _padding: [0; 14],
        }
    }
}

// Verify size at compile time
const _: () = assert!(core::mem::size_of::<IdealSqe>() == 64);

// =============================================================================
// Completion Queue Entry (CQE)
// =============================================================================

/// Completion Queue Entry
///
/// Minimal structure for returning results to userspace.
#[repr(C, align(16))]
#[derive(Debug)]
pub struct IdealCqe {
    /// User data from submission (for correlation)
    pub user_data: u64,
    /// Result value (positive = success/bytes, negative = -errno)
    pub result: i64,
}

impl IdealCqe {
    /// Create an empty CQE
    pub const fn empty() -> Self {
        Self {
            user_data: 0,
            result: 0,
        }
    }
    
    /// Create a success CQE
    pub const fn success(user_data: u64, value: i64) -> Self {
        Self { user_data, result: value }
    }
    
    /// Create an error CQE
    pub const fn error(user_data: u64, errno: i64) -> Self {
        Self { user_data, result: errno }
    }
}

// Verify size at compile time
const _: () = assert!(core::mem::size_of::<IdealCqe>() == 16);

// =============================================================================
// Registered Buffer Management
// =============================================================================

/// A registered buffer that has been pre-validated and pinned
#[derive(Debug)]
pub struct RegisteredBuffer {
    /// Virtual address (in user space)
    pub user_addr: u64,
    /// Physical address (for DMA if needed)
    pub phys_addr: u64,
    /// Kernel-mapped virtual address
    pub kernel_addr: u64,
    /// Buffer size
    pub size: u64,
    /// Is this slot in use?
    pub in_use: bool,
    /// Permissions (read/write)
    pub permissions: BufferPermissions,
}

/// Buffer access permissions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BufferPermissions {
    pub read: bool,
    pub write: bool,
}

impl RegisteredBuffer {
    /// Create an empty (unused) buffer slot
    pub const fn empty() -> Self {
        Self {
            user_addr: 0,
            phys_addr: 0,
            kernel_addr: 0,
            size: 0,
            in_use: false,
            permissions: BufferPermissions { read: false, write: false },
        }
    }
    
    /// Validate that an access is within bounds
    #[inline(always)]
    pub fn validate_access(&self, offset: u64, len: u64) -> Result<(), i64> {
        if !self.in_use {
            return Err(-9); // EBADF
        }
        
        let end = offset.checked_add(len).ok_or(-14_i64)?; // EFAULT
        if end > self.size {
            return Err(-14); // EFAULT
        }
        
        Ok(())
    }
    
    /// Get a kernel pointer for the given offset
    #[inline(always)]
    pub unsafe fn get_ptr(&self, offset: u64) -> *const u8 {
        (self.kernel_addr + offset) as *const u8
    }
    
    /// Get a mutable kernel pointer for the given offset
    #[inline(always)]
    pub unsafe fn get_ptr_mut(&self, offset: u64) -> *mut u8 {
        (self.kernel_addr + offset) as *mut u8
    }
}

/// Buffer registry for a process
#[derive(Debug)]
pub struct BufferRegistry {
    buffers: [RegisteredBuffer; MAX_REGISTERED_BUFFERS],
    next_free: usize,
}

impl BufferRegistry {
    /// Create a new empty buffer registry
    pub const fn new() -> Self {
        const EMPTY: RegisteredBuffer = RegisteredBuffer::empty();
        Self {
            buffers: [EMPTY; MAX_REGISTERED_BUFFERS],
            next_free: 0,
        }
    }
    
    /// Register a new buffer
    ///
    /// Returns the buffer index on success.
    pub fn register(
        &mut self,
        user_addr: u64,
        size: u64,
        permissions: BufferPermissions,
    ) -> Result<u16, i64> {
        // Find a free slot
        for i in 0..MAX_REGISTERED_BUFFERS {
            let idx = (self.next_free + i) % MAX_REGISTERED_BUFFERS;
            if !self.buffers[idx].in_use {
                // Validate and pin the buffer
                // In a real implementation, this would:
                // 1. Walk page tables to verify mapping
                // 2. Pin pages to prevent swapping
                // 3. Map into kernel space
                
                // For now, we do simplified validation
                if !super::syscall::validation::is_user_range(user_addr, size) {
                    return Err(-14); // EFAULT
                }
                
                self.buffers[idx] = RegisteredBuffer {
                    user_addr,
                    phys_addr: 0, // Would be populated by page table walk
                    kernel_addr: user_addr, // Identity-mapped for now
                    size,
                    in_use: true,
                    permissions,
                };
                
                self.next_free = (idx + 1) % MAX_REGISTERED_BUFFERS;
                return Ok(idx as u16);
            }
        }
        
        Err(-28) // ENOSPC - no free buffer slots
    }
    
    /// Unregister a buffer
    pub fn unregister(&mut self, index: u16) -> Result<(), i64> {
        let idx = index as usize;
        if idx >= MAX_REGISTERED_BUFFERS {
            return Err(-22); // EINVAL
        }
        
        if !self.buffers[idx].in_use {
            return Err(-9); // EBADF
        }
        
        // Unpin pages and unmap from kernel
        self.buffers[idx] = RegisteredBuffer::empty();
        Ok(())
    }
    
    /// Get a buffer by index
    #[inline(always)]
    pub fn get(&self, index: u16) -> Option<&RegisteredBuffer> {
        let idx = index as usize;
        if idx < MAX_REGISTERED_BUFFERS && self.buffers[idx].in_use {
            Some(&self.buffers[idx])
        } else {
            None
        }
    }
}

// =============================================================================
// Ring Header (Shared between User and Kernel)
// =============================================================================

/// Ring buffer header for lock-free producer/consumer
#[repr(C, align(64))]
pub struct RingHeader {
    /// Head pointer (consumer index)
    pub head: AtomicU32,
    /// Tail pointer (producer index)
    pub tail: AtomicU32,
    /// Ring mask (size - 1)
    pub ring_mask: u32,
    /// Flags
    pub flags: AtomicU32,
    /// Padding to cache line
    _padding: [u32; 12],
}

impl RingHeader {
    /// Create a new ring header
    pub const fn new() -> Self {
        Self {
            head: AtomicU32::new(0),
            tail: AtomicU32::new(0),
            ring_mask: RING_MASK,
            flags: AtomicU32::new(0),
            _padding: [0; 12],
        }
    }
    
    /// Check if ring has entries available
    #[inline(always)]
    pub fn has_entries(&self) -> bool {
        self.head.load(Ordering::Acquire) != self.tail.load(Ordering::Acquire)
    }
    
    /// Get number of available entries
    #[inline(always)]
    pub fn available(&self) -> u32 {
        let head = self.head.load(Ordering::Acquire);
        let tail = self.tail.load(Ordering::Acquire);
        tail.wrapping_sub(head)
    }
}

/// Ring header flags
pub mod ring_flags {
    /// Kernel polling is active (SQPOLL mode)
    pub const SQPOLL: u32 = 1 << 0;
    /// I/O completion polling is active
    pub const IOPOLL: u32 = 1 << 1;
    /// Ring is being processed
    pub const BUSY: u32 = 1 << 2;
    /// Kernel poller needs wakeup
    pub const NEED_WAKEUP: u32 = 1 << 3;
}

// =============================================================================
// Ring Context (Per-Process)
// =============================================================================

/// Ring-based I/O context for a process
///
/// IMPORTANT: This struct must be page-aligned (4096 bytes) so that when
/// mapped to user space, the user-space address correctly points to the
/// start of the structure.
#[repr(C, align(4096))]
pub struct RingContext {
    /// Submission queue header
    sq_header: RingHeader,
    /// Completion queue header
    cq_header: RingHeader,
    /// Submission queue entries
    sq_entries: [IdealSqe; RING_SIZE],
    /// Completion queue entries
    cq_entries: [IdealCqe; RING_SIZE],
    /// Registered buffers
    buffers: BufferRegistry,
    /// SQPOLL enabled flag
    sqpoll_enabled: AtomicBool,
    /// Operations processed counter
    ops_processed: AtomicU64,
    /// Exit requested (from Exit operation)
    exit_requested: AtomicBool,
    /// Exit code
    exit_code: AtomicU64,
}

impl RingContext {
    /// Create a new ring context
    pub const fn new() -> Self {
        const EMPTY_SQE: IdealSqe = IdealSqe::empty();
        const EMPTY_CQE: IdealCqe = IdealCqe::empty();
        
        Self {
            sq_header: RingHeader::new(),
            cq_header: RingHeader::new(),
            sq_entries: [EMPTY_SQE; RING_SIZE],
            cq_entries: [EMPTY_CQE; RING_SIZE],
            buffers: BufferRegistry::new(),
            sqpoll_enabled: AtomicBool::new(false),
            ops_processed: AtomicU64::new(0),
            exit_requested: AtomicBool::new(false),
            exit_code: AtomicU64::new(0),
        }
    }
    
    /// Enable SQPOLL mode
    pub fn enable_sqpoll(&self) {
        self.sqpoll_enabled.store(true, Ordering::Release);
        self.sq_header.flags.fetch_or(ring_flags::SQPOLL, Ordering::Release);
    }
    
    /// Check if exit was requested
    pub fn exit_requested(&self) -> bool {
        self.exit_requested.load(Ordering::Acquire)
    }
    
    /// Get exit code
    pub fn exit_code(&self) -> i32 {
        self.exit_code.load(Ordering::Acquire) as i32
    }
    
    /// Poll and process pending submissions
    ///
    /// Returns the number of completions generated.
    pub fn poll(&mut self) -> u32 {
        let mut completed = 0u32;
        
        loop {
            let sq_head = self.sq_header.head.load(Ordering::Acquire);
            let sq_tail = self.sq_header.tail.load(Ordering::Acquire);
            
            if sq_head == sq_tail {
                break;
            }
            
            // Get entry at head and copy the data we need
            let idx = (sq_head & RING_MASK) as usize;
            let sqe_copy = self.sq_entries[idx];  // Copy the entire entry
            
            // Process the entry (now we own a copy)
            let result = self.process_sqe(&sqe_copy);
            
            // Write completion
            let cq_tail = self.cq_header.tail.load(Ordering::Acquire);
            let cq_idx = (cq_tail & RING_MASK) as usize;
            self.cq_entries[cq_idx] = IdealCqe::success(sqe_copy.user_data, result);
            
            // Advance CQ tail
            self.cq_header.tail.fetch_add(1, Ordering::Release);
            
            // Advance SQ head
            self.sq_header.head.fetch_add(1, Ordering::Release);
            
            completed += 1;
            self.ops_processed.fetch_add(1, Ordering::Relaxed);
        }
        
        completed
    }
    
    /// Process a single SQE
    fn process_sqe(&mut self, sqe: &IdealSqe) -> i64 {
        let opcode = RingOpcode::from(sqe.opcode);
        
        match opcode {
            RingOpcode::Nop => 0,
            
            RingOpcode::Write => {
                self.do_write(sqe.fd, sqe.buf_index, sqe.buf_offset, sqe.len)
            }
            
            RingOpcode::Read => {
                self.do_read(sqe.fd, sqe.buf_index, sqe.buf_offset, sqe.len)
            }
            
            RingOpcode::GetTime => {
                crate::arch::x86_64::cpu::read_timestamp() as i64
            }
            
            RingOpcode::GetPid => {
                use crate::kernel::process::PROCESS_TABLE;
                let table = PROCESS_TABLE.lock();
                table.current_process()
                    .map(|p| p.pid().as_u64() as i64)
                    .unwrap_or(-3) // ESRCH
            }
            
            RingOpcode::Yield => 0,
            
            RingOpcode::Fence => {
                core::sync::atomic::fence(Ordering::SeqCst);
                0
            }
            
            RingOpcode::Exit => {
                self.exit_requested.store(true, Ordering::Release);
                self.exit_code.store(sqe.arg1, Ordering::Release);
                sqe.arg1 as i64
            }
            
            RingOpcode::RegisterBuffer => {
                let permissions = BufferPermissions {
                    read: (sqe.flags & 1) != 0,
                    write: (sqe.flags & 2) != 0,
                };
                match self.buffers.register(sqe.arg1, sqe.len as u64, permissions) {
                    Ok(idx) => idx as i64,
                    Err(e) => e,
                }
            }
            
            RingOpcode::UnregisterBuffer => {
                match self.buffers.unregister(sqe.buf_index) {
                    Ok(()) => 0,
                    Err(e) => e,
                }
            }
            
            RingOpcode::ConsoleWrite => {
                self.do_console_write(sqe.buf_index, sqe.buf_offset, sqe.len)
            }
            
            _ => -38, // ENOSYS
        }
    }
    
    /// Write using registered buffer
    fn do_write(&self, fd: u32, buf_index: u16, buf_offset: u32, len: u32) -> i64 {
        use crate::kernel::core::traits::CharDevice;
        use crate::kernel::driver::serial::SERIAL1;
        
        // Get registered buffer
        let buf = match self.buffers.get(buf_index) {
            Some(b) => b,
            None => return -9, // EBADF
        };
        
        // Validate access
        if buf.validate_access(buf_offset as u64, len as u64).is_err() {
            return -14; // EFAULT
        }
        
        // No per-call validation needed! Buffer is pre-validated.
        if fd == 1 {
            // stdout -> serial
            let ptr = unsafe { buf.get_ptr(buf_offset as u64) };
            let slice = unsafe { core::slice::from_raw_parts(ptr, len as usize) };
            
            if let Some(mut serial) = SERIAL1.try_lock() {
                for &byte in slice {
                    let _ = serial.write_byte(byte);
                }
            }
            
            len as i64
        } else {
            -9 // EBADF
        }
    }
    
    /// Read using registered buffer
    fn do_read(&self, _fd: u32, _buf_index: u16, _buf_offset: u32, _len: u32) -> i64 {
        // Not implemented yet
        -38 // ENOSYS
    }
    
    /// Console write using registered buffer
    fn do_console_write(&self, buf_index: u16, buf_offset: u32, len: u32) -> i64 {
        use crate::kernel::core::traits::CharDevice;
        use crate::kernel::driver::serial::SERIAL1;
        
        let buf = match self.buffers.get(buf_index) {
            Some(b) => b,
            None => return -9,
        };
        
        if buf.validate_access(buf_offset as u64, len as u64).is_err() {
            return -14;
        }
        
        let ptr = unsafe { buf.get_ptr(buf_offset as u64) };
        let slice = unsafe { core::slice::from_raw_parts(ptr, len as usize) };
        
        if let Some(mut serial) = SERIAL1.try_lock() {
            for &byte in slice {
                let _ = serial.write_byte(byte);
            }
        }
        
        len as i64
    }
    
    /// Get mapping info for userspace
    pub fn get_mapping_info(&self) -> RingMappingInfo {
        RingMappingInfo {
            sq_header: &self.sq_header as *const _ as u64,
            cq_header: &self.cq_header as *const _ as u64,
            sq_entries: self.sq_entries.as_ptr() as u64,
            cq_entries: self.cq_entries.as_ptr() as u64,
            ring_size: RING_SIZE as u32,
        }
    }
    
    /// Get buffer registry for direct access
    pub fn buffers_mut(&mut self) -> &mut BufferRegistry {
        &mut self.buffers
    }
}

/// Mapping information for userspace
#[derive(Debug, Clone, Copy)]
pub struct RingMappingInfo {
    pub sq_header: u64,
    pub cq_header: u64,
    pub sq_entries: u64,
    pub cq_entries: u64,
    pub ring_size: u32,
}

// =============================================================================
// Ideal Syscall Entry (Doorbell-Only)
// =============================================================================

/// Ideal syscall entry point - Doorbell only, no arguments
///
/// This is the ultimate minimalist syscall entry. It takes no arguments
/// and simply signals the kernel to process the ring buffer.
///
/// # Register Usage
///
/// On entry:
/// - RCX: User RIP (saved by CPU)
/// - R11: User RFLAGS (saved by CPU)
/// - RSP: User stack pointer (must save manually)
///
/// On exit:
/// - RAX: Number of completions processed (or error code)
/// - All other registers: Undefined (caller saves what it needs)
///
/// # Assembly Flow
///
/// ```text
/// 1. swapgs               ; Switch to kernel GS
/// 2. Save user RSP        ; gs:[0x00] = RSP
/// 3. Load kernel stack    ; RSP = gs:[0x08]
/// 4. Save return info     ; push user_rsp, rcx, r11
/// 5. Call ring processor  ; process_current_ring()
/// 6. Restore and return   ; pop, swapgs, sysretq
/// ```
#[unsafe(naked)]
pub unsafe extern "C" fn ideal_syscall_entry() {
    core::arch::naked_asm!(
        // ============================================================
        // Phase 1: GS segment switch
        // ============================================================
        "swapgs",
        
        // ============================================================
        // Phase 2: Save user RSP and switch to kernel stack
        // ============================================================
        "mov qword ptr gs:[0x00], rsp",    // Save user RSP
        "mov rsp, qword ptr gs:[0x08]",    // Load kernel stack
        
        // ============================================================
        // Phase 3: Minimal context save (only what sysret needs)
        // ============================================================
        // We DON'T save callee-saved registers because:
        // 1. We don't use them in the handler
        // 2. User code should preserve what it needs (PreserveAll)
        "push qword ptr gs:[0x00]",        // User RSP
        "push rcx",                         // User RIP
        "push r11",                         // User RFLAGS
        
        // ============================================================
        // Phase 4: Call ring processor (no arguments needed!)
        // ============================================================
        "call {process_ring}",
        
        // Result is in RAX - pass through to userspace
        
        // ============================================================
        // Phase 5: Restore and return
        // ============================================================
        "pop r11",                          // User RFLAGS
        "pop rcx",                          // User RIP
        "pop rsp",                          // User RSP (direct restore)
        
        "swapgs",
        "sysretq",
        
        process_ring = sym process_current_ring,
    );
}

/// Process the ring buffer for the current process
///
/// This function is called from the ideal syscall entry and processes
/// all pending entries in the ring buffer.
///
/// Returns the number of completions generated.
#[unsafe(no_mangle)]
extern "C" fn process_current_ring() -> u64 {
    use crate::kernel::process::PROCESS_TABLE;
    
    // Increment syscall counter
    unsafe {
        super::per_cpu::current().inc_syscall_count();
    }
    
    // Get current process's ring context
    let mut table = PROCESS_TABLE.lock();
    let process = match table.current_process_mut() {
        Some(p) => p,
        None => return (-3_i64) as u64, // ESRCH
    };
    
    // Process the ring buffer
    match process.ring_context_mut() {
        Some(ctx) => {
            let completed = ctx.poll();
            completed as u64
        }
        None => {
            // Ring context not initialized - return error
            // The user should call ring_setup syscall first
            (-38_i64) as u64 // ENOSYS - function not implemented
        }
    }
}

// =============================================================================
// SQPOLL Infrastructure
// =============================================================================

/// Wrapper for RingContext pointer to make it Send + Sync
struct RingContextPtr(*mut RingContext);

unsafe impl Send for RingContextPtr {}
unsafe impl Sync for RingContextPtr {}

/// Global list of contexts registered for kernel polling
static SQPOLL_CONTEXTS: Mutex<Vec<RingContextPtr>> = Mutex::new(Vec::new());

/// Register a ring context for kernel polling
///
/// # Safety
/// The context must remain valid for the lifetime of the registration.
pub unsafe fn register_sqpoll(ctx: *mut RingContext) {
    let mut contexts = SQPOLL_CONTEXTS.lock();
    contexts.push(RingContextPtr(ctx));
}

/// Unregister a ring context from kernel polling
pub fn unregister_sqpoll(ctx: *mut RingContext) {
    let mut contexts = SQPOLL_CONTEXTS.lock();
    contexts.retain(|c| c.0 != ctx);
}

/// Kernel poller - processes all registered SQPOLL contexts
///
/// Call this from a dedicated kernel thread for maximum throughput.
/// Returns the total number of operations processed.
pub fn kernel_poll_all() -> u64 {
    let contexts = SQPOLL_CONTEXTS.lock();
    let mut total = 0u64;
    
    for ctx_ptr in contexts.iter() {
        // SAFETY: Contexts are registered by kernel and remain valid
        let ctx = unsafe { &mut *ctx_ptr.0 };
        total += ctx.poll() as u64;
    }
    
    total
}

/// Kernel poller loop for dedicated polling thread
pub fn kernel_poller_loop() -> ! {
    loop {
        let processed = kernel_poll_all();
        
        if processed == 0 {
            // No work - pause to save power
            core::hint::spin_loop();
            
            // Could also implement:
            // - mwait/monitor for better efficiency
            // - Futex-like wakeup mechanism
            // - Adaptive polling (back off under low load)
        }
    }
}

// =============================================================================
// Integration with Process
// =============================================================================

/// Initialize ring-based I/O for a process
pub fn init_ring_for_process(enable_sqpoll: bool) -> Option<Box<RingContext>> {
    let mut ctx = Box::new(RingContext::new());
    
    if enable_sqpoll {
        ctx.enable_sqpoll();
        unsafe {
            register_sqpoll(ctx.as_mut() as *mut RingContext);
        }
    }
    
    Some(ctx)
}

/// Clean up ring-based I/O for a process
///
/// # Safety
/// The context must have been created by init_ring_for_process.
pub unsafe fn cleanup_ring(ctx: Box<RingContext>) {
    unregister_sqpoll(Box::as_ref(&ctx) as *const RingContext as *mut RingContext);
    // Box automatically deallocates when dropped
}

/// Map the RingContext into user space
///
/// This function maps the kernel's RingContext structure into the user's
/// address space at USER_RING_CONTEXT_BASE, allowing direct access to
/// the submission and completion queues.
///
/// # Arguments
/// * `ctx` - The RingContext to map
/// * `user_mapper` - The user's page table mapper
/// * `frame_allocator` - Frame allocator for intermediate page tables
/// * `phys_offset` - Physical memory offset
///
/// # Returns
/// * Ok(user_address) - The user-space address where the context is mapped
/// * Err(error_code) - Negative error code on failure
///
/// # Safety
/// The caller must ensure the page table mapper is valid.
pub unsafe fn map_ring_to_user(
    ctx: &RingContext,
    user_mapper: &mut x86_64::structures::paging::OffsetPageTable,
    frame_allocator: &mut crate::kernel::mm::BootInfoFrameAllocator,
    phys_offset: x86_64::VirtAddr,
) -> Result<u64, i64> {
    use x86_64::structures::paging::{Page, PageTableFlags, Mapper, Size4KiB};
    use crate::kernel::mm::user_paging::USER_RING_CONTEXT_BASE;
    
    // Calculate the physical address of the RingContext
    // Since RingContext is in kernel heap, we use direct map formula
    let ctx_addr = ctx as *const RingContext as u64;
    let ctx_size = core::mem::size_of::<RingContext>();
    let num_pages = (ctx_size + 4095) / 4096;
    
    debug_println!(
        "[map_ring_to_user] Mapping RingContext at kernel {:#x} ({} bytes, {} pages) to user {:#x}",
        ctx_addr, ctx_size, num_pages, USER_RING_CONTEXT_BASE
    );
    
    for i in 0..num_pages {
        let kernel_addr = ctx_addr + (i * 4096) as u64;
        let user_addr = USER_RING_CONTEXT_BASE + (i * 4096) as u64;
        
        // Calculate physical address from kernel virtual address
        // Kernel heap uses direct physical map: virt = phys + phys_offset
        let phys_addr = kernel_addr - phys_offset.as_u64();
        let phys_frame = x86_64::structures::paging::PhysFrame::<Size4KiB>::containing_address(
            x86_64::PhysAddr::new(phys_addr)
        );
        
        let user_page: Page<Size4KiB> = Page::containing_address(
            x86_64::VirtAddr::new(user_addr)
        );
        
        // Map with USER_ACCESSIBLE + WRITABLE
        let flags = PageTableFlags::PRESENT 
            | PageTableFlags::WRITABLE 
            | PageTableFlags::USER_ACCESSIBLE
            | PageTableFlags::NO_EXECUTE;
        
        // Check if already mapped
        use x86_64::structures::paging::mapper::TranslateResult;
        use x86_64::structures::paging::Translate;
        if let TranslateResult::Mapped { .. } = user_mapper.translate(x86_64::VirtAddr::new(user_addr)) {
            debug_println!(
                "[map_ring_to_user] User address {:#x} already mapped, skipping",
                user_addr
            );
            continue;
        }
        
        match user_mapper.map_to(user_page, phys_frame, flags, frame_allocator) {
            Ok(flush) => {
                flush.flush();
                debug_println!(
                    "[map_ring_to_user] Mapped user {:#x} -> phys {:#x}",
                    user_addr, phys_addr
                );
            }
            Err(e) => {
                debug_println!(
                    "[map_ring_to_user] Failed to map page {}: {:?}",
                    i, e
                );
                return Err(-12); // ENOMEM
            }
        }
    }
    
    debug_println!("[map_ring_to_user] Successfully mapped RingContext to user space");
    Ok(USER_RING_CONTEXT_BASE)
}

// =============================================================================
// Initialization
// =============================================================================

/// Initialize the ring-based syscall system
pub fn init() {
    debug_println!("[Ring Syscall] Initializing ring-based syscall system");
    debug_println!("  Ring size: {} entries", RING_SIZE);
    debug_println!("  SQE size: {} bytes", core::mem::size_of::<IdealSqe>());
    debug_println!("  CQE size: {} bytes", core::mem::size_of::<IdealCqe>());
    debug_println!("  Max registered buffers: {}", MAX_REGISTERED_BUFFERS);
    debug_println!("[Ring Syscall] Ready");
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test_case]
    fn test_sqe_size() {
        assert_eq!(core::mem::size_of::<IdealSqe>(), 64);
    }
    
    #[test_case]
    fn test_cqe_size() {
        assert_eq!(core::mem::size_of::<IdealCqe>(), 16);
    }
    
    #[test_case]
    fn test_ring_header_size() {
        assert_eq!(core::mem::size_of::<RingHeader>(), 64);
    }
    
    #[test_case]
    fn test_buffer_validation() {
        let mut buf = RegisteredBuffer::empty();
        assert!(buf.validate_access(0, 100).is_err());
        
        buf.in_use = true;
        buf.size = 1024;
        assert!(buf.validate_access(0, 100).is_ok());
        assert!(buf.validate_access(0, 2000).is_err());
        assert!(buf.validate_access(1000, 100).is_err());
    }
}
