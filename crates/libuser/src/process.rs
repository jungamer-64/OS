//! Process management API

use crate::syscall::{self, SyscallResult};
use crate::abi::error::SyscallError;

/// Exit the current process with the given exit code
///
/// This function never returns.
pub fn exit(code: i32) -> ! {
    syscall::exit(code)
}

/// Get the current process ID
///
/// # Returns
/// Current process ID (always succeeds)
pub fn getpid() -> u64 {
    syscall::getpid()
}



/// Wait for a child process to terminate
///
/// # Arguments
/// * `pid` - Process ID to wait for (currently ignored, waits for any child)
/// * `status` - Optional pointer to store exit code
///
/// # Returns
/// PID of terminated child
///
/// # Errors
/// * `ECHILD` - No child processes
/// * `ESRCH` - Process not found
/// * `EFAULT` - Invalid status pointer
///
/// # Examples
/// ```no_run
/// use libuser::process::{fork, wait};
///
/// match fork() {
///     Ok(0) => {
///         // Child: do some work
///         libuser::process::exit(0);
///     }
///     Ok(child_pid) => {
///         // Parent: wait for child
///         let mut status = 0i32;
///         match wait(-1, Some(&mut status)) {
///             Ok(pid) => println!("Child {} exited with status {}", pid, status),
///             Err(e) => println!("Wait failed: {}", e.description()),
///         }
///     }
///     Err(_) => {}
/// }
/// ```
pub fn wait(pid: i64, status: Option<&mut i32>) -> SyscallResult<u64> {
    syscall::wait(pid, status)
}

/// Spawn a new process
///
/// This creates a new process directly (replacing fork+exec).
///
/// # Arguments
/// * `path` - Path to program to execute
/// * `args` - Command line arguments
///
/// # Returns
/// Child PID
///
/// # Errors
/// * `ENOENT` - File not found
/// * `ENOMEM` - Out of memory
///
/// # Examples
/// ```no_run
/// use libuser::process::spawn;
///
/// match spawn("/bin/shell", &["arg1", "arg2"]) {
///     Ok(pid) => println!("Spawned child {}", pid),
///     Err(e) => println!("Spawn failed: {}", e.description()),
/// }
/// ```
pub fn spawn(path: &str, args: &[&str]) -> SyscallResult<u64> {
    syscall::spawn(path, args)
}
