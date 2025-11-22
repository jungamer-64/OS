//! 物理フレーム管理
//!
//! 物理メモリの割り当てをビットマップで管理します。

use x86_64::structures::paging::{FrameAllocator, PhysFrame, Size4KiB};
use x86_64::PhysAddr;
use spin::Mutex;

/// ビットマップベースのフレームアロケータ
pub struct BitmapFrameAllocator {
    /// 利用可能なフレームの開始アドレス
    start_frame: PhysFrame<Size4KiB>,
    /// 利用可能なフレームの終了アドレス
    #[allow(dead_code)]
    end_frame: PhysFrame<Size4KiB>,
    /// 次に割り当てるフレームのインデックス
    next: usize,
    /// 総フレーム数
    total_frames: usize,
}

impl BitmapFrameAllocator {
    /// 新しいフレームアロケータを作成
    ///
    /// # Safety
    ///
    /// `start_addr` と `end_addr` は有効な物理メモリ範囲を指している必要があります。
    #[must_use]
    pub unsafe fn new(start_addr: PhysAddr, end_addr: PhysAddr) -> Self {
        let start_frame = PhysFrame::containing_address(start_addr);
        let end_frame = PhysFrame::containing_address(end_addr);
        #[allow(clippy::cast_possible_truncation)]
        let total_frames = (end_frame - start_frame) as usize;

        Self {
            start_frame,
            end_frame,
            next: 0,
            total_frames,
        }
    }

    /// 利用可能なフレーム数を取得
    #[must_use]
    pub const fn free_frames(&self) -> usize {
        self.total_frames.saturating_sub(self.next)
    }

    /// 総フレーム数を取得
    #[must_use]
    pub const fn total_frames(&self) -> usize {
        self.total_frames
    }

    /// 使用中のフレーム数を取得
    #[must_use]
    pub const fn used_frames(&self) -> usize {
        self.next
    }
}

unsafe impl FrameAllocator<Size4KiB> for BitmapFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        if self.next >= self.total_frames {
            return None;
        }

        let frame = self.start_frame + self.next as u64;
        self.next += 1;
        Some(frame)
    }
}

/// Mutex で保護されたフレームアロケータ
pub struct LockedFrameAllocator {
    inner: Mutex<BitmapFrameAllocator>,
}

impl LockedFrameAllocator {
    /// 新しいロックされたフレームアロケータを作成
    ///
    /// # Safety
    ///
    /// `start_addr` と `end_addr` は有効な物理メモリ範囲を指している必要があります。
    #[must_use]
    pub unsafe fn new(start_addr: PhysAddr, end_addr: PhysAddr) -> Self {
        unsafe {
            Self {
                inner: Mutex::new(BitmapFrameAllocator::new(start_addr, end_addr)),
            }
        }
    }

    /// 利用可能なフレーム数を取得
    pub fn free_frames(&self) -> usize {
        self.inner.lock().free_frames()
    }

    /// 総フレーム数を取得
    pub fn total_frames(&self) -> usize {
        self.inner.lock().total_frames()
    }

    /// 使用中のフレーム数を取得
    pub fn used_frames(&self) -> usize {
        self.inner.lock().used_frames()
    }
}

unsafe impl FrameAllocator<Size4KiB> for LockedFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        self.inner.lock().allocate_frame()
    }
}
