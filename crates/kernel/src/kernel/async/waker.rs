// crates/kernel/src/kernel/async/waker.rs
//! Waker ユーティリティ
//!
//! カスタム Waker を作成するためのヘルパー関数。

use core::task::{RawWaker, RawWakerVTable, Waker};

/// ダミー Waker を作成
///
/// 何もしない Waker。テストや初期実装で使用。
pub fn dummy_waker() -> Waker {
    fn no_op(_: *const ()) {}
    fn clone(_: *const ()) -> RawWaker {
        dummy_raw_waker()
    }

    fn dummy_raw_waker() -> RawWaker {
        RawWaker::new(
            core::ptr::null(),
            &RawWakerVTable::new(clone, no_op, no_op, no_op),
        )
    }

    unsafe { Waker::from_raw(dummy_raw_waker()) }
}

/// カスタム Waker を作成するビルダー
pub struct WakerBuilder;

impl WakerBuilder {
    /// ダミー Waker を作成
    pub fn dummy() -> Waker {
        dummy_waker()
    }
}
