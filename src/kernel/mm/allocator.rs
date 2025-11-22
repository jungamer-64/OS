//! メモリアロケータ
//!
//! ヒープ割り当てを管理します。
//! リンクリストベースのアロケータを実装しています。

use core::alloc::{GlobalAlloc, Layout};
use core::ptr;
use core::mem;
use spin::Mutex;

/// リンクリストノード（空きブロックを管理）
struct ListNode {
    size: usize,
    next: Option<&'static mut Self>,
}

impl ListNode {
    const fn new(size: usize) -> Self {
        Self { size, next: None }
    }

    fn start_addr(&self) -> usize {
        core::ptr::from_ref(self) as usize
    }

    fn end_addr(&self) -> usize {
        self.start_addr() + self.size
    }
}

/// アドレスを指定されたアラインメントに切り上げ
const fn align_up(addr: usize, align: usize) -> usize {
    (addr + align - 1) & !(align - 1)
}

/// リンクリストベースのヒープアロケータ
pub struct LinkedListAllocator {
    head: ListNode,
}

impl LinkedListAllocator {
    /// 新しい空のアロケータを作成
    #[must_use]
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {
            head: ListNode::new(0),
        }
    }

    /// アロケータを初期化
    ///
    /// # Safety
    ///
    /// `heap_start` と `heap_size` は有効なヒープ領域を指している必要があります。
    /// この関数は一度だけ呼ばれる必要があります。
    pub unsafe fn init(&mut self, heap_start: usize, heap_size: usize) {
        unsafe {
            self.add_free_region(heap_start, heap_size);
        }
    }

    /// 指定された領域を空きリストに追加
    unsafe fn add_free_region(&mut self, addr: usize, size: usize) {
        // 領域がノードを格納できるサイズであることを確認
        assert_eq!(align_up(addr, mem::align_of::<ListNode>()), addr);
        assert!(size >= mem::size_of::<ListNode>());

        // 新しいノードを作成してリストの先頭に追加
        let mut node = ListNode::new(size);
        node.next = self.head.next.take();
        let node_ptr = addr as *mut ListNode;
        unsafe {
            node_ptr.write(node);
            self.head.next = Some(&mut *node_ptr);
        }
    }

    /// 指定されたサイズとアラインメントに適した領域を見つける
    fn find_region(&mut self, size: usize, align: usize) -> Option<(&'static mut ListNode, usize)> {
        // 現在のリストノードへの参照
        let mut current = &mut self.head;

        // 各リストノードを調べる
        while let Some(ref mut region) = current.next {
            if let Ok(alloc_start) = Self::alloc_from_region(region, size, align) {
                // 見つかった領域をリストから削除
                let next = region.next.take();
                // Safety: current.next is Some(region) as verified by the while-let
                if let Some(removed_region) = current.next.take() {
                    current.next = next;
                    return Some((removed_region, alloc_start));
                }
                // This should never happen, but handle it safely
                current.next = next;
                return None;
            }
            // Move to next node - we know it exists from the while-let condition
            match current.next.as_mut() {
                Some(next_node) => current = next_node,
                None => break, // Should not happen, but handle gracefully
            }
        }

        // 適切な領域が見つからなかった
        None
    }

    /// 指定された領域から割り当てを試みる
    fn alloc_from_region(region: &ListNode, size: usize, align: usize) -> Result<usize, ()> {
        let alloc_start = align_up(region.start_addr(), align);
        let alloc_end = alloc_start.checked_add(size).ok_or(())?;

        if alloc_end > region.end_addr() {
            // 領域が小さすぎる
            return Err(());
        }

        let excess_size = region.end_addr() - alloc_end;
        if excess_size > 0 && excess_size < mem::size_of::<ListNode>() {
            // 残りの領域が小さすぎてノードを格納できない
            // （フラグメンテーションを避けるため）
            return Err(());
        }

        Ok(alloc_start)
    }

    /// 割り当てを調整してサイズを適切に設定
    fn size_align(layout: Layout) -> (usize, usize) {
        let layout = layout
            .align_to(mem::align_of::<ListNode>())
            .expect("Failed to adjust layout alignment to ListNode alignment - invalid layout")
            .pad_to_align();
        let size = layout.size().max(mem::size_of::<ListNode>());
        (size, layout.align())
    }
}

unsafe impl GlobalAlloc for LinkedListAllocator {
    unsafe fn alloc(&self, _layout: Layout) -> *mut u8 {
        // この実装は単純化のため、ロックを取得する必要がありますが
        // `&self` しかないので、内部可変性が必要です。
        // 実際には LockedHeap でラップします。
        ptr::null_mut()
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // LockedHeap でラップするため、ここでは実装しません
    }
}

/// Mutex で保護されたヒープアロケータ
pub struct LockedHeap {
    inner: Mutex<LinkedListAllocator>,
}

impl LockedHeap {
    /// 新しい空のロックされたヒープを作成
    #[must_use]
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {
            inner: Mutex::new(LinkedListAllocator::new()),
        }
    }

    /// ヒープを初期化
    ///
    /// # Safety
    ///
    /// `heap_start` と `heap_size` は有効なヒープ領域を指している必要があります。
    pub unsafe fn init(&self, heap_start: usize, heap_size: usize) {
        unsafe {
            self.inner.lock().init(heap_start, heap_size);
        }
    }
}

unsafe impl GlobalAlloc for LockedHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let (size, align) = LinkedListAllocator::size_align(layout);
        let mut allocator = self.inner.lock();

        if let Some((region, alloc_start)) = allocator.find_region(size, align) {
            let alloc_end = alloc_start.checked_add(size).expect("overflow");
            let excess_size = region.end_addr() - alloc_end;
            if excess_size > 0 {
                unsafe {
                    allocator.add_free_region(alloc_end, excess_size);
                }
            }
            alloc_start as *mut u8
        } else {
            ptr::null_mut()
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let (size, _) = LinkedListAllocator::size_align(layout);
        unsafe {
            self.inner.lock().add_free_region(ptr as usize, size);
        }
    }
}
