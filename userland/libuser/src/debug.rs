//! Debug utilities for userland programs
//!
//! Provides debugging helpers for development and testing

/// Hexdump a memory region
pub fn hexdump(data: &[u8], base_addr: usize) {
    const BYTES_PER_LINE: usize = 16;
    
    for (i, chunk) in data.chunks(BYTES_PER_LINE).enumerate() {
        let addr = base_addr + (i * BYTES_PER_LINE);
        
        // Print address
        crate::print!("{:08x}  ", addr);
        
        // Print hex bytes
        for (j, byte) in chunk.iter().enumerate() {
            crate::print!("{:02x} ", byte);
            if j == 7 {
                crate::print!(" ");
            }
        }
        
        // Pad if last line
        if chunk.len() < BYTES_PER_LINE {
            for j in chunk.len()..BYTES_PER_LINE {
                crate::print!("   ");
                if j == 7 {
                    crate::print!(" ");
                }
            }
        }
        
        // Print ASCII
        crate::print!(" |");
        for byte in chunk {
            if *byte >= 0x20 && *byte <= 0x7e {
                crate::print!("{}", *byte as char);
            } else {
                crate::print!(".");
            }
        }
        crate::println!("|");
    }
}

/// Measure execution time using TSC
pub struct Timer {
    start: u64,
}

impl Timer {
    /// Start a timer
    pub fn start() -> Self {
        Self {
            start: Self::read_tsc(),
        }
    }
    
    /// Get elapsed cycles
    pub fn elapsed(&self) -> u64 {
        Self::read_tsc() - self.start
    }
    
    /// Read Time Stamp Counter
    fn read_tsc() -> u64 {
        let low: u32;
        let high: u32;
        unsafe {
            core::arch::asm!(
                "rdtsc",
                out("eax") low,
                out("edx") high,
                options(nomem, nostack)
            );
        }
        ((high as u64) << 32) | (low as u64)
    }
}
