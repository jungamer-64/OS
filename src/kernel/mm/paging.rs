// src/kernel/mm/paging.rs
//! ページング管理
//!
//! ライフタイムベースのページマッピングで安全性を保証。

use core::marker::PhantomData;
use crate::kernel::core::{KernelResult, KernelError, MemoryError, ErrorKind};
use x86_64::{VirtAddr, PhysAddr};
use x86_64::structures::paging::{PageTable, PageTableFlags, PhysFrame, Size4KiB, Mapper, FrameAllocator};
use x86_64::structures::paging::OffsetPageTable;

/// ページテーブルへの参照を保持するページマッピング
///
/// ライフタイム `'pt` により、ページテーブルの所有権を管理します。
/// Drop 時 に自動的にアンマップされます。
pub struct PageMapping<'pt> {
    page: x86_64::structures::paging::Page<Size4KiB>,
    mapper: &'pt mut OffsetPageTable<'pt>,
    _phantom: PhantomData<&'pt mut PageTable>,
}

impl<'pt> PageMapping<'pt> {
    /// 新しいページマッピングを作成
    ///
    /// # Safety
    ///
    /// virt と phys は有効なアドレスである必要があります。
    /// また、mapper は正しく初期化されている必要があります。
    pub unsafe fn new(
        mapper: &'pt mut OffsetPageTable<'pt>,
        frame_allocator: &mut impl FrameAllocator<Size4KiB>,
        virt: VirtAddr,
        phys: PhysAddr,
        flags: PageTableFlags,
    ) -> KernelResult<Self> {
        use x86_64::structures::paging::Page;
        
        let page = Page::from_start_address(virt).map_err(|_| {
            KernelError::with_context(ErrorKind::Memory(MemoryError::InvalidAddress), "Invalid virtual address")
        })?;
        
        let frame = PhysFrame::from_start_address(phys).map_err(|_| {
            KernelError::with_context(ErrorKind::Memory(MemoryError::InvalidAddress), "Invalid physical address")
        })?;
        
        mapper.map_to(page, frame, flags, frame_allocator).map_err(|_| {
             KernelError::with_context(ErrorKind::Memory(MemoryError::OutOfMemory), "Failed to map page")
        })?.flush();

        Ok(Self {
            page,
            mapper,
            _phantom: PhantomData,
        })
    }
}

impl Drop for PageMapping<'_> {
    fn drop(&mut self) {
        // SAFETY: このマッピングを作成したので、アンマップも安全
        // ただし、本来は unmap の結果を確認すべきだが、Drop では panic できないため無視する
        let _ = self.mapper.unmap(self.page);
    }
}

/// ページテーブル（簡易ラッパー）
/// 
/// # TODO
/// 
/// - TLB フラッシュ実装
/// - メモリバリア追加
/// - マルチコアサポート
pub struct PageTableWrapper {
    inner: PageTable,
}

impl PageTableWrapper {
    pub fn new() -> Self {
        Self {
            inner: PageTable::new(),
        }
    }
}
