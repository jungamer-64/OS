// src/mm/allocator.rs
//! メモリアロケータ
//!
//! ヒープ割り当てを管理します。
//! リンクリストベースのアロケータを実装しています。

use core::alloc::{GlobalAlloc, Layout};
use core::ptr;
use core::mem;
use spin::Mutex;

/// ヒープ統計情報
#[derive(Debug, Clone, Copy)]
pub struct HeapStats {
    /// ヒープ容量（初期化時のサイズ）
    pub heap_capacity: usize,
    /// 総割り当てバイト数（累積）
    pub total_allocated: usize,
    /// 総解放バイト数（累積）
    pub total_deallocated: usize,
    /// 現在の使用バイト数
    pub current_usage: usize,
    /// 最大使用バイト数（ピーク）
    pub peak_usage: usize,
    /// 割り当て回数
    pub allocation_count: usize,
    /// 解放回数
    pub deallocation_count: usize,
}

impl HeapStats {
    const fn new() -> Self {
        Self {
            heap_capacity: 0,
            total_allocated: 0,
            total_deallocated: 0,
            current_usage: 0,
            peak_usage: 0,
            allocation_count: 0,
            deallocation_count: 0,
        }
    }
    
    /// 利用可能な空きメモリ（推定）
    pub fn available(&self) -> usize {
        self.heap_capacity.saturating_sub(self.current_usage)
    }
    
    /// 使用率（0-100）
    pub fn usage_percentage(&self) -> usize {
        if self.heap_capacity == 0 {
            return 0;
        }
        (self.current_usage * 100) / self.heap_capacity
    }
}

/// リンクリストノード（空きブロックを管理）
struct ListNode {
    size: usize,
    next: Option<&'static mut ListNode>,
    #[cfg(debug_assertions)]
    magic: u32, // ヒープ破損検出用マジックナンバー
}

#[cfg(debug_assertions)]
const HEAP_MAGIC: u32 = 0xDEADBEEF;

impl ListNode {
    const fn new(size: usize) -> Self {
        Self {
            size,
            next: None,
            #[cfg(debug_assertions)]
            magic: HEAP_MAGIC,
        }
    }

    fn start_addr(&self) -> usize {
        self as *const Self as usize
    }

    fn end_addr(&self) -> usize {
        self.start_addr() + self.size
    }
    
    #[cfg(debug_assertions)]
    fn verify_magic(&self) -> bool {
        self.magic == HEAP_MAGIC
    }
}

/// アドレスを指定されたアラインメントに切り上げ
/// 
/// # Returns
/// 
/// アラインされたアドレス、またはオーバーフロー/不正なアラインメントの場合は None
fn align_up(addr: usize, align: usize) -> Option<usize> {
    let mask = align.wrapping_sub(1);
    // alignが2の累乗でない、または0の場合はNone
    if align == 0 || (align & mask) != 0 {
        return None;
    }
    addr.checked_add(mask).map(|n| n & !mask)
}

/// リンクリストベースのヒープアロケータ
pub struct LinkedListAllocator {
    head: ListNode,
    stats: HeapStats,
}

impl LinkedListAllocator {
    /// 新しい空のアロケータを作成
    pub const fn new() -> Self {
        Self {
            head: ListNode::new(0),
            stats: HeapStats::new(),
        }
    }

    /// アロケータを初期化
    ///
    /// # Safety
    ///
    /// `heap_start` と `heap_size` は有効なヒープ領域を指している必要があります。
    /// この関数は一度だけ呼ばれる必要があります。
    pub unsafe fn init(&mut self, heap_start: usize, heap_size: usize) {
        self.stats.heap_capacity = heap_size;
        unsafe {
            self.add_free_region(heap_start, heap_size);
        }
    }

    /// 指定された領域を空きリストに追加し、隣接ブロックと結合
    /// 
    /// アドレス順にソートされた状態を維持しながら挿入し、前後のブロックと結合する
    unsafe fn add_free_region(&mut self, addr: usize, size: usize) {
        let node_align = mem::align_of::<ListNode>();
        let node_min_size = mem::size_of::<ListNode>();

        // アラインメント調整
        let aligned = match align_up(addr, node_align) {
            Some(a) => a,
            None => return,
        };

        if aligned < addr { return; }
        let shrink = aligned - addr;
        if size <= shrink || size - shrink < node_min_size { return; }
        
        let usable_size = size - shrink;
        let new_start = aligned;
        // new_end is not strictly needed here as we calculate it dynamically during merge, 
        // but let's keep it if we want to use it for checks. 
        // Actually, the error said it's unused, so let's remove it.

        // 挿入位置の検索
        // current < new_region < next となる位置を探す
        let mut current = &mut self.head;
        
        while let Some(ref mut next) = current.next {
            if next.start_addr() > new_start {
                break;
            }
            current = current.next.as_mut().unwrap();
        }

        // ここで current は new_region の直前のノード（またはhead）
        // current.next は new_region の直後のノード（またはNone）

        // Step 1: 前のブロック（current）との結合判定
        // currentがhead(ダミー)の場合は結合しない（サイズ0なので）
        let merged_with_prev = if current.size > 0 && current.end_addr() == new_start {
            // 前のブロックと結合
            current.size += usable_size;
            true
        } else {
            false
        };

        // Step 2: 新しいノードの挿入（前のブロックと結合しなかった場合）
        if !merged_with_prev {
            let mut new_node = ListNode::new(usable_size);
            // current.next を new_node.next に繋ぐ
            new_node.next = current.next.take();
            
            let node_ptr = new_start as *mut ListNode;
            unsafe {
                node_ptr.write(new_node);
                current.next = Some(&mut *node_ptr);
            }
        }

        // Step 3: 後ろのブロック（next）との結合判定
        // current は今や「結合されたブロック」または「新しく挿入されたブロック」を指している必要はない
        // 実際には、merged_with_prevならcurrentが拡大されたブロック。
        // そうでなければ、current.nextが新しいブロック。
        // どちらの場合も、"着目しているブロック" と "その次のブロック" の結合を試みる必要がある。

        // 結合対象のブロック（prevと結合したならcurrent、そうでなければcurrent.next）
        let target_node = if merged_with_prev {
            current
        } else {
            // current.next は必ずSome(new_node)になっているはず
            current.next.as_mut().unwrap()
        };

        // target_node と target_node.next の結合
        // 借用エラー回避のため、先にend_addrを計算
        let target_end = target_node.end_addr();
        
        if let Some(ref mut next) = target_node.next {
            if target_end == next.start_addr() {
                // 次のブロックを吸収
                target_node.size += next.size;
                // 次の次のブロックへのポインタを取得して繋ぎ変える
                target_node.next = next.next.take();
            }
        }
    }

