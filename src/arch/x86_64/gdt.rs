//! Global Descriptor Table (GDT)
//!
//! GDT と TSS (Task State Segment) を設定します。
//! カーネルモード（Ring 0）とユーザーモード（Ring 3）のセグメントを定義します。

use x86_64::structures::gdt::{GlobalDescriptorTable, Descriptor, SegmentSelector};
use x86_64::structures::tss::TaskStateSegment;
use x86_64::VirtAddr;
use lazy_static::lazy_static;
use spin::Mutex;

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

/// GDT とセグメントセレクタ
pub struct Selectors {
    /// カーネルコードセグメント（Ring 0）
    pub kernel_code: SegmentSelector,
    /// カーネルデータセグメント（Ring 0）
    pub kernel_data: SegmentSelector,
    /// ユーザーコードセグメント（Ring 3）
    pub user_code: SegmentSelector,
    /// ユーザーデータセグメント（Ring 3）
    pub user_data: SegmentSelector,
    /// TSSセグメント
    pub tss: SegmentSelector,
}

lazy_static! {
    static ref GDT: (GlobalDescriptorTable, Selectors) = {
        let mut gdt = GlobalDescriptorTable::new();
        
        // カーネルセグメント（Ring 0）
        let kernel_code = gdt.append(Descriptor::kernel_code_segment());
        let kernel_data = gdt.append(Descriptor::kernel_data_segment());
        
        // ユーザーセグメント（Ring 3）
        let user_code = gdt.append(Descriptor::user_code_segment());
        let user_data = gdt.append(Descriptor::user_data_segment());
        
        // TSS
        let tss = gdt.append(Descriptor::tss_segment(unsafe { 
            &*core::ptr::addr_of!(TSS) 
        }));
        
        (gdt, Selectors {
            kernel_code,
            kernel_data,
            user_code,
            user_data,
            tss,
        })
    };
    
    /// システムコール用のカーネルスタック（後でプロセスごとに管理）
    pub static ref SYSCALL_KERNEL_STACK: Mutex<VirtAddr> = Mutex::new(VirtAddr::new(0));
}

/// セグメントセレクタを取得
pub fn selectors() -> &'static Selectors {
    &GDT.1
}

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
        
        // システムコール用のカーネルスタックも設定（Ring 3 → Ring 0遷移用）
        // Note: x86_64では privilege_stack_table[0] がRing 3からの遷移に使用される
        TSS.privilege_stack_table[0] = {
            let stack_start = VirtAddr::from_ptr(core::ptr::addr_of!(DOUBLE_FAULT_STACK));
            stack_start + (STACK_SIZE as u64)
        };
        
        // GDT をロード
        GDT.0.load();
        CS::set_reg(GDT.1.kernel_code);
        load_tss(GDT.1.tss);
    }
}
