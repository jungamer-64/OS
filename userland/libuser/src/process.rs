//! Process management API

use crate::syscall;

/// Exit the current process with the given exit code
pub fn exit(code: i32) -> ! {
    syscall::exit(code)
}

/// Get the current process ID
pub fn getpid() -> i64 {
    syscall::getpid()
}

/// Fork the current process
///
/// Returns:
/// - 0 in the child process
/// - Child PID in the parent process
/// - Negative error code on failure
pub fn fork() -> i64 {
    syscall::fork()
}

/// Execute a program
pub fn exec(path: &str) -> i64 {
    syscall::exec(path)
}

/// Wait for a child process to terminate
///
/// Returns the PID of the terminated child, or negative error code
pub fn wait(pid: i64, status: Option<&mut i32>) -> i64 {
    syscall::wait(pid, status)
}

/// Spawn a new process (fork + exec pattern)
pub fn spawn(path: &str) -> Result<i64, i64> {
    let pid = fork();
    if pid == 0 {
        // Child process
        exec(path);
        // If exec returns, it failed
        exit(1);
    } else if pid > 0 {
        // Parent process
        Ok(pid)
    } else {
        // Error
        Err(pid)
    }
}
