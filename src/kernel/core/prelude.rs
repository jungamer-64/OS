//! カーネルコア prelude
//!
//! `use crate::kernel::core::prelude::*;` でよく使う型と trait をインポート

pub use super::traits::{Device, CharDevice, BlockDevice, Task, Scheduler, TaskState};
pub use super::types::{DeviceId, TaskId, ProcessId, Priority};
pub use super::result::{KernelResult, KernelError, ErrorKind, DeviceError, MemoryError, TaskError};
