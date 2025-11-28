// kernel/src/arch/x86_64/smp.rs
//! Symmetric Multi-Processing (SMP) Support
//!
//! This module implements AP (Application Processor) initialization for
//! multi-core systems. The BSP (Bootstrap Processor) wakes up APs using
//! the INIT-SIPI-SIPI sequence.
//!
//! # Boot Sequence
//!
//! ```text
//! 1. BSP detects number of CPUs via ACPI/MP tables
//! 2. BSP allocates per-CPU stacks and data
//! 3. BSP sends INIT IPI to all APs
//! 4. After 10ms, BSP sends SIPI to all APs
//! 5. After 200μs, BSP sends second SIPI
//! 6. APs wake up in real mode at SIPI vector address
//! 7. APs transition to protected mode, then long mode
//! 8. APs set up their own GDT, IDT, Per-CPU data
//! 9. APs signal ready to BSP and enter idle loop
//! ```
//!
//! # Memory Layout for AP Boot
//!
//! The AP boot code must be placed below 1MB because APs start in real mode.
//! We use a trampoline page at a well-known address (e.g., 0x8000).
//!
//! ```text
//! 0x8000 - 0x8FFF: AP trampoline code (16-bit -> 32-bit -> 64-bit)
//! 0x9000 - 0x9FFF: Per-AP boot data (stack pointer, GDT pointer, etc.)
//! ```

use core::sync::atomic::{AtomicU32, AtomicU64, AtomicBool, Ordering};
use alloc::vec::Vec;

use x86_64::VirtAddr;

use crate::debug_println;
use super::per_cpu;

/// Maximum number of CPUs supported
pub const MAX_CPUS: usize = 256;

/// AP trampoline address (must be below 1MB, page-aligned)
pub const AP_TRAMPOLINE_ADDR: u64 = 0x8000;

/// AP boot data address
pub const AP_BOOT_DATA_ADDR: u64 = 0x9000;

/// Local APIC base address (from MSR 0x1B)
pub const LAPIC_BASE_MSR: u32 = 0x1B;

/// Local APIC register offsets
pub mod lapic {
    /// Local APIC ID Register
    pub const ID: u32 = 0x020;
    /// Local APIC Version Register
    pub const VERSION: u32 = 0x030;
    /// Task Priority Register
    pub const TPR: u32 = 0x080;
    /// EOI Register
    pub const EOI: u32 = 0x0B0;
    /// Spurious Interrupt Vector Register
    pub const SIVR: u32 = 0x0F0;
    /// Interrupt Command Register (low)
    pub const ICR_LOW: u32 = 0x300;
    /// Interrupt Command Register (high)
    pub const ICR_HIGH: u32 = 0x310;
}

/// IPI delivery modes
pub mod ipi {
    /// INIT IPI
    pub const INIT: u32 = 0x0500;
    /// Startup IPI (SIPI)
    pub const SIPI: u32 = 0x0600;
    /// Level assert
    pub const LEVEL_ASSERT: u32 = 0x4000;
    /// Level de-assert
    pub const LEVEL_DEASSERT: u32 = 0x0000;
    /// Destination shorthand: All excluding self
    pub const ALL_EXCLUDING_SELF: u32 = 0xC0000;
}

/// Per-CPU boot information passed to APs
#[repr(C, align(64))]
pub struct ApBootData {
    /// Stack pointer for this AP
    pub stack_top: u64,
    /// GDT pointer (limit + base)
    pub gdt_ptr: [u8; 10],
    /// IDT pointer (limit + base)
    pub idt_ptr: [u8; 10],
    /// CR3 value (page table root)
    pub cr3: u64,
    /// CPU ID for this AP
    pub cpu_id: u32,
    /// Flag indicating AP is ready
    pub ready: AtomicBool,
    /// Per-CPU data address
    pub per_cpu_addr: u64,
}

/// Global SMP state
pub struct SmpState {
    /// Number of CPUs detected
    pub cpu_count: AtomicU32,
    
    /// Number of APs that have started
    pub aps_started: AtomicU32,
    
    /// BSP APIC ID
    pub bsp_apic_id: AtomicU32,
    
    /// Local APIC base address (virtual)
    pub lapic_base: AtomicU64,
    
    /// Whether SMP initialization is complete
    pub initialized: AtomicBool,
}

impl SmpState {
    /// Create a new SMP state
    pub const fn new() -> Self {
        Self {
            cpu_count: AtomicU32::new(1), // At least BSP
            aps_started: AtomicU32::new(0),
            bsp_apic_id: AtomicU32::new(0),
            lapic_base: AtomicU64::new(0),
            initialized: AtomicBool::new(false),
        }
    }
}

/// Global SMP state
static SMP_STATE: SmpState = SmpState::new();

