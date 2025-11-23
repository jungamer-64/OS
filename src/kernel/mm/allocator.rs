// src/mm/allocator.rs
//! メモリアロケータ
//!
//! ヒープ割り当てを管理します。
//! リンクリストベースのアロケータを実装しています。

use core::alloc::{GlobalAlloc, Layout};
use core::ptr;
use core::mem;
use core::sync::atomic::{AtomicBool, Ordering};
use spin::Mutex;
use super::types::{PhysAddr, VirtAddr, LayoutSize, MemoryError};
use super::frame::BootInfoFrameAllocator;

/// Global frame allocator
pub static BOOT_INFO_ALLOCATOR: Mutex<Option<BootInfoFrameAllocator>> = Mutex::new(None);

/// ヒープ統計情報
#[derive(Debug, Clone, Copy)]
pub struct HeapStats {
    /// ヒープ容量（初期化時のサイズ）
    pub heap_capacity: LayoutSize,
    /// 総割り当てバイト数（累積）
    pub total_allocated: LayoutSize,
    /// 総解放バイト数（累積）
    pub total_deallocated: LayoutSize,
    /// 現在の使用バイト数
    pub current_usage: LayoutSize,
    /// 最大使用バイト数（ピーク）
    pub peak_usage: LayoutSize,
    /// 割り当て回数
    pub allocation_count: usize,
    /// 解放回数
    pub deallocation_count: usize,
}

impl HeapStats {
    const fn new() -> Self {
        Self {
            heap_capacity: LayoutSize::zero(),
            total_allocated: LayoutSize::zero(),
            total_deallocated: LayoutSize::zero(),
            current_usage: LayoutSize::zero(),
            peak_usage: LayoutSize::zero(),
            allocation_count: 0,
            deallocation_count: 0,
        }
    }
    
    /// 利用可能な空きメモリ（推定）
    pub fn available(&self) -> LayoutSize {
        self.heap_capacity.checked_sub(self.current_usage)
            .unwrap_or(LayoutSize::zero())
    }
    
    /// 使用率（0-100）
    pub fn usage_rate(&self) -> usize {
        let capacity = self.heap_capacity.as_usize();
        let usage = self.current_usage.as_usize();
        if capacity == 0 {
            return 0;
        }
        (usage * 100) / capacity
    }
}

/// リンクリストノード（空きブロックを管理）
struct ListNode {
    size: LayoutSize,
    next: Option<&'static mut ListNode>,
    // Always include magic number for heap corruption detection
    // (not just in debug builds to maintain consistent size)
    magic: u32,
}

const HEAP_MAGIC: u32 = 0xDEADBEEF;

impl ListNode {
    const fn new(size: LayoutSize) -> Self {
        Self {
            size,
            next: None,
            magic: HEAP_MAGIC,
        }
    }

    fn start_addr(&self) -> VirtAddr {
        unsafe { VirtAddr::new_unchecked(self as *const Self as usize) }
    }

    fn end_addr(&self) -> VirtAddr {
        self.start_addr().checked_add(self.size.as_usize())
            .expect("ListNode end address overflow")
    }
    
    fn verify_magic(&self) -> bool {
        self.magic == HEAP_MAGIC
    }
}

/// リンクリストベースのヒープアロケータ
pub struct LinkedListAllocator {
    head: ListNode,
    stats: HeapStats,
}

impl Default for LinkedListAllocator {
    fn default() -> Self {
        Self::new()
    }
}

impl LinkedListAllocator {
    /// 新しい空のアロケータを作成
    pub const fn new() -> Self {
        Self {
            head: ListNode::new(LayoutSize::zero()),
            stats: HeapStats::new(),
        }
    }

