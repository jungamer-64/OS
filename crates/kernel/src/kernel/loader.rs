// src/kernel/loader.rs
//! Simple program loader for Phase 2

use x86_64::VirtAddr;
// use crate::kernel::process::{Process, ProcessId};
use crate::kernel::mm::user_paging::{map_user_code, map_user_stack, USER_CODE_BASE, DEFAULT_USER_STACK_SIZE};
use x86_64::structures::paging::{OffsetPageTable, FrameAllocator, Size4KiB};

/// Loaded program information
pub struct LoadedProgram {
    /// Entry point address of the program
    pub entry_point: VirtAddr,
    /// Top of the user stack
    pub stack_top: VirtAddr,
}

/// Load error types
#[derive(Debug)]
pub enum LoadError {
    /// Process creation failed
    ProcessCreationFailure,
    /// Memory mapping failed
    MappingFailure,
}

/// Embedded user program
/// 
/// In Phase 2, we embed the compiled user program directly into the kernel.
/// Phase 3 will implement proper ELF loading from disk.
/// 
/// Load embedded user program into a new process
///
/// # Arguments
/// * `data` - Program binary data
/// * `mapper` - User page table mapper
/// * `frame_allocator` - Frame allocator
///
/// # Returns
/// LoadedProgram info
pub fn load_user_program<A>(
    data: &[u8],
    mapper: &mut OffsetPageTable,
    frame_allocator: &mut A,
) -> Result<LoadedProgram, LoadError>
where
    A: FrameAllocator<Size4KiB>,
{
    let code = data;
    // Entry point at the start of the binary (release build)
    let entry_point = VirtAddr::new(USER_CODE_BASE);
    
    // DEBUG: Print first 16 bytes of user program
    crate::debug_println!("[load_user_program] User code size: {} bytes", code.len());
    crate::debug_println!("[load_user_program] First 16 bytes: {:02x?}", 
        &code[..16.min(code.len())]);
    
    // Map code into user space
    unsafe {
        map_user_code(mapper, code, entry_point, frame_allocator)
            .map_err(|_| LoadError::MappingFailure)?;
    }
    
    // Map stack
    let stack_top = unsafe {
        map_user_stack(mapper, DEFAULT_USER_STACK_SIZE, frame_allocator)
            .map_err(|_| LoadError::MappingFailure)?
    };
    
    Ok(LoadedProgram {
        entry_point,
        stack_top,
    })
}
