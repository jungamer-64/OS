use core::fmt;
use super::syscall::sys_write;

pub struct Console;

impl fmt::Write for Console {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        sys_write(1, s.as_bytes());
        Ok(())
    }
}

pub fn print(args: fmt::Arguments) {
    use core::fmt::Write;
    Console.write_fmt(args).unwrap();
}

#[macro_export]
macro_rules! uprint {
    ($($arg:tt)*) => ($crate::console::print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! uprintln {
    () => ($crate::uprint!("\n"));
    ($($arg:tt)*) => ($crate::uprint!("{}\n", format_args!($($arg)*)));
}
