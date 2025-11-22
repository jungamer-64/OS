// src/kernel/mm/mod.rs
//! メモリ管理モジュール

pub mod paging;
pub mod allocator;
pub mod frame;

pub use allocator::{LockedHeap, LinkedListAllocator};
pub use frame::{BitmapFrameAllocator, LockedFrameAllocator};
