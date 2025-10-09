use crate::vga_buffer::ColorCode;
use crate::{serial_print, serial_println};
use core::panic::PanicInfo;

const PANIC_SEPARATOR: &str = "========================================\n";

pub fn display_panic_info_serial(info: &PanicInfo) {
    if !crate::serial::is_available() {
        return;
    }

    serial_println!("");
    serial_separator();
    serial_println!("       !!! KERNEL PANIC !!!");
    serial_separator();

    serial_println!("Message: {}", info.message());

    if let Some(location) = info.location() {
        serial_println!(
            "Location: {}:{}:{}",
            location.file(),
            location.line(),
            location.column()
        );
    }

    serial_separator();
    serial_println!("System halted. CPU in hlt loop.");
}

pub fn display_panic_info_vga(info: &PanicInfo) {
    crate::vga_buffer::print_colored("\n!!! KERNEL PANIC !!!\n\n", ColorCode::panic());

    if let Some(location) = info.location() {
        crate::vga_buffer::print_colored("File: ", ColorCode::error());
        crate::vga_buffer::print_colored(location.file(), ColorCode::normal());
        crate::vga_buffer::print_colored("\n", ColorCode::normal());

        crate::vga_buffer::print_colored("Line: ", ColorCode::error());
        crate::vga_buffer::print_colored("(see serial output)\n", ColorCode::normal());

        crate::vga_buffer::print_colored("Column: ", ColorCode::error());
        crate::vga_buffer::print_colored("(see serial output)\n", ColorCode::normal());
    }

    crate::vga_buffer::print_colored(
        "\nSystem halted. See serial for more details.\n",
        ColorCode::warning(),
    );
}

fn serial_separator() {
    serial_print!("{}", PANIC_SEPARATOR);
}
