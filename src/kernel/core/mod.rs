//! カーネルコア抽象化
//! 
//! このモジュールは、カーネル全体で使用する基本的な trait、型、
//! エラーハンドリングを提供します。

pub mod traits;
pub mod types;
pub mod result;

pub use traits::{Device, CharDevice, BlockDevice, Task, Scheduler, TaskState};
pub use types::{DeviceId, TaskId, ProcessId, Priority};
pub use result::{KernelResult, KernelError, ErrorKind, DeviceError, MemoryError, TaskError};
