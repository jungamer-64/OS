// crates/kernel/src/kernel/async/io_uring_future.rs
//! io_uring と async executor の統合
//!
//! io_uring 操作を Future として抽象化し、async/await で使用可能にします。
//!
//! # アーキテクチャ
//!
//! ```text
//! User/Kernel Code                  Async Runtime
//! ┌─────────────────┐              ┌─────────────────┐
//! │ IoUringFuture   │              │    Executor     │
//! │   .await        │──────poll───►│                 │
//! └─────────────────┘              │  ┌───────────┐  │
//!         ▲                        │  │ TaskQueue │  │
//!         │                        │  └───────────┘  │
//!         │ wake                   └─────────────────┘
//!         │                                ▲
//! ┌───────┴───────┐                        │
//! │ IoUringWaker  │                        │
//! │  (completes)  │────────────────────────┘
//! └───────────────┘
//! ```
//!
//! # 使用例
//!
//! ```ignore
//! use crate::kernel::r#async::io_uring_future::{IoUringOp, submit_async};
//!
//! // 非同期 write
//! let written = submit_async(IoUringOp::Write {
//!     fd: 1,
//!     buf: data.as_ptr(),
//!     len: data.len() as u32,
//! }).await;
//! ```

use core::future::Future;
use core::pin::Pin;
use core::sync::atomic::{AtomicU64, AtomicI32, AtomicBool, Ordering};
use core::task::{Context, Poll, Waker};
use alloc::sync::Arc;
use alloc::collections::BTreeMap;
use spin::Mutex;

use crate::debug_println;

// ============================================================================
// Operation Types
// ============================================================================

use crate::abi::io_uring_v2::{SubmissionEntryV2, CompletionEntryV2};
use crate::kernel::capability::{Handle, FileResource};

// ... (imports)

// ============================================================================
// Operation Types
// ============================================================================

/// io_uring 操作の種類
#[derive(Debug, Clone)]
pub enum IoUringOp {
    /// No operation (テスト用)
    Nop,
    
    /// Read from file descriptor
    Read {
        fd: i32,
        buf: *mut u8,
        len: u32,
    },
    
    /// Write to file descriptor
    Write {
        fd: i32,
        buf: *const u8,
        len: u32,
    },
    
    /// Close file descriptor
    Close {
        fd: i32,
    },
    
    /// Memory map allocation
    Mmap {
        hint: u64,
        len: u64,
    },
    
    /// Memory unmap
    Munmap {
        addr: u64,
        len: u64,
    },

    /// Read from capability (V2)
    ReadV2 {
        capability_id: u64,
        buf: *mut u8,
        len: u32,
        offset: u64,
    },
    
    /// Write to capability (V2)
    WriteV2 {
        capability_id: u64,
        buf: *const u8,
        len: u32,
        offset: u64,
    },
    
    /// Close capability (V2)
    CloseV2 {
        capability_id: u64,
    },
}

// SAFETY: IoUringOp contains raw pointers but they are only used within
// the kernel's address space and are not dereferenced in async context.
unsafe impl Send for IoUringOp {}
unsafe impl Sync for IoUringOp {}

// ============================================================================
// Pending Operations Registry
// ============================================================================

/// ペンディング操作の状態
struct PendingOperation {
    /// 完了したかどうか
    completed: AtomicBool,
    /// 結果 (完了後に設定)
    result: AtomicI32,
    /// 待機中の Waker
    waker: Mutex<Option<Waker>>,
}

impl PendingOperation {
    fn new() -> Self {
        Self {
            completed: AtomicBool::new(false),
            result: AtomicI32::new(0),
            waker: Mutex::new(None),
        }
    }
    
    fn complete(&self, result: i32) {
        self.result.store(result, Ordering::Release);
        self.completed.store(true, Ordering::Release);
        
        // Wake the waiting task
        if let Some(waker) = self.waker.lock().take() {
            waker.wake();
        }
    }
    
    fn is_completed(&self) -> bool {
        self.completed.load(Ordering::Acquire)
    }
    
    fn get_result(&self) -> i32 {
        self.result.load(Ordering::Acquire)
    }
    
