use core::arch::asm;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyscallNumber {
    Write = 0,
    Read = 1,
    Exit = 2,
    GetPid = 3,
    Alloc = 4,
    Dealloc = 5,
    Fork = 6,
    Exec = 7,
    Wait = 8,
    Mmap = 9,
    Munmap = 10,
    Pipe = 11,
}

/// Raw system call
/// 
/// Arguments:
/// * `number`: System call number (rax)
/// * `arg1`: Argument 1 (rdi)
/// * `arg2`: Argument 2 (rsi)
/// * `arg3`: Argument 3 (rdx)
/// * `arg4`: Argument 4 (r10)
/// * `arg5`: Argument 5 (r8)
/// * `arg6`: Argument 6 (r9)
#[inline(always)]
pub unsafe fn syscall(
    number: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    arg4: u64,
    arg5: u64,
    arg6: u64,
) -> i64 {
    let result: i64;
    unsafe {
        asm!(
            "syscall",
            in("rax") number,
            in("rdi") arg1,
            in("rsi") arg2,
            in("rdx") arg3,
            in("r10") arg4,
            in("r8") arg5,
            in("r9") arg6,
            lateout("rax") result,
            lateout("rcx") _, // Clobbered by syscall
            lateout("r11") _, // Clobbered by syscall
        );
    }
    result
}

// Wrapper functions for specific system calls

pub fn sys_write(fd: u64, buf: &[u8]) -> i64 {
    unsafe {
        syscall(
            SyscallNumber::Write as u64,
            fd,
            buf.as_ptr() as u64,
            buf.len() as u64,
            0, 0, 0
        )
    }
}

pub fn sys_exit(code: i32) -> ! {
    unsafe {
        syscall(
            SyscallNumber::Exit as u64,
            code as u64,
            0, 0, 0, 0, 0
        );
    }
    loop {}
}

pub fn sys_getpid() -> i64 {
    unsafe {
        syscall(
            SyscallNumber::GetPid as u64,
            0, 0, 0, 0, 0, 0
        )
    }
}

pub fn sys_fork() -> i64 {
    unsafe {
        syscall(
            SyscallNumber::Fork as u64,
            0, 0, 0, 0, 0, 0
        )
    }
}

pub fn sys_exec(path: &str) -> i64 {
    unsafe {
        syscall(
            SyscallNumber::Exec as u64,
            path.as_ptr() as u64,
            path.len() as u64,
            0, 0, 0, 0
        )
    }
}

pub fn sys_wait(pid: i64, status: Option<&mut i32>) -> i64 {
    let status_ptr = status.map(|s| s as *mut i32 as u64).unwrap_or(0);
    unsafe {
        syscall(
            SyscallNumber::Wait as u64,
            pid as u64,
            status_ptr,
            0, 0, 0, 0
        )
    }
}

pub fn sys_mmap(len: usize) -> i64 {
    unsafe {
        syscall(
            SyscallNumber::Mmap as u64,
            0, // addr hint (ignored for now)
use core::arch::asm;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyscallNumber {
    Write = 0,
    Read = 1,
    Exit = 2,
    GetPid = 3,
    Alloc = 4,
    Dealloc = 5,
    Fork = 6,
    Exec = 7,
    Wait = 8,
    Mmap = 9,
    Munmap = 10,
    Pipe = 11,
}

/// Raw system call
/// 
/// Arguments:
/// * `number`: System call number (rax)
/// * `arg1`: Argument 1 (rdi)
/// * `arg2`: Argument 2 (rsi)
/// * `arg3`: Argument 3 (rdx)
/// * `arg4`: Argument 4 (r10)
/// * `arg5`: Argument 5 (r8)
/// * `arg6`: Argument 6 (r9)
#[inline(always)]
pub unsafe fn syscall(
    number: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    arg4: u64,
    arg5: u64,
    arg6: u64,
) -> i64 {
    let result: i64;
    unsafe {
        asm!(
            "syscall",
            in("rax") number,
            in("rdi") arg1,
            in("rsi") arg2,
            in("rdx") arg3,
            in("r10") arg4,
            in("r8") arg5,
            in("r9") arg6,
            lateout("rax") result,
            lateout("rcx") _, // Clobbered by syscall
            lateout("r11") _, // Clobbered by syscall
        );
    }
    result
}

// Wrapper functions for specific system calls

pub fn sys_exit(code: i32) -> ! {
    unsafe {
        syscall(
            SyscallNumber::Exit as u64,
            code as u64,
            0, 0, 0, 0, 0
        );
    }
    loop {}
}

pub fn sys_getpid() -> i64 {
    unsafe {
        syscall(
            SyscallNumber::GetPid as u64,
            0, 0, 0, 0, 0, 0
        )
    }
}

pub fn sys_fork() -> i64 {
    unsafe {
        syscall(
            SyscallNumber::Fork as u64,
            0, 0, 0, 0, 0, 0
        )
    }
}

pub fn sys_exec(path: &str) -> i64 {
    unsafe {
        syscall(
            SyscallNumber::Exec as u64,
            path.as_ptr() as u64,
            path.len() as u64,
            0, 0, 0, 0
        )
    }
}

pub fn sys_wait(pid: i64, status: Option<&mut i32>) -> i64 {
    let status_ptr = status.map(|s| s as *mut i32 as u64).unwrap_or(0);
    unsafe {
        syscall(
            SyscallNumber::Wait as u64,
            pid as u64,
            status_ptr,
            0, 0, 0, 0
        )
    }
}

pub fn sys_mmap(len: usize) -> i64 {
    unsafe {
        syscall(
            SyscallNumber::Mmap as u64,
            0, // addr hint (ignored for now)
            len as u64,
            0, 0, 0, 0
        )
    }
}

pub fn sys_munmap(addr: *mut u8, len: usize) -> isize {
    unsafe {
        syscall(
            SyscallNumber::Munmap as u64,
            addr as u64,
            len as u64,
            0,
            0,
            0,
            0,
        )
    }
}

/// sys_pipe - Create a pipe
///
/// Creates a pipe and returns two file descriptors:
/// - pipefd[0] is the read end
/// - pipefd[1] is the write end
///
/// Returns 0 on success, negative error code on failure
pub fn sys_pipe(pipefd: &mut [u64; 2]) -> isize {
    unsafe {
        syscall(
            SyscallNumber::Pipe as u64,
            pipefd.as_mut_ptr() as u64,
            0,
            0,
            0,
            0,
            0,
        )
    }
}

/// sys_read - Read from file descriptor
///
/// Reads up to `len` bytes from the file descriptor `fd` into `buf`.
/// Returns the number of bytes read, or negative error code.
pub fn sys_read(fd: u64, buf: *mut u8, len: usize) -> isize {
    unsafe {
        syscall(
            SyscallNumber::Read as u64,
            fd,
            buf as u64,
            len as u64,
            0,
            0,
            0,
        )
    }
}

/// sys_write - Write to file descriptor
///
/// Writes up to `len` bytes from `buf` to the file descriptor `fd`.
/// Returns the number of bytes written, or negative error code.
pub fn sys_write_fd(fd: u64, buf: *const u8, len: usize) -> isize {
    unsafe {
        syscall(
            SyscallNumber::Write as u64,
            fd,
            buf as u64,
            len as u64,
            0,
            0,
            0,
        )
    }
}