/// Get the SMP state
pub fn state() -> &'static SmpState {
    &SMP_STATE
}

/// Read Local APIC register
/// 
/// # Safety
/// 
/// Requires valid LAPIC base address
unsafe fn lapic_read(offset: u32) -> u32 {
    let base = SMP_STATE.lapic_base.load(Ordering::Relaxed);
    let addr = (base + u64::from(offset)) as *const u32;
    // SAFETY: Caller ensures LAPIC is mapped and offset is valid
    unsafe { core::ptr::read_volatile(addr) }
}

/// Write Local APIC register
/// 
/// # Safety
/// 
/// Requires valid LAPIC base address
unsafe fn lapic_write(offset: u32, value: u32) {
    let base = SMP_STATE.lapic_base.load(Ordering::Relaxed);
    let addr = (base + u64::from(offset)) as *mut u32;
    // SAFETY: Caller ensures LAPIC is mapped and offset is valid
    unsafe { core::ptr::write_volatile(addr, value); }
}

/// Get the Local APIC ID of the current processor
pub fn get_apic_id() -> u32 {
    if SMP_STATE.lapic_base.load(Ordering::Relaxed) == 0 {
        return 0;
    }
    
    // SAFETY: LAPIC base is initialized
    unsafe { (lapic_read(lapic::ID) >> 24) & 0xFF }
}

/// Initialize the Local APIC for the BSP
pub fn init_bsp_lapic(phys_mem_offset: u64) {
    use x86_64::registers::model_specific::Msr;
    
    // Read LAPIC base from MSR
    let lapic_msr = unsafe {
        let msr = Msr::new(LAPIC_BASE_MSR);
        msr.read()
    };
    
    let lapic_phys = lapic_msr & 0xFFFF_F000;
    let lapic_virt = lapic_phys + phys_mem_offset;
    
    SMP_STATE.lapic_base.store(lapic_virt, Ordering::Release);
    
    // Get BSP APIC ID
    let bsp_id = unsafe { (lapic_read(lapic::ID) >> 24) & 0xFF };
    SMP_STATE.bsp_apic_id.store(bsp_id, Ordering::Release);
    
    // Enable Local APIC (set bit 8 of SIVR)
    unsafe {
        let sivr = lapic_read(lapic::SIVR);
        lapic_write(lapic::SIVR, sivr | 0x100);
    }
    
    debug_println!("[SMP] BSP LAPIC initialized");
    debug_println!("  LAPIC physical: {:#x}", lapic_phys);
    debug_println!("  LAPIC virtual: {:#x}", lapic_virt);
    debug_println!("  BSP APIC ID: {}", bsp_id);
}

/// Send an IPI (Inter-Processor Interrupt)
/// 
/// # Safety
/// 
/// Requires LAPIC to be initialized
pub unsafe fn send_ipi(dest_apic_id: u32, vector: u32, delivery_mode: u32) {
    // Set destination APIC ID
    // SAFETY: LAPIC is initialized
    unsafe {
        lapic_write(lapic::ICR_HIGH, dest_apic_id << 24);
        
        // Send IPI
        let icr_low = vector | delivery_mode;
        lapic_write(lapic::ICR_LOW, icr_low);
    }
    
    // Wait for delivery
    while unsafe { lapic_read(lapic::ICR_LOW) } & 0x1000 != 0 {
        core::hint::spin_loop();
    }
}

/// Send INIT IPI to all APs
pub fn send_init_all() {
    debug_println!("[SMP] Sending INIT IPI to all APs");
    
    unsafe {
        // Assert INIT
        lapic_write(lapic::ICR_HIGH, 0);
        lapic_write(lapic::ICR_LOW, ipi::INIT | ipi::LEVEL_ASSERT | ipi::ALL_EXCLUDING_SELF);
        
        // Wait for delivery
        while lapic_read(lapic::ICR_LOW) & 0x1000 != 0 {
            core::hint::spin_loop();
        }
        
        // De-assert INIT
        lapic_write(lapic::ICR_LOW, ipi::INIT | ipi::LEVEL_DEASSERT | ipi::ALL_EXCLUDING_SELF);
        
        while lapic_read(lapic::ICR_LOW) & 0x1000 != 0 {
            core::hint::spin_loop();
        }
    }
}

/// Send SIPI (Startup IPI) to all APs
/// 
/// The vector specifies the page number (4KB) where AP startup code is located.
/// For example, vector 0x08 means startup code at physical address 0x8000.
pub fn send_sipi_all(vector: u8) {
    debug_println!("[SMP] Sending SIPI to all APs, vector={:#x}", vector);
    
    unsafe {
        lapic_write(lapic::ICR_HIGH, 0);
        lapic_write(lapic::ICR_LOW, u32::from(vector) | ipi::SIPI | ipi::ALL_EXCLUDING_SELF);
        
        // Wait for delivery
        while lapic_read(lapic::ICR_LOW) & 0x1000 != 0 {
            core::hint::spin_loop();
        }
    }
}

