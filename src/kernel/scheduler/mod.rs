//! Process Scheduler

use crate::kernel::process::{ProcessId, PROCESS_TABLE};
use spin::Mutex;
use alloc::vec::Vec;
use lazy_static::lazy_static;

/// Simple round-robin scheduler
pub struct RoundRobinScheduler {
    current_pid: Option<ProcessId>,
}

impl RoundRobinScheduler {
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
    pub static ref SCHEDULER: Mutex<RoundRobinScheduler> = 
        Mutex::new(RoundRobinScheduler::new());
}