    /// アロケータを初期化
    ///
    /// # Safety
    ///
    /// `heap_start` と `heap_size` は有効なヒープ領域を指している必要があります。
    /// この関数は一度だけ呼ばれる必要があります。
    pub unsafe fn init(&mut self, heap_start: VirtAddr, heap_size: LayoutSize) {
        use crate::debug_println;
        debug_println!("[DEBUG] init() called: heap_start={:#x}, heap_size={}", 
                      heap_start.as_usize(), heap_size.as_usize());
        debug_println!("[DEBUG] head before init: magic={:#x}, size={}", 
                      self.head.magic, self.head.size.as_usize());
        
        // ListNode のアラインメントを考慮して、実際に使える開始アドレスとサイズを計算する
        let node_align = mem::align_of::<ListNode>();

        // VirtAddr::align_up を使用
        let aligned_start = match heap_start.align_up(node_align) {
            Some(a) => a,
            None => return, // アラインメント計算不可
        };

        // 切り上げで減った先頭分
        let head_shrink = aligned_start.as_usize().saturating_sub(heap_start.as_usize());

        // 使えるサイズを計算
        let heap_size_val = heap_size.as_usize();
        if heap_size_val <= head_shrink {
            // 元サイズが小さすぎて使える領域がない
            return;
        }
        let usable_size = LayoutSize::new(heap_size_val - head_shrink);

        // usable_size が ListNode を置ける最小サイズより小さいなら初期化失敗扱い
        if usable_size.as_usize() < mem::size_of::<ListNode>() {
            return;
        }

        // stats.heap_capacity は実際に使えるサイズを記録する
        self.stats.heap_capacity = usable_size;

        // 実際に free リージョンを追加（aligned_start, usable_size）
        // Safety: aligned_start は align_up によって ListNode のアラインに整えられている
        unsafe {
            self.add_free_region(aligned_start, usable_size);
        }
    }

    /// 指定された領域を空きリストに追加し、隣接ブロックと結合
    /// 
    /// # Safety
    /// 
    /// - addr は有効なメモリアドレスである必要があります（非ヌル）
    /// - size は少なくとも ListNode を格納できるサイズである必要があります
    /// - addr + size がオーバーフローしないことを保証する必要があります
    /// - [addr, addr+size) の範囲は有効なヒープ領域内である必要があります
    /// 
    /// アドレス順にソートされた状態を維持しながら挿入し、前後のブロックと結合する
    /// 
    /// 注意: addr は ListNode のアラインメントに合わせて切り上げられます。
    unsafe fn add_free_region(&mut self, addr: VirtAddr, size: LayoutSize) {
        use crate::debug_println;
        
        // スタックトレース情報を追加（簡易版）
        debug_println!("[DEBUG] add_free_region() called: addr={:#x}, size={}", 
                      addr.as_usize(), size.as_usize());
        debug_println!("  [TRACE] Called from alloc/dealloc/init");
        
        let addr_val = addr.as_usize();
        let size_val = size.as_usize();
        
        // ヌルポインタチェック
        if addr_val == 0 {
            return;
        }
        
        // オーバーフローチェック
        if let Some(end) = addr_val.checked_add(size_val) {
            // end がアドレス空間内に収まるか確認
            if end <= addr_val {
                return;
            }
        } else {
            // オーバーフローする場合は追加しない
            return;
        }
        
        let node_align = mem::align_of::<ListNode>();
        let node_min_size = mem::size_of::<ListNode>();

        // アラインメント調整 (PhysAddr::align_up 使用)
        let aligned = match addr.align_up(node_align) {
            Some(a) => a,
            None => return,
        };

        let aligned_val = aligned.as_usize();
        if aligned_val < addr_val { return; }
        let shrink = aligned_val - addr_val;
        if size_val <= shrink || size_val - shrink < node_min_size { return; }
        
        let usable_size = LayoutSize::new(size_val - shrink);
        let new_start = aligned;

        // 挿入位置の検索
        // current < new_region < next となる位置を探す
        let mut current = &mut self.head;
        
        while let Some(ref mut next) = current.next {
            if next.start_addr().as_usize() > new_start.as_usize() {
                break;
            }
            // Safety: while let条件でSomeであることが保証されている
            current = match current.next.as_mut() {
                Some(n) => n,
                None => unreachable!("next was Some in while let condition"),
            };
        }

        // ここで current は new_region の直前のノード（またはhead）
        // current.next は new_region の直後のノード（またはNone）

        // Step 1: 前のブロック（current）との結合判定
        // currentがhead(ダミー)の場合は結合しない（サイズ0なので）
        let merged_with_prev = if !current.size.is_zero() && 
            current.end_addr().as_usize() == new_start.as_usize() {
            // 前のブロックと結合
            current.size = current.size.checked_add(usable_size)
                .expect("ListNode size overflow during merge");
            true
        } else {
            false
        };

        // Step 2: 新しいノードの挿入（前のブロックと結合しなかった場合）
        if !merged_with_prev {
            debug_println!("[DEBUG] Creating new ListNode at {:#x}, size={}", 
                          new_start.as_usize(), usable_size.as_usize());
            
            let mut new_node = ListNode::new(usable_size);
            debug_println!("[DEBUG] new_node created: magic={:#x}, size={}", 
                          new_node.magic, new_node.size.as_usize());
            
            // current.next を new_node.next に繋ぐ
            new_node.next = current.next.take();
            
            let node_ptr = unsafe { new_start.as_mut_ptr::<ListNode>() };
            debug_println!("[DEBUG] Writing new_node to {:#x}", node_ptr as usize);
            
            unsafe {
                node_ptr.write(new_node);
                debug_println!("[DEBUG] Write completed, setting current.next");
                current.next = Some(&mut *node_ptr);
                debug_println!("[DEBUG] current.next set successfully");
            }
        }

        // Step 3: 後ろのブロック（next）との結合判定
        // 結合対象のブロック（prevと結合したならcurrent、そうでなければcurrent.next）
        let target_node = if merged_with_prev {
            current
        } else {
            // current.next は必ずSome(new_node)になっているはず
            match current.next.as_mut() {
                Some(n) => n,
                None => unreachable!("new_node was just inserted"),
            }
        };

        // target_node と target_node.next の結合
        // 借用エラー回避のため、先にend_addrを計算
        let target_end = target_node.end_addr();
        
        if let Some(ref mut next) = target_node.next
            && target_end.as_usize() == next.start_addr().as_usize() {
                // 次のブロックを吸収
                target_node.size = target_node.size.checked_add(next.size)
                    .expect("ListNode size overflow during merge");
                // 次の次のブロックへのポインタを取得して繋ぎ変える
                target_node.next = next.next.take();
            }
    }

