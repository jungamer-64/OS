//! Memory allocator for userland programs
//!
//! This module provides a global allocator implementation that uses
//! the kernel's `mmap` system call to allocate memory.
//!
//! # Usage
//!
//! To enable heap allocation in your program, add the following:
//!
//! ```no_run
//! #[global_allocator]
//! static ALLOCATOR: libuser::alloc::MmapAllocator = libuser::alloc::MmapAllocator;
//! ```
//!
//! Then you can use Rust's standard `alloc` types:
//!
//! ```no_run
//! extern crate alloc;
//! use alloc::vec::Vec;
//!
//! let mut v = Vec::new();
//! v.push(42);
//! ```
//!
//! # Implementation Note
//!
//! This is a minimal allocator suitable for Phase 1-2.
//! A more sophisticated allocator will be implemented in Phase 5.

use crate::syscall;
use crate::mem::{PROT_READ, PROT_WRITE, MAP_PRIVATE, MAP_ANONYMOUS};
use core::alloc::{GlobalAlloc, Layout};
use core::ptr::null_mut;

/// Global allocator using mmap
///
/// This allocator directly maps each allocation request to a `mmap` call.
/// This is simple but not very efficient for small allocations.
pub struct MmapAllocator;

unsafe impl GlobalAlloc for MmapAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let size = layout.size();
        
        // Directly use mmap for all allocations
        match syscall::mmap(
            0,
            size as u64,
            PROT_READ | PROT_WRITE,
            MAP_PRIVATE | MAP_ANONYMOUS,
        ) {
            Ok(addr) => addr as *mut u8,
            Err(_) => null_mut(),
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let size = layout.size();
        let _ = syscall::munmap(ptr as u64, size as u64);
    }
}