/// Delay for approximately the given number of microseconds
/// 
/// Uses a simple busy-wait loop. Not precise, but sufficient for SMP init.
fn delay_us(us: u64) {
    // Rough approximation: ~1000 iterations per microsecond at 1GHz
    // This is imprecise but good enough for INIT-SIPI delays
    let iterations = us * 1000;
    for _ in 0..iterations {
        core::hint::spin_loop();
    }
}

/// Initialize SMP and start all APs
/// 
/// This function:
/// 1. Detects the number of CPUs (currently hardcoded)
/// 2. Sets up AP trampoline code
/// 3. Sends INIT-SIPI-SIPI sequence
/// 4. Waits for APs to start
/// 
/// # Arguments
/// * `phys_mem_offset` - Physical memory offset for LAPIC mapping
/// * `cpu_count` - Number of CPUs to start (from ACPI/MP tables)
pub fn init(phys_mem_offset: u64, cpu_count: u32) {
    if cpu_count <= 1 {
        debug_println!("[SMP] Single CPU system, skipping AP init");
        SMP_STATE.cpu_count.store(1, Ordering::Release);
        SMP_STATE.initialized.store(true, Ordering::Release);
        return;
    }
    
    debug_println!("[SMP] Initializing {} CPUs", cpu_count);
    
    // Initialize BSP LAPIC
    init_bsp_lapic(phys_mem_offset);
    
    // Store CPU count
    SMP_STATE.cpu_count.store(cpu_count, Ordering::Release);
    
    // TODO: Copy AP trampoline code to AP_TRAMPOLINE_ADDR
    // TODO: Set up per-AP boot data at AP_BOOT_DATA_ADDR
    
    // For now, skip actual AP startup (requires trampoline code)
    debug_println!("[SMP] AP startup not yet implemented");
    debug_println!("[SMP] Running in single-CPU mode");
    
    /*
    // Send INIT IPI to all APs
    send_init_all();
    
    // Wait 10ms
    delay_us(10_000);
    
    // Send first SIPI
    let sipi_vector = (AP_TRAMPOLINE_ADDR / 0x1000) as u8;
    send_sipi_all(sipi_vector);
    
    // Wait 200μs
    delay_us(200);
    
    // Send second SIPI (some CPUs need this)
    send_sipi_all(sipi_vector);
    
    // Wait for APs to start (timeout: 1 second)
    let expected_aps = cpu_count - 1;
    let timeout_us = 1_000_000u64;
    let start = 0u64; // TODO: read timestamp counter
    
    while SMP_STATE.aps_started.load(Ordering::Acquire) < expected_aps {
        // TODO: check timeout
        core::hint::spin_loop();
    }
    
    debug_println!("[SMP] {} APs started successfully", 
        SMP_STATE.aps_started.load(Ordering::Relaxed));
    */
    
    SMP_STATE.initialized.store(true, Ordering::Release);
    debug_println!("[SMP] Initialization complete");
}

/// Entry point for APs (called from assembly trampoline)
/// 
/// # Safety
/// 
/// This function is called from assembly code in AP trampoline.
/// The cpu_id must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ap_entry(cpu_id: u32) {
    debug_println!("[SMP] AP {} started", cpu_id);
    
    // Initialize Per-CPU data for this AP
    // TODO: per_cpu::init_ap(cpu_id);
    
    // Set up GDT for this AP
    // TODO: Load AP-specific GDT
    
    // Set up IDT for this AP (shared with BSP)
    // TODO: Load IDT
    
    // Enable interrupts
    // x86_64::instructions::interrupts::enable();
    
    // Signal that this AP is ready
    SMP_STATE.aps_started.fetch_add(1, Ordering::Release);
    
    // Enter idle loop
    loop {
        // TODO: Check for work in scheduler
        x86_64::instructions::hlt();
    }
}

/// Get the number of online CPUs
pub fn cpu_count() -> u32 {
    SMP_STATE.cpu_count.load(Ordering::Acquire)
}

/// Check if SMP is initialized
pub fn is_initialized() -> bool {
    SMP_STATE.initialized.load(Ordering::Acquire)
}

/// Get the current CPU ID
/// 
/// Returns the APIC ID of the current processor
pub fn current_cpu_id() -> u32 {
    get_apic_id()
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test_case]
    fn test_smp_state_creation() {
        // Just verify the static state is created correctly
        assert_eq!(SMP_STATE.cpu_count.load(Ordering::Relaxed), 1);
    }
    
    #[test_case]
    fn test_max_cpus() {
        assert_eq!(MAX_CPUS, 256);
    }
}
