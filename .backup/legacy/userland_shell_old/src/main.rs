#![no_std]
#![no_main]

mod syscall;
mod console;

use core::panic::PanicInfo;

#[no_mangle]
#[link_section = ".text.entry"]
pub extern "C" fn _start() -> ! {
    uprintln!("Hello from Userland Shell!");
    uprintln!("=== Testing IPC Pipes ===\n");
    
    // Create a pipe
    let mut pipefd = [0u64; 2];
    let result = syscall::sys_pipe(&mut pipefd);
    
    if result < 0 {
        uprintln!("Failed to create pipe: {}", result);
        syscall::sys_exit(1);
    }
    
    uprintln!("Pipe created: read_fd={}, write_fd={}", pipefd[0], pipefd[1]);
    
    // Fork
    let pid = syscall::sys_fork();
    
    if pid == 0 {
        // Child process: write to pipe
        uprintln!("[Child] Writing to pipe...");
        
        let message = b"Hello from child process!";
        let written = syscall::sys_write_fd(pipefd[1], message.as_ptr(), message.len());
        
        if written >= 0 {
            uprintln!("[Child] Wrote {} bytes", written);
        } else {
            uprintln!("[Child] Write failed: {}", written);
        }
        
        uprintln!("[Child] Exiting");
        syscall::sys_exit(0);
    } else if pid > 0 {
        // Parent process: read from pipe
        uprintln!("[Parent] Child PID: {}", pid);
        uprintln!("[Parent] Reading from pipe...");
        
        let mut buffer = [0u8; 64];
        let read = syscall::sys_read(pipefd[0], buffer.as_mut_ptr(), buffer.len());
        
        if read > 0 {
            uprintln!("[Parent] Read {} bytes: {:?}", read, &buffer[..read as usize]);
            
            // Print as string
            if let Ok(s) = core::str::from_utf8(&buffer[..read as usize]) {
                uprintln!("[Parent] Message: '{}'", s);
            }
        } else {
            uprintln!("[Parent] Read failed: {}", read);
        }
        
        // Wait for child
        syscall::sys_wait(pid, None);
        uprintln!("[Parent] Child terminated");
        uprintln!("\n=== Pipe Test Complete ===");
        syscall::sys_exit(0);
    } else {
        uprintln!("Fork failed!");
        syscall::sys_exit(1);
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    uprintln!("Shell Panic: {}", info);
    syscall::sys_exit(1);
}
