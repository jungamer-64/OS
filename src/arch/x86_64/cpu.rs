// src/arch/x86_64/cpu.rs

use crate::arch::Cpu;
use x86_64::instructions::{hlt, interrupts};

pub struct X86Cpu;

impl Cpu for X86Cpu {
    fn halt() {
        hlt();
    }
    
    fn disable_interrupts() {
        interrupts::disable();
    }
    
    fn enable_interrupts() {
        interrupts::enable();
    }
    
    fn are_interrupts_enabled() -> bool {
        interrupts::are_enabled()
    }
}

/// Read the Time Stamp Counter (TSC).
pub fn read_timestamp() -> u64 {
    unsafe { core::arch::x86_64::_rdtsc() }
}
