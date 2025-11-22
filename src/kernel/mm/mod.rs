// src/kernel/mm/mod.rs
//! メモリ管理モジュール

pub mod paging;
pub mod allocator;
pub mod frame;
pub mod types;

pub use allocator::{LockedHeap, LinkedListAllocator};
pub use frame::BootInfoFrameAllocator;
pub use types::{PhysAddr, VirtAddr, LayoutSize, PageFrameNumber, MemoryError};

use bootloader_api::info::{MemoryRegionKind, MemoryRegions};

/// ブート情報からヒープを初期化
///
/// 利用可能なメモリ領域を検索し、ヒープとして初期化します。
pub fn init_heap(regions: &MemoryRegions) -> Result<(PhysAddr, LayoutSize), &'static str> {
    // ヒープに必要な最小サイズ (例: 100 KiB)
    const MIN_HEAP_SIZE: u64 = 100 * 1024;

    // 利用可能な領域を探す
    let heap_region = regions.iter()
        .find(|r| r.kind == MemoryRegionKind::Usable && r.end - r.start >= MIN_HEAP_SIZE)
        .ok_or("No usable memory region found for heap")?;

    let heap_start = PhysAddr::new(heap_region.start as usize);
    let heap_size = LayoutSize::new((heap_region.end - heap_region.start) as usize);

    // グローバルアロケータを初期化
    // 注意: lib.rs で定義されている ALLOCATOR ではなく、
    // ここでは kernel::mm::allocator::LockedHeap のインスタンスを初期化する必要があるが、
    // グローバルアロケータは static 変数として宣言されているため、
    // 外部からアクセスする手段が必要。
    //
    // しかし、lib.rs の ALLOCATOR は private なので、
    // ここでは初期化ロジックを提供するだけにして、
    // 実際の初期化は main.rs または lib.rs の public な関数経由で行うのが良い。
    //
    // 今回は lib.rs にある `init_heap` を呼び出す形にするため、
    // この関数は単に適切なアドレスとサイズを返すようにする。
    
    Ok((heap_start, heap_size))
}
