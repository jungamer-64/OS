// src/display.rs

//! Display and output formatting facade.
//!
//! This module re-exports helpers defined in submodules to keep the
//! public API stable while the implementation is split across smaller
//! units.

mod boot;
mod core;
mod panic;

#[allow(unused_imports)]
pub use boot::{
    display_boot_environment, display_boot_environment_with, display_boot_information,
    display_boot_information_with, display_feature_list, display_feature_list_with,
    display_usage_note, display_usage_note_with,
};
#[allow(unused_imports)]
pub use core::{
    broadcast, broadcast_args, broadcast_args_with, broadcast_with, HardwareOutput, Output,
};
#[allow(unused_imports)]
pub use panic::{display_panic_info_serial, display_panic_info_vga};

#[cfg(all(test, feature = "std-tests"))]
mod tests;