    fn register_waker(&self, waker: &Waker) {
        let mut guard = self.waker.lock();
        *guard = Some(waker.clone());
    }
}

/// 次の user_data 値
static NEXT_USER_DATA: AtomicU64 = AtomicU64::new(1);

/// ペンディング操作のレジストリ
static PENDING_OPS: Mutex<BTreeMap<u64, Arc<PendingOperation>>> = Mutex::new(BTreeMap::new());

/// 新しい user_data を割り当て、操作を登録
fn register_operation() -> (u64, Arc<PendingOperation>) {
    let user_data = NEXT_USER_DATA.fetch_add(1, Ordering::Relaxed);
    let op = Arc::new(PendingOperation::new());
    
    PENDING_OPS.lock().insert(user_data, Arc::clone(&op));
    
    (user_data, op)
}

/// 操作を完了としてマーク
pub fn complete_operation(user_data: u64, result: i32) {
    if let Some(op) = PENDING_OPS.lock().remove(&user_data) {
        debug_println!(
            "[io_uring_future] Completing operation user_data={} result={}",
            user_data, result
        );
        op.complete(result);
    }
}

/// 操作をキャンセル
#[allow(dead_code)]
pub fn cancel_operation(user_data: u64) {
    if let Some(op) = PENDING_OPS.lock().remove(&user_data) {
        op.complete(-125); // ECANCELED
    }
}

// ============================================================================
// IoUringFuture
// ============================================================================

/// io_uring 操作を表す Future
pub struct IoUringFuture {
    /// この操作の user_data
    user_data: u64,
    /// 操作の状態へのハンドル
    operation: Arc<PendingOperation>,
    /// 操作がサブミットされたか
    submitted: bool,
    /// 操作の種類
    op: IoUringOp,
}

impl IoUringFuture {
    /// 新しい IoUringFuture を作成
    pub fn new(op: IoUringOp) -> Self {
        let (user_data, operation) = register_operation();
        Self {
            user_data,
            operation,
            submitted: false,
            op,
        }
    }
}

impl Future for IoUringFuture {
    type Output = i32;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // 初回ポーリング時に操作をサブミット
        if !self.submitted {
            self.submitted = true;
            
            // 操作を io_uring にサブミット
            let result = submit_op_to_ring(&self.op, self.user_data);
            
            if result < 0 {
                // サブミット失敗
                PENDING_OPS.lock().remove(&self.user_data);
                return Poll::Ready(result);
            }
            
            debug_println!(
                "[io_uring_future] Submitted operation user_data={}",
                self.user_data
            );
        }
        
        // 完了をチェック
        if self.operation.is_completed() {
            let result = self.operation.get_result();
            // レジストリからは complete_operation で既に削除済み
            Poll::Ready(result)
        } else {
            // Waker を登録して Pending
            self.operation.register_waker(cx.waker());
            Poll::Pending
        }
    }
}

impl Drop for IoUringFuture {
    fn drop(&mut self) {
        // 完了していない場合はレジストリから削除
        if !self.operation.is_completed() {
            PENDING_OPS.lock().remove(&self.user_data);
        }
    }
}

// ============================================================================
// Submit Operations
// ============================================================================

