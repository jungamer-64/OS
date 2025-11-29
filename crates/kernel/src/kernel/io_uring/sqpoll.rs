// kernel/src/kernel/io_uring/sqpoll.rs
//! SQPOLL (Submission Queue Polling) Worker
//!
//! This module implements a kernel-side polling thread that continuously
//! monitors submission queues, eliminating the need for syscalls in the
//! I/O hot path.
//!
//! # Architecture
//!
//! ```text
//! User Space                          Kernel Space
//! ┌─────────────────┐                ┌─────────────────────┐
//! │   Application   │                │   SQPOLL Worker     │
//! │                 │                │   (kernel thread)   │
//! │ ┌─────────────┐ │                │                     │
//! │ │ Write SQE   │ │                │   ┌───────────────┐ │
//! │ │ Update tail │ │   no syscall   │   │ Poll SQ tail  │ │
//! │ └─────────────┘ │ ─────────────► │   │ Process SQEs  │ │
//! │                 │                │   │ Update CQ     │ │
//! │ ┌─────────────┐ │                │   └───────────────┘ │
//! │ │ Read CQE    │◄┼────────────────┼─────────────────────│
//! │ └─────────────┘ │                │                     │
//! └─────────────────┘                └─────────────────────┘
//! ```
//!
//! # Benefits
//!
//! - **Zero syscall I/O**: No kernel transition for submitting operations
//! - **Lower latency**: Immediate processing when worker is polling
//! - **Reduced context switches**: Kernel thread stays active
//!
//! # Power Management
//!
//! The worker uses adaptive polling:
//! - Active polling when submissions are frequent
//! - Idle timeout transitions to sleep state
//! - Woken by syscall when new submissions arrive after idle

use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use spin::Mutex;

use crate::debug_println;
use crate::kernel::process::ProcessId;

/// SQPOLL worker configuration
#[derive(Debug, Clone, Copy)]
pub struct SqPollConfig {
    /// CPU affinity (which CPU to run on)
    pub cpu_id: u32,
    
    /// Idle timeout in microseconds before sleeping
    /// Default: 1000μs (1ms)
    pub idle_timeout_us: u32,
    
    /// Maximum busy-wait iterations before checking timeout
    pub spin_iterations: u32,
}

impl Default for SqPollConfig {
    fn default() -> Self {
        Self {
            cpu_id: 0,
            idle_timeout_us: 1000,
            spin_iterations: 1000,
        }
    }
}

/// SQPOLL worker state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SqPollState {
    /// Worker is not running
    Stopped = 0,
    /// Worker is actively polling
    Polling = 1,
    /// Worker is idle/sleeping
    Idle = 2,
    /// Worker is shutting down
    Stopping = 3,
}

impl From<u8> for SqPollState {
    fn from(v: u8) -> Self {
        match v {
            0 => Self::Stopped,
            1 => Self::Polling,
            2 => Self::Idle,
            3 => Self::Stopping,
            _ => Self::Stopped,
        }
    }
}

/// Per-ring SQPOLL context
/// 
/// Tracks the state needed for polling a specific io_uring instance
pub struct SqPollRingContext {
    /// Process ID owning this ring
    pub pid: ProcessId,
    
    /// Ring identifier within process
    pub ring_id: u32,
    
    /// SQ tail address (for polling)
    pub sq_tail_addr: u64,
    
    /// Last observed SQ tail value
    pub last_tail: AtomicU32,
    
    /// Submissions processed by this worker
    pub submissions_polled: AtomicU64,
    
    /// Whether this ring is active for polling
    pub active: AtomicBool,
    /// Optional kernel virtual address of the doorbell page for this ring
    pub doorbell_addr: u64,
}

impl SqPollRingContext {
    /// Create a new ring context
    pub fn new(pid: ProcessId, ring_id: u32, sq_tail_addr: u64, doorbell_addr: u64) -> Self {
        Self {
            pid,
            ring_id,
            sq_tail_addr,
            last_tail: AtomicU32::new(0),
            submissions_polled: AtomicU64::new(0),
            active: AtomicBool::new(true),
            doorbell_addr,
        }
    }
    