    /// 指定されたサイズとアラインメントに適した領域を見つける
    fn find_region(&mut self, size: usize, align: usize) -> Option<(&'static mut ListNode, usize)> {
        let mut current = &mut self.head;

        while let Some(ref mut region) = current.next {
            #[cfg(debug_assertions)]
            {
                if !region.verify_magic() {
                    panic!("Heap corruption detected: invalid magic number in ListNode");
                }
            }

            if let Ok(alloc_start) = Self::alloc_from_region(region, size, align) {
                // 見つかった領域をリストから削除
                let next = region.next.take();
                // Safety: current.nextがSomeであることはwhile let で保証されている
                let removed = current.next.take().unwrap();
                current.next = next;
                return Some((removed, alloc_start));
            }
            // 次のノードへ移動
            // Safety: while let条件でSomeであることが保証されている
            current = current.next.as_mut().unwrap();
        }

        None
    }

    /// 指定された領域から割り当てを試みる
    fn alloc_from_region(region: &ListNode, size: usize, align: usize) -> Result<usize, ()> {
        let alloc_start = align_up(region.start_addr(), align).ok_or(())?;
        
        // アライン後の開始位置が領域内にあるかチェック
        if alloc_start < region.start_addr() || alloc_start > region.end_addr() {
            return Err(());
        }
        
        let alloc_end = alloc_start.checked_add(size).ok_or(())?;
        
        if alloc_end > region.end_addr() {
            return Err(());
        }
        
        let excess_size = region.end_addr() - alloc_end;
        if excess_size > 0 && excess_size < mem::size_of::<ListNode>() {
            // 残りの領域が小さすぎてノードを格納できない
            return Err(());
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
        self.stats.total_allocated += size;
        self.stats.current_usage += size;
        self.stats.peak_usage = self.stats.peak_usage.max(self.stats.current_usage);
    }
    
    /// メモリ解放後の統計更新
    fn record_deallocation(&mut self, size: usize) {
        self.stats.deallocation_count += 1;
        self.stats.total_deallocated += size;
        self.stats.current_usage = self.stats.current_usage.saturating_sub(size);
    }
    
    /// 統計情報を取得
    pub fn stats(&self) -> HeapStats {
        self.stats
    }
}

/// Mutex で保護されたヒープアロケータ
pub struct LockedHeap {
    inner: Mutex<LinkedListAllocator>,
}

impl LockedHeap {
    /// 新しい空のロックされたヒープを作成
    pub const fn new() -> Self {
        Self {
            inner: Mutex::new(LinkedListAllocator::new()),
        }
    }

    /// ヒープを初期化
    ///
    /// # Safety
    ///
    /// - heap_start と heap_size は有効なヒープ領域を指している必要があります
    /// - この関数は一度だけ呼ばれる必要があります
    pub unsafe fn init(&self, heap_start: usize, heap_size: usize) {
        unsafe {
            self.inner.lock().init(heap_start, heap_size);
        }
    }
    
    /// ヒープ統計情報を取得
    pub fn stats(&self) -> HeapStats {
        self.inner.lock().stats()
    }
}

unsafe impl GlobalAlloc for LockedHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let (size, align) = match LinkedListAllocator::size_align(layout) {
            Ok(sa) => sa,
            Err(_) => return ptr::null_mut(),
        };
        
        let mut allocator = self.inner.lock();

        if let Some((region, alloc_start)) = allocator.find_region(size, align) {
            let region_start = region.start_addr();
            let region_end = region.end_addr();

            // Prefixの処理
            if alloc_start > region_start {
                let prefix_size = alloc_start - region_start;
                if prefix_size >= mem::size_of::<ListNode>() {
                    unsafe {
                        allocator.add_free_region(region_start, prefix_size);
                    }
                }
            }

            // alloc_end の計算
            let alloc_end = match alloc_start.checked_add(size) {
                Some(end) => end,
                None => {
                    unsafe {
                        allocator.add_free_region(region_start, region.size);
                    }
                    return ptr::null_mut();
                }
            };

            // Suffixの処理
            if alloc_end < region_end {
                let suffix_size = region_end - alloc_end;
                if suffix_size >= mem::size_of::<ListNode>() {
                    unsafe {
                        allocator.add_free_region(alloc_end, suffix_size);
                    }
                }
            }
            
            allocator.record_allocation(size);
            alloc_start as *mut u8
        } else {
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
        
        unsafe {
            allocator.add_free_region(ptr as usize, size);
        }
    }
}