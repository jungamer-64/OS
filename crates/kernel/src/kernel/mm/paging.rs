// kernel/src/kernel/mm/paging.rs
//! ページング管理
//!
//! x86_64のページテーブル操作を提供します。

use x86_64::structures::paging::{OffsetPageTable, PageTable, PageTableFlags};
use x86_64::VirtAddr;

/// Copy-on-Write flag (Bit 9, available for OS use)
pub const COW_FLAG: PageTableFlags = PageTableFlags::BIT_9;

/// 新しい OffsetPageTable を初期化します。
///
/// # Safety
///
/// この関数を呼び出すには、以下の条件を満たす必要があります:
/// 
/// - 全物理メモリが `physical_memory_offset` から始まる仮想アドレス空間に
///   連続してマッピングされていること
/// - `physical_memory_offset` が有効な仮想アドレスであること
/// - CR3レジスタが有効なレベル4ページテーブルを指していること
/// - この関数は一度だけ呼び出されること（`&mut` 参照のエイリアシングを防ぐため）
/// - 返される OffsetPageTable への参照が同時に複数存在しないこと
/// 
/// これらの条件が満たされない場合、未定義動作が発生する可能性があります。
pub unsafe fn init(physical_memory_offset: VirtAddr) -> OffsetPageTable<'static> {
    // Safety: 呼び出し元が上記の条件を保証している
    let level_4_table = unsafe { active_level_4_table(physical_memory_offset) };
    // Safety: level_4_table は有効なページテーブルを指しており、
    // physical_memory_offset は全物理メモリがマップされているオフセット
    unsafe { OffsetPageTable::new(level_4_table, physical_memory_offset) }
}

/// アクティブなレベル4ページテーブルへの可変参照を返します。
///
/// # Safety
///
/// この関数を呼び出すには、以下の条件を満たす必要があります:
/// 
/// - 全物理メモリが `physical_memory_offset` から始まる仮想アドレス空間に
///   連続してマッピングされていること
/// - CR3レジスタが有効なレベル4ページテーブルのフレームを指していること
/// - 返される可変参照が同時に複数存在しないこと（排他的アクセス）
/// - ページテーブルの内容が有効な状態であること
unsafe fn active_level_4_table(physical_memory_offset: VirtAddr)
    -> &'static mut PageTable
{
    use x86_64::registers::control::Cr3;

    let (level_4_table_frame, _) = Cr3::read();

    let phys = level_4_table_frame.start_address();
    let virt = physical_memory_offset + phys.as_u64();
    let page_table_ptr: *mut PageTable = virt.as_mut_ptr();

    // アドレスの妥当性を確認（デバッグビルドのみ）
    debug_assert!(
        !page_table_ptr.is_null(),
        "Page table pointer must not be null"
    );
    debug_assert!(
        (page_table_ptr as usize).is_multiple_of(core::mem::align_of::<PageTable>()),
        "Page table pointer must be properly aligned"
    );

    // SAFETY: 
    // - CR3レジスタから取得したフレームは有効なレベル4ページテーブルを指している
    // - 呼び出し元が全物理メモリがマップされていることを保証している
    // - 仮想アドレスの計算結果は有効なページテーブルを指している
    unsafe { &mut *page_table_ptr }
}
