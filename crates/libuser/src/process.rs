//! Process management API

use crate::syscall::{self, SyscallResult, SyscallError};

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

/// Fork the current process
///
/// # Returns
/// * `Ok(0)` - In child process
/// * `Ok(child_pid)` - In parent process
/// * `Err(error)` - On failure
///
/// # Errors
/// * `ENOMEM` - Out of memory
///
/// # Examples
/// ```no_run
/// use libuser::process::fork;
///
/// match fork() {
///     Ok(0) => {
///         // Child process
///         println!("I am the child");
///     }
///     Ok(pid) => {
///         // Parent process
///         println!("Child PID: {}", pid);
///     }
///     Err(e) => {
///         println!("Fork failed: {}", e.description());
///     }
/// }
/// ```
pub fn fork() -> SyscallResult<u64> {
    syscall::fork()
}

/// Execute a program
///
/// This function only returns on error. On success, the current process
/// image is replaced with the new program.
///
/// # Arguments
/// * `path` - Path to program (currently ignored in Phase 1)
///
/// # Returns
/// Only returns on error
///
/// # Errors
/// * `ENOMEM` - Out of memory
pub fn exec(path: &str) -> SyscallError {
    syscall::exec(path)
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

/// Spawn a new process (fork + exec pattern)
///
/// This is a convenience function that combines fork and exec.
///
/// # Arguments
/// * `path` - Path to program to execute
///
/// # Returns
/// Child PID in parent process
///
/// # Errors
/// * Fork or exec errors
///
/// # Examples
/// ```no_run
/// use libuser::process::spawn;
///
/// match spawn("/bin/shell") {
///     Ok(pid) => println!("Spawned child {}", pid),
///     Err(e) => println!("Spawn failed: {}", e.description()),
/// }
/// ```
pub fn spawn(path: &str) -> SyscallResult<u64> {
    match fork()? {
        0 => {
            // Child process
            let _err = exec(path);
            // If exec returns, it failed
            crate::io::println("exec failed");
            exit(1);
        }
        pid => {
            // Parent process
            Ok(pid)
        }
    }
}