    /// Check if there are new submissions
    /// 
    /// Returns the number of new submissions since last poll
    pub fn check_new_submissions(&self) -> u32 {
        // Read the current tail from shared memory
        // SAFETY: sq_tail_addr points to valid kernel-mapped memory
        let current_tail = unsafe {
            let tail_ptr = self.sq_tail_addr as *const AtomicU32;
            (*tail_ptr).load(Ordering::Acquire)
        };
        
        let last = self.last_tail.load(Ordering::Relaxed);
        
        if current_tail != last {
            // Calculate new submissions (handle wraparound)
            let new_count = current_tail.wrapping_sub(last);
            new_count
        } else {
            // If there is a doorbell, check if it was rung
            if self.doorbell_addr != 0 {
                // SAFETY: doorbell_addr comes from kernel and points to a valid Doorbell page
                let db_ptr = self.doorbell_addr as *const crate::kernel::io_uring::doorbell::Doorbell;
                let rings = unsafe { (*db_ptr).peek() };
                if rings > 0 { return rings; }
            }
            0
        }
    }
    
    /// Update last observed tail after processing
    pub fn update_tail(&self, new_tail: u32) {
        self.last_tail.store(new_tail, Ordering::Release);
    }
}

/// Global SQPOLL worker state
pub struct SqPollWorker {
    /// Worker state
    state: AtomicU32,
    
    /// Configuration
    config: SqPollConfig,
    
    /// Registered rings for polling
    rings: Mutex<BTreeMap<(ProcessId, u32), Arc<SqPollRingContext>>>,
    
    /// Total polls performed
    poll_count: AtomicU64,
    
    /// Total submissions processed
    submissions_total: AtomicU64,
    
    /// Idle cycles
    idle_cycles: AtomicU64,
    
    /// Wakeup flag (set by syscall to wake idle worker)
    needs_wakeup: AtomicBool,
}

impl SqPollWorker {
    /// Create a new SQPOLL worker
    pub const fn new(config: SqPollConfig) -> Self {
        Self {
            state: AtomicU32::new(SqPollState::Stopped as u32),
            config,
            rings: Mutex::new(BTreeMap::new()),
            poll_count: AtomicU64::new(0),
            submissions_total: AtomicU64::new(0),
            idle_cycles: AtomicU64::new(0),
            needs_wakeup: AtomicBool::new(false),
        }
    }
    
    /// Get current state
    pub fn state(&self) -> SqPollState {
        SqPollState::from(self.state.load(Ordering::Acquire) as u8)
    }
    
    /// Register a ring for polling
    pub fn register_ring(&self, ctx: Arc<SqPollRingContext>) {
        let key = (ctx.pid, ctx.ring_id);
        let doorbell_addr = ctx.doorbell_addr;
        self.rings.lock().insert(key, ctx.clone());
        debug_println!("[SQPOLL] Registered ring {:?} (doorbell={:#x})", key, doorbell_addr);
    }
    
    /// Unregister a ring
    pub fn unregister_ring(&self, pid: ProcessId, ring_id: u32) {
        self.rings.lock().remove(&(pid, ring_id));
        debug_println!("[SQPOLL] Unregistered ring ({:?}, {})", pid, ring_id);
    }
    
    /// Wake up the worker if it's idle
    pub fn wakeup(&self) {
        self.needs_wakeup.store(true, Ordering::Release);
    }
    
    /// Start the polling worker
    /// 
    /// Note: In a full implementation, this would spawn a kernel thread.
    /// For now, we implement cooperative polling that can be called from
    /// the scheduler's idle loop.
    pub fn start(&self) {
        self.state.store(SqPollState::Polling as u32, Ordering::Release);
        debug_println!("[SQPOLL] Worker started");
    }
    
    /// Stop the polling worker
    pub fn stop(&self) {
        self.state.store(SqPollState::Stopping as u32, Ordering::Release);
        debug_println!("[SQPOLL] Worker stopping");
    }
    
