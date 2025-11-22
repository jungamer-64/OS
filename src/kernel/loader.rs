//! Simple program loader for Phase 2

use x86_64::VirtAddr;
// use crate::kernel::process::{Process, ProcessId};
use crate::kernel::mm::user_paging::{map_user_code, map_user_stack, USER_CODE_BASE};
use x86_64::structures::paging::{OffsetPageTable, FrameAllocator, Size4KiB};

/// Loaded program information
pub struct LoadedProgram {
    pub entry_point: VirtAddr,
    pub stack_top: VirtAddr,
}

/// Load error types
#[derive(Debug)]
pub enum LoadError {
    ProcessCreationFailure,
    MappingFailure,
}

/// Embedded user program
/// 
/// In Phase 2, we embed the compiled user program directly into the kernel.
/// Phase 3 will implement proper ELF loading from disk.
/// 
/// Note: We use a dummy array for now until we have a real user program.
#[repr(C, align(4096))]
struct EmbeddedProgram {
    // data: [u8; include_bytes!("../../target/userland/user_program.bin").len()],
    // Temporary placeholder: infinite loop
    // eb fe (jmp $)
    data: [u8; 2], 
}

static USER_PROGRAM: EmbeddedProgram = EmbeddedProgram {
    // data: *include_bytes!("../../target/userland/user_program.bin"),
    data: [0xeb, 0xfe], // jmp $
};

/// Load embedded user program into a new process
///
/// # Arguments
/// * `mapper` - User page table mapper
/// * `frame_allocator` - Frame allocator
///
/// # Returns
/// LoadedProgram info
pub fn load_user_program<A>(
    mapper: &mut OffsetPageTable,
    frame_allocator: &mut A,
) -> Result<LoadedProgram, LoadError>
where
    A: FrameAllocator<Size4KiB>,
{
    let code = &USER_PROGRAM.data;
    let entry_point = VirtAddr::new(USER_CODE_BASE);
    
    // Map code into user space
    // We need to pass physical_memory_offset? 
    // map_user_code implementation in user_paging.rs uses global PHYS_MEM_OFFSET
    unsafe {
        map_user_code(mapper, code, entry_point, frame_allocator)
            .map_err(|_| LoadError::MappingFailure)?;
    }
    
    // Map stack
    let stack_top = unsafe {
        map_user_stack(mapper, 64 * 1024, frame_allocator)
            .map_err(|_| LoadError::MappingFailure)?
    };
    
    Ok(LoadedProgram {
        entry_point,
        stack_top,
    })
}
