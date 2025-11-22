//! 物理フレーム管理
//!
//! ブートローダから渡されたメモリマップに基づいて、物理メモリフレームを管理します。

use bootloader_api::info::{MemoryRegionKind, MemoryRegions};
use x86_64::structures::paging::{FrameAllocator, PhysFrame, Size4KiB};
use x86_64::PhysAddr;

/// ブート情報（メモリマップ）に基づくフレームアロケータ
///
/// 単純なバンプアロケータとして実装されており、一度割り当てたフレームは再利用しません。
/// OS起動時の初期化段階で使用することを想定しています。
pub struct BootInfoFrameAllocator {
    memory_map: &'static MemoryRegions,
    next: usize,
}

impl BootInfoFrameAllocator {
    /// メモリマップからフレームアロケータを初期化
    ///
    /// # Safety
    ///
    /// この関数を呼び出すには、以下の条件を満たす必要があります:
    /// 
    /// - `memory_map` が有効なメモリ領域情報を含むこと
    /// - `memory_map` のライフタイムが 'static であり、プログラム全体で有効であること
    /// - メモリマップ内の各領域が有効なアドレス範囲を指していること
    /// - この関数は一度だけ呼び出されるべきであること
    /// - 割り当てられるフレームが他の目的で使用中でないこと
    pub unsafe fn init(memory_map: &'static MemoryRegions) -> Self {
        // メモリマップが空でないことを確認（デバッグビルドのみ）
        debug_assert!(
            memory_map.iter().count() > 0,
            "Memory map must not be empty"
        );
        
        // 各メモリ領域の基本的な妥当性を確認（デバッグビルドのみ）
        #[cfg(debug_assertions)]
        for region in memory_map.iter() {
            debug_assert!(
                region.start < region.end,
                "Memory region start must be less than end"
            );
            debug_assert!(
                region.start.checked_add(region.end - region.start).is_some(),
                "Memory region must not overflow"
            );
        }
        
        BootInfoFrameAllocator {
            memory_map,
            next: 0,
        }
    }

    /// 利用可能なフレームのイテレータを返す
    fn usable_frames(&self) -> impl Iterator<Item = PhysFrame> {
        // 利用可能な領域（Usable）のみを抽出
        let regions = self.memory_map.iter();
        let usable_regions = regions
            .filter(|r| r.kind == MemoryRegionKind::Usable);
        
        // 各領域をフレームのアドレス範囲に変換
        let addr_ranges = usable_regions
            .map(|r| {
                let start = PhysFrame::containing_address(PhysAddr::new(r.start));
                let end = PhysFrame::containing_address(PhysAddr::new(r.end - 1));
                PhysFrame::range_inclusive(start, end)
            });
        
        // 範囲をフラットなフレーム列に変換
        addr_ranges.flatten()
    }
}

unsafe impl FrameAllocator<Size4KiB> for BootInfoFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        let frame = self.usable_frames().nth(self.next);
        self.next += 1;
        frame
    }
}

// Safety: BootInfoFrameAllocator only reads from MemoryRegions which is static and immutable after boot.
// Access to the allocator itself is synchronized via Mutex in BOOT_INFO_ALLOCATOR.
unsafe impl Send for BootInfoFrameAllocator {}
unsafe impl Sync for BootInfoFrameAllocator {}
