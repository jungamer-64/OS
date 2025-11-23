//! 物理フレーム管理
//!
//! ブートローダから渡されたメモリマップに基づいて、物理メモリフレームを管理します。

use bootloader_api::info::{MemoryRegionKind, MemoryRegions};
use x86_64::structures::paging::{FrameAllocator, FrameDeallocator, PhysFrame, Size4KiB};
use x86_64::PhysAddr;
use alloc::collections::{VecDeque, BTreeMap};

/// ブート情報（メモリマップ）に基づくフレームアロケータ
///
/// バンプアロケータ + フリーリストのハイブリッド実装。
/// 解放されたフレームはフリーリストで再利用されます。
pub struct BootInfoFrameAllocator {
    memory_map: &'static MemoryRegions,
    next: usize,
    /// 解放されたフレームのリスト（再利用可能）
    free_frames: VecDeque<PhysFrame<Size4KiB>>,
    /// フレームの参照カウント
    references: BTreeMap<PhysFrame<Size4KiB>, usize>,
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
            free_frames: VecDeque::new(),
            references: BTreeMap::new(),
        }
    }

    /// 利用可能なフレームのイテレータを返す
    fn usable_frames(&self) -> impl Iterator<Item = PhysFrame> {
        // CRITICAL: Skip low memory (< 1MB) to avoid:
        // - NULL pointer (0x0)
        // - BIOS data area
        // - Real mode IVT
        // - Video memory
        // - DMA zones
        const SAFE_MEMORY_START: u64 = 0x100000; // 1MB
        
        // 利用可能な領域（Usable）のみを抽出し、低位メモリをスキップ
        let regions = self.memory_map.iter();
        let usable_regions = regions
            .filter(|r| r.kind == MemoryRegionKind::Usable)
            .filter(|r| r.end > SAFE_MEMORY_START); // Skip regions entirely below 1MB
        
        // 各領域をフレームのアドレス範囲に変換
        let addr_ranges = usable_regions
            .map(|r| {
                // Clamp start address to SAFE_MEMORY_START
                let start_addr = r.start.max(SAFE_MEMORY_START);
                let start = PhysFrame::containing_address(PhysAddr::new(start_addr));
                let end = PhysFrame::containing_address(PhysAddr::new(r.end - 1));
                PhysFrame::range_inclusive(start, end)
            });
        
        // 範囲をフラットなフレーム列に変換
        addr_ranges.flatten()
    }
    
    /// フレームの参照カウントを増やす
    pub fn add_reference(&mut self, frame: PhysFrame<Size4KiB>) {
        let count = self.references.entry(frame).or_insert(0);
        *count += 1;
    }

    /// フレームの参照カウントを減らす
    ///
    /// 参照カウントが0になった場合はtrueを返し、呼び出し元で解放処理を行う必要があることを示します。
    /// (このメソッド自体は解放を行いません)
    pub fn remove_reference(&mut self, frame: PhysFrame<Size4KiB>) -> bool {
        if let Some(count) = self.references.get_mut(&frame) {
            *count -= 1;
            if *count == 0 {
                self.references.remove(&frame);
                return true;
            }
            return false;
        }
        // 参照がない場合は既に解放されているとみなす（または管理外）
        true
    }

    /// フレームを解放してフリーリストに追加
    ///
    /// # Safety
    ///
    /// 呼び出し側は以下を保証する必要があります:
    /// - `frame` がこのアロケータから割り当てられたものであること
    /// - `frame` が他の場所で使用されていないこと
    /// - `frame` が二重解放されないこと
    pub unsafe fn deallocate_frame(&mut self, frame: PhysFrame<Size4KiB>) {
        // 参照カウントを減らす
        if self.remove_reference(frame) {
            // 参照がなくなった場合のみフリーリストに追加
            self.free_frames.push_back(frame);
        }
    }
}

unsafe impl FrameAllocator<Size4KiB> for BootInfoFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        // まずフリーリストから取得を試みる
        if let Some(frame) = self.free_frames.pop_front() {
            self.add_reference(frame);
            return Some(frame);
        }
        
        // フリーリストが空の場合は通常の割り当て
        let frame = self.usable_frames().nth(self.next);
        self.next += 1;
        
        if let Some(frame) = frame {
            self.add_reference(frame);
        }
        
        frame
    }
}

impl FrameDeallocator<Size4KiB> for BootInfoFrameAllocator {
    unsafe fn deallocate_frame(&mut self, frame: PhysFrame<Size4KiB>) {
        // 内部実装を呼び出し
        // SAFETY: Caller guarantees frame is valid and not in use
        unsafe {
            self.deallocate_frame(frame);
        }
    }
}

// Safety: BootInfoFrameAllocator only reads from MemoryRegions which is static and immutable after boot.
// Access to the allocator itself is synchronized via Mutex in BOOT_INFO_ALLOCATOR.
unsafe impl Send for BootInfoFrameAllocator {}
unsafe impl Sync for BootInfoFrameAllocator {}

/// Empty frame allocator that never allocates
/// Used when we only need to map existing frames
pub struct EmptyFrameAllocator;

unsafe impl FrameAllocator<Size4KiB> for EmptyFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        None
    }
}