    /// 指定されたサイズとアラインメントに適した領域を見つける
    fn find_region(&mut self, size: usize, align: usize) -> Option<(VirtAddr, VirtAddr, LayoutSize, usize)> {
        let mut current = &mut self.head;

        while let Some(ref mut region) = current.next {
            // Debug: Check magic number
            let magic_val = region.magic;
            if !region.verify_magic() {
                use crate::debug_println;
                debug_println!("[ERROR] Heap corruption: magic={:#x}, expected={:#x}", magic_val, HEAP_MAGIC);
                debug_println!("  addr={:#x}, size={}", region.start_addr().as_usize(), region.size.as_usize());
                panic!("Heap corruption detected: invalid magic number in ListNode");
            }

            if let Ok(alloc_start) = Self::alloc_from_region(region, size, align) {
                // 領域情報をコピー
                let region_start = region.start_addr();
                let region_end = region.end_addr();
                let region_size = region.size;
                
                // リストから削除
                let next = region.next.take();
                let _ = current.next.take();  // regionを削除
                current.next = next;
                
                return Some((region_start, region_end, region_size, alloc_start));
            }
            // 次のノードへ移動
            // Safety: while let条件でSomeであることが保証されている
            current = match current.next.as_mut() {
                Some(n) => n,
                None => unreachable!("current.next was Some in while let condition"),
            };
        }

        None
    }

