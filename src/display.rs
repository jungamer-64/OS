// src/display.rs

//! Display and output formatting facade.

pub mod color;
mod backend;
mod boot;
mod output;
pub use output::{
    broadcast, broadcast_args, broadcast_args_with, broadcast_with, hardware_output, HardwareOutput, Output,
};
mod shell;
mod panic;
pub mod keyboard;

#[cfg(test)]
mod tests;

pub use color::{Color, ColorCode};
pub use backend::{
    default_display_backend, DefaultDisplayBackend, DisplayError, DisplayHardware, StubDisplay,
    VgaDisplay,
};
pub use boot::{
    display_boot_environment, display_boot_environment_with, display_boot_information,
    display_boot_information_with, display_feature_list, display_feature_list_with,
    display_usage_note, display_usage_note_with,
};
pub use shell::{show_wait_shell, run_shell};
pub use panic::{display_panic_info_serial, display_panic_info_vga};

use core::fmt;
use crate::serial;

/// Prints formatted text to the configured output writers.
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::display::print_impl(format_args!($($arg)*)));
}

/// Prints formatted text to the configured output writers, with a newline.
#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($fmt:expr) => ($crate::print!(concat!($fmt, "\n")));
    ($fmt:expr, $($arg:tt)*) => ($crate::print!(concat!($fmt, "\n"), $($arg)*));
}

/// Hidden function that the macros call.
#[doc(hidden)]
pub fn print_impl(args: fmt::Arguments) {
    use core::fmt::Write;
    
    // 1. Write to Display (VGA or Stub)
    // We use a helper to adapt DisplayHardware to fmt::Write
    struct DisplayWriter<T>(T);
    
    impl<T: DisplayHardware> fmt::Write for DisplayWriter<T> {
        fn write_str(&mut self, s: &str) -> fmt::Result {
            use crate::vga_buffer::ColorCode;
            // Use normal color (LightGray on Black)
            let color = ColorCode::normal();
            self.0.write_colored(s, color).map_err(|_| fmt::Error)
        }
    }

    let display = default_display_backend();
    if display.is_available() {
        let mut writer = DisplayWriter(display);
        let _ = writer.write_fmt(args);
    }

    // 2. Write to Serial
    // SerialPorts implements fmt::Write directly
    serial::with_serial_ports(|ports| ports.write_fmt(args)).unwrap();
}

pub fn clear_screen() {
    #[cfg(target_arch = "x86_64")]
    crate::vga_buffer::clear().ok();
}

pub fn get_writer() -> impl Output {
    output::hardware_output()
}