/// 操作を実際の io_uring にサブミット
fn submit_op_to_ring(op: &IoUringOp, user_data: u64) -> i32 {
    use crate::kernel::io_uring::handlers_v2::dispatch_sqe_v2;
    use crate::kernel::process::PROCESS_TABLE;
    
    // Get the current process's capability table
    let table = PROCESS_TABLE.lock();
    let process = match table.current_process() {
        Some(p) => p,
        None => {
            // No current process, return error
            complete_operation(user_data, -3); // ESRCH
            return -3;
        }
    };
    let cap_table = process.capability_table();

    // Handle V2 operations
    match op {
        IoUringOp::ReadV2 { capability_id, buf, len, offset } => {
            let sqe = SubmissionEntryV2::read_raw(*capability_id, *buf as u64, *len, *offset, user_data);
            let cqe = dispatch_sqe_v2(&sqe, cap_table, None, true); // Allow raw addr for kernel
            let result = match cqe.into_result() {
                Ok(val) => val,
                Err(e) => -(e as i32),
            };
            complete_operation(user_data, result);
            return 0;
        }
        IoUringOp::WriteV2 { capability_id, buf, len, offset } => {
            let sqe = SubmissionEntryV2::write_raw(*capability_id, *buf as u64, *len, *offset, user_data);
            let cqe = dispatch_sqe_v2(&sqe, cap_table, None, true); // Allow raw addr for kernel
            let result = match cqe.into_result() {
                Ok(val) => val,
                Err(e) => -(e as i32),
            };
            complete_operation(user_data, result);
            return 0;
        }
        IoUringOp::CloseV2 { capability_id } => {
            let sqe = SubmissionEntryV2::close(*capability_id, user_data);
            let cqe = dispatch_sqe_v2(&sqe, cap_table, None, true);
            let result = match cqe.into_result() {
                Ok(val) => val,
                Err(e) => -(e as i32),
            };
            complete_operation(user_data, result);
            return 0;
        }
        _ => {
            // Unsupported V1 operation - shouldn't happen
            debug_println!("[IO_URING] Unsupported V1 operation: {:?}", op);
            complete_operation(user_data, -38); // ENOSYS
            return -38;
        }
    }
}

// ============================================================================
// Public API
// ============================================================================

/// 非同期 io_uring 操作をサブミット
///
/// # Example
///
/// ```ignore
/// let result = submit_async(IoUringOp::Nop).await;
/// assert_eq!(result, 0);
/// ```
pub fn submit_async(op: IoUringOp) -> IoUringFuture {
    IoUringFuture::new(op)
}

/// 非同期 write
pub fn write_async(fd: i32, buf: &[u8]) -> IoUringFuture {
    submit_async(IoUringOp::Write {
        fd,
        buf: buf.as_ptr(),
        len: buf.len() as u32,
    })
}

/// 非同期 read
///
/// # Safety
/// The buffer must remain valid until the Future completes.
pub fn read_async(fd: i32, buf: &mut [u8]) -> IoUringFuture {
    submit_async(IoUringOp::Read {
        fd,
        buf: buf.as_mut_ptr(),
        len: buf.len() as u32,
    })
}

/// 非同期 close
pub fn close_async(fd: i32) -> IoUringFuture {
    submit_async(IoUringOp::Close { fd })
}

/// 非同期 write (V2 Capability-based)
///
/// # Ownership
/// `cap` is borrowed. The caller retains ownership.
pub fn write_async_v2(cap: &Handle<FileResource>, buf: &[u8], offset: u64) -> IoUringFuture {
    submit_async(IoUringOp::WriteV2 {
        capability_id: cap.raw(),
        buf: buf.as_ptr(),
        len: buf.len() as u32,
        offset,
    })
}

/// 非同期 read (V2 Capability-based)
///
/// # Safety
/// The buffer must remain valid until the Future completes.
/// 
/// # Ownership
/// `cap` is borrowed. The caller retains ownership.
pub fn read_async_v2(cap: &Handle<FileResource>, buf: &mut [u8], offset: u64) -> IoUringFuture {
    submit_async(IoUringOp::ReadV2 {
        capability_id: cap.raw(),
        buf: buf.as_mut_ptr(),
        len: buf.len() as u32,
        offset,
    })
}

/// 非同期 close (V2 Capability-based)
///
/// # Ownership
/// `cap` is moved. The resource will be closed.
pub fn close_async_v2(cap: Handle<FileResource>) -> IoUringFuture {
    let capability_id = cap.into_raw();
    submit_async(IoUringOp::CloseV2 { capability_id })
}

/// 非同期メモリ割り当て
pub fn mmap_async(len: u64) -> IoUringFuture {
    submit_async(IoUringOp::Mmap { hint: 0, len })
}

/// 非同期メモリ解放
pub fn munmap_async(addr: u64, len: u64) -> IoUringFuture {
    submit_async(IoUringOp::Munmap { addr, len })
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_register_operation() {
        let (user_data1, _) = register_operation();
        let (user_data2, _) = register_operation();
        assert!(user_data2 > user_data1);
    }
}
