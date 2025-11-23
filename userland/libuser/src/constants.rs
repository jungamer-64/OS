//! Common constants for userland programs
//!
//! This module provides system-wide constants used by userland programs.

/// Page size (4KB)
///
/// This is the standard x86-64 page size.
/// All memory allocations should be aligned to this size.
pub const PAGE_SIZE: usize = 4096;

/// Page size as u64
pub const PAGE_SIZE_U64: u64 = 4096;

/// Round up to page size
///
/// # Examples
/// ```
/// use libuser::constants::round_up_to_page_size;
///
/// assert_eq!(round_up_to_page_size(0), 0);
/// assert_eq!(round_up_to_page_size(1), 4096);
/// assert_eq!(round_up_to_page_size(4096), 4096);
/// assert_eq!(round_up_to_page_size(4097), 8192);
/// ```
pub const fn round_up_to_page_size(size: usize) -> usize {
    (size + PAGE_SIZE - 1) & !(PAGE_SIZE - 1)
}

/// Round up to page size (u64 version)
pub const fn round_up_to_page_size_u64(size: u64) -> u64 {
    (size + PAGE_SIZE_U64 - 1) & !(PAGE_SIZE_U64 - 1)
}

/// User space address range
pub mod address_range {
    /// Start of user space
    pub const USER_SPACE_START: u64 = 0x0000_0000_0000_0000;
    
    /// End of user space (exclusive)
    pub const USER_SPACE_END: u64 = 0x0000_8000_0000_0000;
    
    /// Start of kernel space
    pub const KERNEL_SPACE_START: u64 = 0xFFFF_8000_0000_0000;
    
    /// Check if an address is in user space
    pub const fn is_user_address(addr: u64) -> bool {
        addr < USER_SPACE_END
    }
}

/// Standard file descriptors
pub mod fd {
    /// Standard input
    pub const STDIN: u64 = 0;
    /// Standard output
    pub const STDOUT: u64 = 1;
    /// Standard error
    pub const STDERR: u64 = 2;
}

/// Exit codes
pub mod exit_code {
    /// Success
    pub const SUCCESS: i32 = 0;
    /// General error
    pub const ERROR: i32 = 1;
    /// Invalid usage
    pub const USAGE_ERROR: i32 = 2;
}
