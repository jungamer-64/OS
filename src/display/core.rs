use crate::serial_print;
use crate::vga_buffer::ColorCode;
use core::cmp;
use core::fmt::{self, Write};

/// Text output target abstraction.
pub trait Output {
    fn write(&mut self, text: &str, color: ColorCode);
}

/// Hardware-backed dual output (VGA + serial).
pub struct HardwareOutput;

impl Output for HardwareOutput {
    fn write(&mut self, text: &str, color: ColorCode) {
        crate::vga_buffer::print_colored(text, color);
        serial_print!("{}", text);
    }
}

pub(crate) fn hardware_output() -> HardwareOutput {
    HardwareOutput
}

struct StackString {
    buf: [u8; 512],
    len: usize,
}

impl StackString {
    const fn new() -> Self {
        Self {
            buf: [0u8; 512],
            len: 0,
        }
    }

    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buf[..self.len]).unwrap_or("<fmt error>")
    }
}

impl Write for StackString {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let bytes = s.as_bytes();
        let space = self.buf.len().saturating_sub(self.len);
        let to_copy = cmp::min(space, bytes.len());
        self.buf[self.len..self.len + to_copy].copy_from_slice(&bytes[..to_copy]);
        self.len += to_copy;
        Ok(())
    }
}

#[allow(dead_code)]
pub fn broadcast(message: &str, color: ColorCode) {
    let mut out = hardware_output();
    broadcast_with(&mut out, message, color);
}

pub fn broadcast_with<O: Output>(out: &mut O, message: &str, color: ColorCode) {
    broadcast_args_with(out, format_args!("{}", message), color);
}

#[allow(dead_code)]
pub fn broadcast_args(args: fmt::Arguments, color: ColorCode) {
    let mut out = hardware_output();
    broadcast_args_with(&mut out, args, color);
}

pub fn broadcast_args_with<O: Output>(out: &mut O, args: fmt::Arguments, color: ColorCode) {
    let mut buf = StackString::new();
    let _ = buf.write_fmt(args);
    out.write(buf.as_str(), color);
}