    /// 指定された領域から割り当てを試みる
    fn alloc_from_region(region: &ListNode, size: usize, align: usize) -> Result<usize, ()> {
        let region_start = region.start_addr();
        // PhysAddr::align_up を使用
        let alloc_start_addr = region_start.align_up(align).ok_or(())?;
        let alloc_start = alloc_start_addr.as_usize();
        
        // アライン後の開始位置が領域内にあるかチェック
        let region_start_val = region_start.as_usize();
        let region_end_val = region.end_addr().as_usize();
        if alloc_start < region_start_val || alloc_start > region_end_val {
            return Err(());
        }
        
        let alloc_end = alloc_start.checked_add(size).ok_or(())?;
        
        if alloc_end > region_end_val {
            return Err(());
        }
        
        let excess_size = region_end_val - alloc_end;
        if excess_size > 0 && excess_size < mem::size_of::<ListNode>() {
            // 残りの領域が小さすぎてノードを格納できないが、
            // 内部断片化として許容する（アロケーションに含める）
            // return Err(());
        }
        
        Ok(alloc_start)
    }

    /// 割り当てを調整してサイズを適切に設定
    fn size_align(layout: Layout) -> Result<(usize, usize), ()> {
        let layout = layout
            .align_to(mem::align_of::<ListNode>())
            .map_err(|_| ())?
            .pad_to_align();
        let size = layout.size().max(mem::size_of::<ListNode>());
        Ok((size, layout.align()))
    }
    
    /// メモリ割り当て後の統計更新
    fn record_allocation(&mut self, size: usize) {
        self.stats.allocation_count += 1;
        let size_layout = LayoutSize::new(size);
        self.stats.total_allocated = self.stats.total_allocated.checked_add(size_layout)
            .expect("Total allocated overflow");
        self.stats.current_usage = self.stats.current_usage.checked_add(size_layout)
            .expect("Current usage overflow");
        if self.stats.current_usage.as_usize() > self.stats.peak_usage.as_usize() {
            self.stats.peak_usage = self.stats.current_usage;
        }
    }
    
    /// メモリ解放後の統計更新
    fn record_deallocation(&mut self, size: usize) {
        self.stats.deallocation_count += 1;
        let size_layout = LayoutSize::new(size);
        self.stats.total_deallocated = self.stats.total_deallocated.checked_add(size_layout)
            .expect("Total deallocated overflow");
        self.stats.current_usage = self.stats.current_usage.checked_sub(size_layout)
            .unwrap_or(LayoutSize::zero());
    }
    
    /// 統計情報を取得
    pub fn stats(&self) -> HeapStats {
        self.stats
    }
}

/// Mutex で保護されたヒープアロケータ
pub struct LockedHeap {
    inner: Mutex<LinkedListAllocator>,
    /// 初期化済みフラグ（二重初期化防止用）
    initialized: AtomicBool,
}

impl Default for LockedHeap {
    fn default() -> Self {
        Self::new()
    }
}

impl LockedHeap {
    /// 新しいロックされたヒープアロケータを作成
    pub const fn new() -> Self {
        Self {
            inner: Mutex::new(LinkedListAllocator::new()),
            initialized: AtomicBool::new(false),
        }
    }

    /// ヒープが初期化済みかどうかを確認
    pub fn is_initialized(&self) -> bool {
        self.initialized.load(Ordering::Acquire)
    }

    /// ヒープを初期化
    /// 
    /// # Safety
    ///
    /// - heap_start と heap_size は有効なヒープ領域を指している必要があります
    /// 
    /// # Errors
    ///
    /// 既に初期化されている場合は `Err(MemoryError::InvalidAddress)` を返します
    pub unsafe fn init(&self, heap_start: VirtAddr, heap_size: LayoutSize) -> Result<(), MemoryError> {
        // 既に初期化済みの場合はエラー
        if self.initialized.compare_exchange(
            false, 
            true, 
            Ordering::AcqRel, 
            Ordering::Acquire
        ).is_err() {
            return Err(MemoryError::InvalidAddress);
        }
        
        // 直接VirtAddrを使用
        unsafe {
            self.inner.lock().init(heap_start, heap_size);
        }
        Ok(())
    }
    
    /// ヒープ統計情報を取得
    pub fn stats(&self) -> HeapStats {
        self.inner.lock().stats()
    }
}

