// kernel/src/kernel/scheduler/mod.rs
//! Process Scheduler

use crate::kernel::process::{ProcessId, PROCESS_TABLE};
use spin::{Mutex, Lazy};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Timer task spawned flag
static TIMER_TASK_SPAWNED: AtomicBool = AtomicBool::new(false);

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

/// Global round-robin scheduler instance.
pub static SCHEDULER: Lazy<Mutex<RoundRobinScheduler>> = 
    Lazy::new(|| Mutex::new(RoundRobinScheduler::new()));

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
    /// Async task spawned flag
    async_task_spawned: AtomicBool,
}

impl SqpollConfig {
    /// Create a new SQPOLL configuration (disabled by default)
    pub const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            poll_count: AtomicU64::new(0),
            ops_processed: AtomicU64::new(0),
            async_task_spawned: AtomicBool::new(false),
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
    
    /// Check if async task has been spawned
    pub fn is_async_task_spawned(&self) -> bool {
        self.async_task_spawned.load(Ordering::Acquire)
    }
    
    /// Mark async task as spawned
    pub fn set_async_task_spawned(&self) {
        self.async_task_spawned.store(true, Ordering::Release);
    }
}

/// Global SQPOLL configuration
pub static SQPOLL: Lazy<SqpollConfig> = Lazy::new(SqpollConfig::new);

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
    
    // Also poll doorbells for ring-based SQPOLL
    // Iterate over processes and call the doorbell-aware poll for any process
    // which has a doorbell pointer allocated. This integrates the new
    // io_uring::sqpoll implementation while keeping the legacy poller.
    let mut processed_total = processed;
    {
        use crate::kernel::process::PROCESS_TABLE;
        use crate::kernel::io_uring::doorbell::Doorbell;
        let table = PROCESS_TABLE.lock();
        for p in table.ready_processes() {
                if let Some(kptr) = p.ring_doorbell_kern_ptr() {
                // SAFETY: kptr is a kernel virtual pointer to a Doorbell page
                let db_ptr = kptr as *const Doorbell;
                // Print debug info for tracing
                    crate::debug_println!("[SQPOLL] Polling process PID={} doorbell at {:#x}", p.pid().as_u64(), kptr);
                let processed = crate::kernel::io_uring::sqpoll::poll_with_doorbell(unsafe { &*db_ptr });
                processed_total += processed as u64;
            }
                // Also poll legacy syscall_ring contexts for this process (if any)
                // This keeps legacy flow working alongside new doorbell flow.
        }
    }

    // Record statistics
    SQPOLL.record_poll(processed_total);
    
    processed_total
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

/// Main kernel idle entry point with SQPOLL and async runtime
///
/// Call this instead of `hlt` in the main kernel loop.
/// This function integrates:
/// 1. Timer task (spawned on first call)
/// 2. Async runtime polling (kernel tasks)
/// 3. SQPOLL for io_uring-style I/O
/// 4. CPU halt when no work is available
pub fn kernel_idle() {
    // Spawn timer_task on first call (ensures timers work correctly)
    if !TIMER_TASK_SPAWNED.swap(true, Ordering::AcqRel) {
        crate::kernel::r#async::spawn_task(crate::kernel::r#async::timer_task());
        crate::debug_println!("[Scheduler] Timer task spawned");
    }
    
    // First, poll the async runtime for pending kernel tasks
    let async_work = crate::kernel::r#async::poll_runtime().is_some();
    
    // Then, try SQPOLL for user-space ring buffers
    let sqpoll_work = if SQPOLL.is_enabled() {
        idle_with_sqpoll(10)
    } else {
        false
    };
    
    // If any work was done, return immediately to allow rescheduling
    if async_work || sqpoll_work {
        return;
    }
    
    // No work - halt until next interrupt
    x86_64::instructions::hlt();
}

/// Run async runtime until idle
///
/// Useful for batch processing of kernel tasks.
/// Returns the number of tasks that completed.
pub fn run_async_tasks() -> usize {
    crate::kernel::r#async::run_runtime_until_idle()
}

// ============================================================================
// Async SQPOLL Task
// ============================================================================

/// SQPOLL を async タスクとして実行
///
/// この関数は無限ループで SQPOLL を実行し、io_uring 操作を処理します。
/// Timer を使って定期的にポーリングします。
pub async fn sqpoll_async_task() {
    use crate::kernel::r#async::{Timer, yield_now};
    
    crate::debug_println!("[SQPOLL] Async task started");
    
    loop {
        // SQPOLL が無効になったらタスク終了
        if !SQPOLL.is_enabled() {
            crate::debug_println!("[SQPOLL] Async task stopping (disabled)");
            break;
        }
        
        // ポーリング実行
        let processed = sqpoll_tick();
        
        if processed > 0 {
            // 処理があった場合は即座に次のポーリング
            yield_now().await;
        } else {
            // 処理がなかった場合は少し待ってからポーリング
            // 10ms (1 tick) 待機
            Timer::after(10).await;
        }
    }
}

/// SQPOLL async タスクを開始
///
/// SQPOLL を有効化し、async executor 上でポーリングタスクをスポーン
pub fn start_sqpoll_async() {
    if SQPOLL.is_async_task_spawned() {
        crate::debug_println!("[SQPOLL] Async task already running");
        return;
    }
    
    SQPOLL.enable();
    SQPOLL.set_async_task_spawned();
    
    crate::kernel::r#async::spawn_task(sqpoll_async_task());
    crate::debug_println!("[SQPOLL] Async task spawned");
}

/// SQPOLL を停止
pub fn stop_sqpoll_async() {
    SQPOLL.disable();
    // タスクは次のポーリング時に自動的に終了
}

// ============================================================================
// Async I/O Helper Functions
// ============================================================================

/// 非同期で io_uring NOP を実行
pub async fn async_nop() -> i32 {
    use crate::kernel::r#async::io_uring_future::{submit_async, IoUringOp};
    submit_async(IoUringOp::Nop).await
}

/// 非同期でデータを書き込む
pub async fn async_write(fd: i32, data: &[u8]) -> i32 {
    use crate::kernel::r#async::io_uring_future::write_async;
    write_async(fd, data).await
}

/// 非同期でデータを読み込む
pub async fn async_read(fd: i32, buf: &mut [u8]) -> i32 {
    use crate::kernel::r#async::io_uring_future::read_async;
    read_async(fd, buf).await
}
