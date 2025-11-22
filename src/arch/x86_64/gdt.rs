//! Global Descriptor Table (GDT)
//!
//! GDT と TSS (Task State Segment) を設定します。
//! ダブルフォールト用のスタック切り替えもここで設定します。

use x86_64::structures::gdt::{GlobalDescriptorTable, Descriptor, SegmentSelector};
use x86_64::structures::tss::TaskStateSegment;
use x86_64::VirtAddr;

/// ダブルフォールト用の IST インデックス
pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;

/// ダブルフォールト用のスタック
const STACK_SIZE: usize = 4096 * 5;

#[repr(C, align(4096))]
struct AlignedStack {
    data: [u8; STACK_SIZE],
}

static mut DOUBLE_FAULT_STACK: AlignedStack = AlignedStack {
    data: [0; STACK_SIZE],
};

static mut TSS: TaskStateSegment = TaskStateSegment::new();
static mut GDT: Option<(GlobalDescriptorTable, SegmentSelector, SegmentSelector)> = None;

/// GDT を初期化
pub fn init() {
    use x86_64::instructions::tables::load_tss;
    use x86_64::instructions::segmentation::{CS, Segment};

    unsafe {
        // TSS を設定
        TSS.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] = {
            let stack_start = VirtAddr::from_ptr(core::ptr::addr_of!(DOUBLE_FAULT_STACK));
            stack_start + (STACK_SIZE as u64)
        };

        // GDT を作成
        let mut gdt = GlobalDescriptorTable::new();
        let code_selector = gdt.append(Descriptor::kernel_code_segment());
        let tss_selector = gdt.append(Descriptor::tss_segment(&*core::ptr::addr_of!(TSS)));
        
        GDT = Some((gdt, code_selector, tss_selector));
        
        // GDT をロード
        if let Some((ref gdt, code_sel, tss_sel)) = GDT {
            gdt.load();
            CS::set_reg(code_sel);
            load_tss(tss_sel);
        }
    }
}