unsafe impl GlobalAlloc for LockedHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        use crate::debug_println;
        
        let (size, align) = match LinkedListAllocator::size_align(layout) {
            Ok(sa) => sa,
            Err(_) => return ptr::null_mut(),
        };
        
        debug_println!("[DEBUG] alloc() called: size={}, align={}", size, align);
        
        let mut allocator = self.inner.lock();

        if let Some((region_start, region_end, region_size, alloc_start)) = allocator.find_region(size, align) {
            debug_println!("[DEBUG] Found region: start={:#x}, end={:#x}, size={}, alloc_start={:#x}", 
                          region_start.as_usize(), region_end.as_usize(), region_size.as_usize(), alloc_start);
            
            // Prefixの処理
            let region_start_val = region_start.as_usize();
            if alloc_start > region_start_val {
                let prefix_size = LayoutSize::new(alloc_start - region_start_val);
                if prefix_size.as_usize() >= mem::size_of::<ListNode>() {
                    debug_println!("[DEBUG] Adding prefix: addr={:#x}, size={}", 
                                  region_start_val, prefix_size.as_usize());
                    unsafe {
                        allocator.add_free_region(region_start, prefix_size);
                    }
                }
            }

            // alloc_end の計算
            let alloc_end = match alloc_start.checked_add(size) {
                Some(end) => end,
                None => {
                    // オーバーフロー時は元の領域を戻す
                    unsafe {
                        allocator.add_free_region(region_start, region_size);
                    }
                    return ptr::null_mut();
                }
            };

            // Suffixの処理
            let region_end_val = region_end.as_usize();
            if alloc_end < region_end_val {
                let suffix_size = LayoutSize::new(region_end_val - alloc_end);
                if suffix_size.as_usize() >= mem::size_of::<ListNode>() {
                    let suffix_addr = unsafe { VirtAddr::new_unchecked(alloc_end) };
                    debug_println!("[DEBUG] Adding suffix: addr={:#x}, size={}", 
                                  suffix_addr.as_usize(), suffix_size.as_usize());
                    unsafe {
                        allocator.add_free_region(suffix_addr, suffix_size);
                    }
                }
            }
            
            allocator.record_allocation(size);
            debug_println!("[DEBUG] Returning allocation: {:#x}", alloc_start);
            alloc_start as *mut u8
        } else {
            debug_println!("[DEBUG] No suitable region found");
            ptr::null_mut()
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let (size, _) = match LinkedListAllocator::size_align(layout) {
            Ok(sa) => sa,
            Err(_) => return,
        };
        
        let mut allocator = self.inner.lock();
        allocator.record_deallocation(size);
        
        let addr = unsafe { VirtAddr::new_unchecked(ptr as usize) };
        let size_layout = LayoutSize::new(size);
        unsafe {
            allocator.add_free_region(addr, size_layout);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::alloc::Layout;

    // Helper to align up (PhysAddrメソッドを使用するため不要だが、テスト内の生usize計算用に残す)
    fn align_up(addr: usize, align: usize) -> Option<usize> {
        let remainder = addr % align;
        if remainder == 0 {
            Some(addr)
        } else {
            addr.checked_add(align - remainder)
        }
    }

    #[test]
    fn test_init_unaligned() {
        let mut heap = LockedHeap::new();
        // Use a static array to ensure validity during test
        static mut HEAP_MEM: [u8; 4096] = [0; 4096];
        
        unsafe {
            let start = HEAP_MEM.as_ptr() as usize;
            let unaligned_start = start + 1;
            let size = 4096 - 1;
            
            // VirtAddr::newを使用（注意: types.rsでnewが復活している前提）
            heap.init(VirtAddr::new(unaligned_start), LayoutSize::new(size));
            
            let stats = heap.stats();
            let align = core::mem::align_of::<ListNode>();
            let aligned_start = align_up(unaligned_start, align).unwrap();
            let expected_capacity = size - (aligned_start - unaligned_start);
            
            assert_eq!(stats.heap_capacity.as_usize(), expected_capacity);
            assert!(stats.heap_capacity.as_usize() > 0);
            
            let layout = Layout::new::<u64>();
            let ptr = heap.alloc(layout);
            assert!(!ptr.is_null());
            assert_eq!(ptr as usize % layout.align(), 0);
            heap.dealloc(ptr, layout);
        }
    }

    #[test]
    fn test_coalescing() {
        let mut heap = LockedHeap::new();
        static mut HEAP_MEM: [u8; 4096] = [0; 4096];
        unsafe { heap.init(VirtAddr::new(HEAP_MEM.as_ptr() as usize), LayoutSize::new(4096)); }

        let layout = Layout::from_size_align(64, 16).unwrap();

        unsafe {
            let ptr1 = heap.alloc(layout);
            let ptr2 = heap.alloc(layout);
            let ptr3 = heap.alloc(layout);

            assert!(!ptr1.is_null());
            assert!(!ptr2.is_null());
            assert!(!ptr3.is_null());

            // Free middle
            heap.dealloc(ptr2, layout);
            
            // Free first -> merge with middle
            heap.dealloc(ptr1, layout);
            
            // Free last -> merge with (first+middle)
            heap.dealloc(ptr3, layout);

            let stats = heap.stats();
            assert_eq!(stats.current_usage.as_usize(), 0);
            
            // Verify fragmentation is low by allocating full capacity
            let cap = stats.heap_capacity.as_usize();
            let full_layout = Layout::from_size_align(cap, 16).unwrap();
            let ptr_full = heap.alloc(full_layout);
            assert!(!ptr_full.is_null());
            heap.dealloc(ptr_full, full_layout);
        }
    }

    #[test]
    fn test_prefix_suffix() {
        let mut heap = LockedHeap::new();
        static mut HEAP_MEM: [u8; 4096] = [0; 4096];
        unsafe { heap.init(VirtAddr::new(HEAP_MEM.as_ptr() as usize), LayoutSize::new(4096)); }
        
        let align = 256; 
        let size = 64;
        let layout = Layout::from_size_align(size, align).unwrap();
        
        unsafe {
            let ptr = heap.alloc(layout);
            assert!(!ptr.is_null());
            assert_eq!(ptr as usize % align, 0);
            
            let stats = heap.stats();
            assert_eq!(stats.current_usage.as_usize(), size);
            
            heap.dealloc(ptr, layout);
            
            let stats_after = heap.stats();
            assert_eq!(stats_after.current_usage.as_usize(), 0);
            
            let cap = stats_after.heap_capacity.as_usize();
            let full_layout = Layout::from_size_align(cap, mem::align_of::<ListNode>()).unwrap();
            let ptr_full = heap.alloc(full_layout);
            assert!(!ptr_full.is_null());
        }
    }
    
    #[test]
    fn test_small_fragment() {
        let mut heap = LockedHeap::new();
        static mut HEAP_MEM: [u8; 4096] = [0; 4096];
        unsafe { heap.init(VirtAddr::new(HEAP_MEM.as_ptr() as usize), LayoutSize::new(4096)); }
        
        let cap = heap.stats().heap_capacity.as_usize();
        let node_size = mem::size_of::<ListNode>();
        
        // Alloc such that remaining is < node_size
        let size = cap - node_size + 1; 
        
        let layout = Layout::from_size_align(size, 1).unwrap();
        
        unsafe {
            let ptr = heap.alloc(layout);
            assert!(!ptr.is_null());
            
            let stats = heap.stats();
            // Usage should be size
            assert_eq!(stats.current_usage.as_usize(), size);
            
            // The fragment is lost (internal fragmentation).
            // If we dealloc, we get `size` back.
            heap.dealloc(ptr, layout);
            
            // If we try to alloc full capacity, it should FAIL because the fragment is lost.
            let full_layout = Layout::from_size_align(cap, mem::align_of::<ListNode>()).unwrap();
            let ptr_full = heap.alloc(full_layout);
            assert!(ptr_full.is_null());
        }
    }
}