    /// Poll all registered rings once
    /// 
    /// Returns the total number of new submissions found
    pub fn poll_once(&self) -> u32 {
        let state = self.state();
        if state != SqPollState::Polling {
            return 0;
        }
        
        self.poll_count.fetch_add(1, Ordering::Relaxed);
        
        let rings = self.rings.lock();
        let mut total_new = 0u32;
        
        for (_key, ctx) in rings.iter() {
            if !ctx.active.load(Ordering::Relaxed) {
                continue;
            }
            
            let new_count = ctx.check_new_submissions();
            if new_count > 0 {
                total_new += new_count;
                ctx.submissions_polled.fetch_add(u64::from(new_count), Ordering::Relaxed);
                // Process the ring using the process table
                use crate::kernel::process::PROCESS_TABLE;
                let mut table = PROCESS_TABLE.lock();
                if let Some(process) = table.get_process_mut(ctx.pid) {
                    if let Some((iog_ctx, cap_table)) = process.io_uring_with_capabilities() {
                        let processed = iog_ctx.process(cap_table);
                        if processed > 0 {
                            // If ring has a doorbell, set CQ ready
                            if ctx.doorbell_addr != 0 {
                                let db_ptr = ctx.doorbell_addr as *mut crate::kernel::io_uring::doorbell::Doorbell;
                                unsafe { (*db_ptr).set_cq_ready(); }
                                debug_println!("[SQPOLL] Set CQ ready for PID={} ring={} (doorbell={:#x})", ctx.pid.as_u64(), ctx.ring_id, ctx.doorbell_addr);
                            }
                        }
                    }
                }
            }
        }
        
        if total_new > 0 {
            self.submissions_total.fetch_add(u64::from(total_new), Ordering::Relaxed);
        } else {
            self.idle_cycles.fetch_add(1, Ordering::Relaxed);
        }
        
        total_new
    }
    
    /// Main polling loop (cooperative version)
    /// 
    /// Call this from the scheduler's idle loop or a timer interrupt
    pub fn poll_loop_iteration(&self) {
        // Check for wakeup request
        if self.needs_wakeup.swap(false, Ordering::AcqRel) {
            // Woken from idle, switch to polling
            if self.state() == SqPollState::Idle {
                self.state.store(SqPollState::Polling as u32, Ordering::Release);
            }
        }
        
        let mut idle_count = 0u32;
        
        for _ in 0..self.config.spin_iterations {
            if self.state() == SqPollState::Stopping {
                self.state.store(SqPollState::Stopped as u32, Ordering::Release);
                return;
            }
            
            let new = self.poll_once();
            if new == 0 {
                idle_count += 1;
            } else {
                idle_count = 0;
            }
            
            // Adaptive polling: go idle after threshold
            if idle_count > self.config.spin_iterations / 10 {
                self.state.store(SqPollState::Idle as u32, Ordering::Release);
                return;
            }
        }
    }
    
    /// Get statistics
    pub fn stats(&self) -> SqPollStats {
        SqPollStats {
            state: self.state(),
            poll_count: self.poll_count.load(Ordering::Relaxed),
            submissions_total: self.submissions_total.load(Ordering::Relaxed),
            idle_cycles: self.idle_cycles.load(Ordering::Relaxed),
            registered_rings: self.rings.lock().len() as u32,
        }
    }
    
    /// Poll with doorbell integration (Phase 2 - Zero-Syscall I/O)
    /// 
    /// This method integrates SQPOLL with the Doorbell mechanism to enable
    /// completely syscall-free I/O. The doorbell allows userspace to notify
    /// the kernel of new submissions without making a syscall.
    /// 
    /// # Workflow
    /// 
    /// 1. Check if doorbell was rung (atomically read and clear)
    /// 2. If rung, wake up from idle and start polling
    /// 3. Set `sqpoll_running` flag to indicate active processing
    /// 4. Poll all registered rings for new submissions
    /// 5. If no submissions found, set `needs_wakeup` flag and go idle
    /// 
    /// # Arguments
    /// 
    /// * `doorbell` - Reference to the doorbell structure
    /// 
    /// # Returns
    /// 
    /// Number of submissions processed
    pub fn poll_with_doorbell(&self, doorbell: &super::doorbell::Doorbell) -> u32 {
        // Debug: print doorbell address being polled
        let addr = doorbell as *const super::doorbell::Doorbell as u64;
        debug_println!("[SQPOLL] poll_with_doorbell called for doorbell addr={:#x}", addr);
        // Check if doorbell was rung
        let rings_count = doorbell.check_and_clear();
        
        // If doorbell was rung, wake up from idle
        if rings_count > 0 {
            let current_state = self.state();
            if current_state == SqPollState::Idle {
                // Transition from idle to polling
                self.state.store(SqPollState::Polling as u32, Ordering::Release);
                debug_println!("[SQPOLL] Woken by doorbell (count={})", rings_count);
            }
            
            // Clear needs_wakeup flag - we're now active
            doorbell.set_needs_wakeup(false);
            doorbell.set_sqpoll_running(true);
        }
        
        // Only poll if we're in polling state
        if self.state() != SqPollState::Polling {
            return 0;
        }
        
        // Poll all registered rings
        let processed = self.poll_once();
        
        if processed > 0 {
            // Got some work, stay in polling state
            doorbell.set_sqpoll_running(true);
            doorbell.set_needs_wakeup(false);
        } else {
            // No work found, go idle
            self.state.store(SqPollState::Idle as u32, Ordering::Release);
            doorbell.set_sqpoll_running(false);
            doorbell.set_needs_wakeup(true);
            debug_println!("[SQPOLL] Going idle (no submissions)");
        }
        
        processed
    }
}

