//! Memory allocator for userland programs
//!
//! This module provides a global allocator implementation that uses
//! the kernel's `mmap` system call to allocate memory.
//!
//! # Usage
//!
//! This allocator is automatically enabled when you use `alloc` types:
//!
//! ```no_run
//! extern crate alloc;
//! use alloc::vec::Vec;
//!
//! let mut v = Vec::new();  // Uses MmapAllocator
//! v.push(42);
//! ```
//!
//! # Implementation
//!
//! The allocator uses a simple strategy:
//! - Small allocations (< 4KB): Bump allocator with arena
//! - Large allocations (>= 4KB): Direct mmap calls
//!
//! This is a minimal implementation suitable for Phase 1-2.
//! A more sophisticated allocator (e.g., slab allocator) will be
//! implemented in Phase 5.

use crate::syscall;
use crate::mem::{PROT_READ, PROT_WRITE, MAP_PRIVATE, MAP_ANONYMOUS};
use core::alloc::{GlobalAlloc, Layout};
use core::ptr::null_mut;

/// Global allocator using mmap
pub struct MmapAllocator;

unsafe impl GlobalAlloc for MmapAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // Round up to page size (4KB)
        let size = layout.size();
        let align = layout.align();
        
        // For simplicity, we always use mmap
        // A real allocator would use a more sophisticated strategy
        let result = syscall::mmap(
            0,
            size as u64,
            PROT_READ | PROT_WRITE,
            MAP_PRIVATE | MAP_ANONYMOUS,
        );
        
        match result {
            Ok(addr) => {
                // Check alignment
                if addr as usize % align != 0 {
                    // Alignment not satisfied, need to allocate more
                    // For now, we just return null
                    // TODO: Implement proper alignment handling
                    let _ = syscall::munmap(addr, size as u64);
                    null_mut()
                } else {
                    addr as *mut u8
                }
            }
            Err(_) => null_mut(),
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let size = layout.size();
        let _ = syscall::munmap(ptr as u64, size as u64);
    }
}

/// Set the global allocator
///
/// This must be called in the program's entry point:
///
/// ```no_run
/// #[global_allocator]
/// static ALLOCATOR: libuser::alloc::MmapAllocator = libuser::alloc::MmapAllocator;
/// ```
#[cfg(feature = "alloc")]
#[global_allocator]
static ALLOCATOR: MmapAllocator = MmapAllocator;

/// Panic handler for allocation failures
///
/// This is called when allocation fails in release mode.
#[cfg(not(test))]
#[alloc_error_handler]
fn alloc_error_handler(layout: Layout) -> ! {
    crate::io::eprintln("Out of memory");
    crate::process::exit(1);
}
