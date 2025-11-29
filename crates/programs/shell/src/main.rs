//! Shell Program

#![no_std]
#![no_main]

use libuser::io::println;
use libuser::process::exit;
use core::panic::PanicInfo;

#[no_mangle]
#[link_section = ".text.entry"]
pub extern "C" fn _start(argc: u64, argv: *const *const u8) -> ! {
    use libuser::process::spawn;
    
    println("Tiny OS Shell");
    
    // Check arguments
    if argc > 1 {
        // We are the child
        println("I am the child shell!");
        
        // Print arguments
        unsafe {
            let args_slice = core::slice::from_raw_parts(argv, argc as usize);
            for (i, &arg_ptr) in args_slice.iter().enumerate() {
                // Determine length of string
                let mut len = 0;
                while *arg_ptr.add(len) != 0 {
                    len += 1;
                }
                let s_slice = core::slice::from_raw_parts(arg_ptr, len);
                if let Ok(s) = core::str::from_utf8(s_slice) {
                    println("Arg {}: {}", i, s);
                }
            }
        }
        
        exit(0);
    }
    
    println("I am the parent shell. Spawning child...");
    
    match spawn("shell", &["child_arg"]) {
        Ok(pid) => {
            println("Spawned child with PID {}", pid);
            // Wait for child (TODO: Implement wait properly)
            // For now just exit
        },
        Err(_) => println("Failed to spawn child"),
    }
    
    exit(0);
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println("Shell Panic: {}", info);
    exit(1);
}