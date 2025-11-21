// src/arch/x86_64/qemu.rs

use x86_64::instructions::port::Port;

/// Write the exit code to QEMU's debug exit port.
pub fn exit_qemu(code: u32) {
    unsafe {
        let mut port = Port::<u32>::new(0xF4);
        port.write(code);
    }
}
