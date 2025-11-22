// src/display/output.rs

use core::fmt;
use crate::vga_buffer::ColorCode;
use crate::display::{default_display_backend, DisplayHardware};
use crate::serial;

/// Trait for outputting formatted text with color.
pub trait Output {
    fn write_fmt(&mut self, args: fmt::Arguments, color: ColorCode);
}

/// Output implementation that writes to hardware (Display + Serial).
pub struct HardwareOutput {
    display: crate::display::DefaultDisplayBackend,
}

impl HardwareOutput {
    pub fn new() -> Self {
        Self {
            display: default_display_backend(),
        }
    }
}

impl Output for HardwareOutput {
    fn write_fmt(&mut self, args: fmt::Arguments, color: ColorCode) {
        // Write to display
        if self.display.is_available() {
            struct ColorWriter<'a, D: DisplayHardware> {
                display: &'a mut D,
                color: ColorCode,
            }

            impl<'a, D: DisplayHardware> fmt::Write for ColorWriter<'a, D> {
                fn write_str(&mut self, s: &str) -> fmt::Result {
                    self.display.write_colored(s, self.color).map_err(|_| fmt::Error)
                }
            }

            let mut writer = ColorWriter {
                display: &mut self.display,
                color,
            };
            use core::fmt::Write;
            let _ = writer.write_fmt(args);
        }

        // Write to serial (ignore color)
        let _ = serial::with_serial_ports(|ports| {
            use core::fmt::Write;
            ports.write_fmt(args)
        });
    }
}

pub fn hardware_output() -> HardwareOutput {
    HardwareOutput::new()
}

pub fn broadcast_args_with<O: Output + ?Sized>(out: &mut O, args: fmt::Arguments, color: ColorCode) {
    out.write_fmt(args, color);
}

pub fn broadcast_with<O: Output + ?Sized>(out: &mut O, text: &str, color: ColorCode) {
    out.write_fmt(format_args!("{}", text), color);
}

pub fn broadcast_args(args: fmt::Arguments) {
    let mut out = hardware_output();
    broadcast_args_with(&mut out, args, ColorCode::normal());
}

pub fn broadcast(text: &str) {
    let mut out = hardware_output();
    broadcast_with(&mut out, text, ColorCode::normal());
}
