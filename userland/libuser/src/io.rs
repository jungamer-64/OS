//! High-level I/O API

use crate::syscall;

/// Print a string to stdout
pub fn print(s: &str) {
    let _ = syscall::write(1, s.as_bytes());
}

/// Print a string to stdout with a newline
pub fn println(s: &str) {
    print(s);
    print("\n");
}

/// Macro for formatted printing (simplified version)
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {{
        // For now, we don't support format! in no_std without alloc
        // Users should use the print() function directly
        $crate::io::print($($arg)*);
    }};
}

/// Macro for formatted printing with newline
#[macro_export]
macro_rules! println {
    () => { $crate::io::println("") };
    ($s:expr) => { $crate::io::println($s) };
}
