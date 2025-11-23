// src/kernel/mm/mod.rs
//! メモリ管理モジュール

pub mod paging;
pub mod allocator;
pub mod frame;
pub mod types;
pub mod user_paging;
pub mod page_fault;

pub use allocator::{LockedHeap, LinkedListAllocator};
pub use frame::BootInfoFrameAllocator;
pub use types::{PhysAddr, VirtAddr, LayoutSize, PageFrameNumber, MemoryError};

use bootloader_api::info::{MemoryRegionKind, MemoryRegions};
use core::sync::atomic::AtomicU64;

/// Higher-half kernel base address
pub const KERNEL_BASE: u64 = 0xFFFF_FFFF_8000_0000;

/// Physical memory mapping base (in kernel higher-half)
/// This is where physical memory is mapped in the virtual address space
pub const PHYS_MEM_BASE: u64 = 0xFFFF_8880_0000_0000;

/// Physical memory offset (global)
pub static PHYS_MEM_OFFSET: AtomicU64 = AtomicU64::new(0);

/// ブート情報からヒープを初期化
///
/// 利用可能なメモリ領域を検索し、ヒープとして初期化します。
pub fn init_heap(regions: &MemoryRegions) -> Result<(PhysAddr, LayoutSize), &'static str> {
    // ヒープに必要な最小サイズ (例: 100 KiB)
    const MIN_HEAP_SIZE: u64 = 100 * 1024;
    
    // IMPORTANT: Skip low memory regions (< 1MB) to avoid conflicts
    // with legacy BIOS data structures, DMA zones, and potential unmapped regions
    const SAFE_MEMORY_START: u64 = 0x100000; // 1MB
    
    // CRITICAL: Use a region AFTER the first one to avoid conflicts with frame allocator
    // Frame allocator starts from the first usable region, so we skip it
    // We use an iterator with skip(1) to avoid heap allocation before heap initialization
    let mut suitable_regions = regions.iter()
        .filter(|r| r.kind == MemoryRegionKind::Usable)
        .filter(|r| r.start >= SAFE_MEMORY_START) // Skip low memory
        .filter(|r| r.end - r.start >= MIN_HEAP_SIZE);
    
    // Skip the first region (used by frame allocator)
    let _ = suitable_regions.next();
    
    // Use the second region, or fall back to first if only one exists
    let heap_region = suitable_regions.next()
        .or_else(|| {
            // Fall back: re-iterate to get the first one
            regions.iter()
                .filter(|r| r.kind == MemoryRegionKind::Usable)
                .filter(|r| r.start >= SAFE_MEMORY_START)
                .find(|r| r.end - r.start >= MIN_HEAP_SIZE)
        })
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
