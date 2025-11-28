//! Process Scheduler

use crate::kernel::process::{ProcessId, PROCESS_TABLE};
use spin::Mutex;
use alloc::vec::Vec;
use lazy_static::lazy_static;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Simple round-robin scheduler
pub struct RoundRobinScheduler {
    current_pid: Option<ProcessId>,
}

impl Default for RoundRobinScheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl RoundRobinScheduler {
    /// Creates a new round-robin scheduler.
    pub const fn new() -> Self {
        Self { current_pid: None }
    }
    
    /// Select next process to run
    pub fn schedule(&mut self) -> Option<ProcessId> {
        let table = PROCESS_TABLE.lock();
        
        // Get all ready processes
        let ready: Vec<_> = table
            .ready_processes()
            .map(|p| p.pid())
            .collect();
        
        if ready.is_empty() {
            return None;
        }
        
        // Round-robin: pick next after current
        let next_idx = if let Some(current) = self.current_pid {
            ready
                .iter()
                .position(|&pid| pid == current)
                .map(|idx| (idx + 1) % ready.len())
                .unwrap_or(0)
        } else {
            0
        };
        
        let next_pid = ready[next_idx];
        self.current_pid = Some(next_pid);
        
        Some(next_pid)
    }
}

lazy_static! {
    /// Global round-robin scheduler instance.
    pub static ref SCHEDULER: Mutex<RoundRobinScheduler> = 
        Mutex::new(RoundRobinScheduler::new());
}

// ============================================================================
// SQPOLL (Submission Queue Polling) Support
// ============================================================================

/// SQPOLL configuration
pub struct SqpollConfig {
    /// Whether SQPOLL is enabled globally
    enabled: AtomicBool,
    /// Number of polls performed
    poll_count: AtomicU64,
    /// Number of operations processed
    ops_processed: AtomicU64,
}

impl SqpollConfig {
    /// Create a new SQPOLL configuration (disabled by default)
    pub const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            poll_count: AtomicU64::new(0),
            ops_processed: AtomicU64::new(0),
        }
    }
    
    /// Enable SQPOLL
    pub fn enable(&self) {
        self.enabled.store(true, Ordering::Release);
        crate::debug_println!("[SQPOLL] Enabled");
    }
    
    /// Disable SQPOLL
    pub fn disable(&self) {
        self.enabled.store(false, Ordering::Release);
        crate::debug_println!("[SQPOLL] Disabled");
    }
    
    /// Check if SQPOLL is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Acquire)
    }
    
    /// Get poll statistics
    pub fn stats(&self) -> (u64, u64) {
        (
            self.poll_count.load(Ordering::Relaxed),
            self.ops_processed.load(Ordering::Relaxed),
        )
    }
    
    /// Record a poll cycle
    fn record_poll(&self, ops: u64) {
        self.poll_count.fetch_add(1, Ordering::Relaxed);
        if ops > 0 {
            self.ops_processed.fetch_add(ops, Ordering::Relaxed);
        }
    }
}

lazy_static! {
    /// Global SQPOLL configuration
    pub static ref SQPOLL: SqpollConfig = SqpollConfig::new();
}

/// Poll all ring buffers from SQPOLL-enabled processes
///
/// This function should be called from the idle loop or a dedicated
/// polling thread. It processes pending submissions in all registered
/// ring buffers.
///
/// # Returns
/// Total number of operations processed across all processes
pub fn sqpoll_tick() -> u64 {
    if !SQPOLL.is_enabled() {
        return 0;
    }
    
    // Poll all registered SQPOLL contexts
    let processed = crate::arch::x86_64::syscall_ring::kernel_poll_all();
    
    // Record statistics
    SQPOLL.record_poll(processed);
    
    processed
}

/// Idle loop with SQPOLL support
///
/// This function is called when there are no ready processes.
/// Instead of just halting, it polls ring buffers for pending work.
///
/// # Arguments
/// * `max_iterations` - Maximum poll iterations before halting (0 = unlimited)
///
/// # Returns
/// - `true` if work was processed and scheduler should be checked
/// - `false` if no work was found
pub fn idle_with_sqpoll(max_iterations: usize) -> bool {
    let mut found_work = false;
    let iterations = if max_iterations == 0 { usize::MAX } else { max_iterations };
    
    for _ in 0..iterations {
        let processed = sqpoll_tick();
        
        if processed > 0 {
            found_work = true;
        } else {
            // No work - yield CPU
            core::hint::spin_loop();
            
            // Optionally halt until next interrupt
            // This saves power but adds latency
            // x86_64::instructions::hlt();
            
            break;
        }
    }
    
    found_work
}

/// Main kernel idle entry point with SQPOLL
///
/// Call this instead of `hlt` in the main kernel loop.
pub fn kernel_idle() {
    // First, try SQPOLL
    if SQPOLL.is_enabled() {
        // Do a few poll iterations
        if idle_with_sqpoll(10) {
            // Work was found - don't halt, return to scheduler
            return;
        }
    }
    
    // No SQPOLL work - halt until next interrupt
    x86_64::instructions::hlt();
}