/// SQPOLL statistics
#[derive(Debug, Clone, Copy)]
pub struct SqPollStats {
    /// Current state
    pub state: SqPollState,
    /// Total poll iterations
    pub poll_count: u64,
    /// Total submissions processed
    pub submissions_total: u64,
    /// Idle cycles
    pub idle_cycles: u64,
    /// Number of registered rings
    pub registered_rings: u32,
}

/// Global SQPOLL worker instance
static SQPOLL_WORKER: SqPollWorker = SqPollWorker::new(SqPollConfig {
    cpu_id: 0,
    idle_timeout_us: 1000,
    spin_iterations: 1000,
});

/// Initialize the SQPOLL subsystem
pub fn init() {
    SQPOLL_WORKER.start();
    debug_println!("[SQPOLL] Subsystem initialized");
}

/// Register a ring for SQPOLL
pub fn register_ring(pid: ProcessId, ring_id: u32, sq_tail_addr: u64, doorbell_addr: u64) {
    let ctx = Arc::new(SqPollRingContext::new(pid, ring_id, sq_tail_addr, doorbell_addr));
    SQPOLL_WORKER.register_ring(ctx);
}

/// Unregister a ring from SQPOLL
pub fn unregister_ring(pid: ProcessId, ring_id: u32) {
    SQPOLL_WORKER.unregister_ring(pid, ring_id);
}

/// Wake up the SQPOLL worker
pub fn wakeup() {
    SQPOLL_WORKER.wakeup();
}

/// Poll once (for scheduler integration)
pub fn poll() -> u32 {
    SQPOLL_WORKER.poll_once()
}

/// Get SQPOLL statistics
pub fn stats() -> SqPollStats {
    SQPOLL_WORKER.stats()
}

/// Poll with doorbell integration (Phase 2)
/// 
/// This is the main entry point for doorbell-integrated polling.
/// Call this from the scheduler's idle loop instead of `poll()` when
/// doorbell is available.
/// 
/// # Arguments
/// 
/// * `doorbell` - Reference to the process's doorbell
/// 
/// # Returns
/// 
/// Number of submissions processed
pub fn poll_with_doorbell(doorbell: &super::doorbell::Doorbell) -> u32 {
    SQPOLL_WORKER.poll_with_doorbell(doorbell)
}


#[cfg(test)]
mod tests {
    use super::*;
    
    #[test_case]
    fn test_sqpoll_config_default() {
        let config = SqPollConfig::default();
        assert_eq!(config.idle_timeout_us, 1000);
        assert_eq!(config.spin_iterations, 1000);
    }
    
    #[test_case]
    fn test_sqpoll_state_conversion() {
        assert_eq!(SqPollState::from(0), SqPollState::Stopped);
        assert_eq!(SqPollState::from(1), SqPollState::Polling);
        assert_eq!(SqPollState::from(2), SqPollState::Idle);
        assert_eq!(SqPollState::from(3), SqPollState::Stopping);
        assert_eq!(SqPollState::from(255), SqPollState::Stopped);
    }
}
