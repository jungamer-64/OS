// kernel/src/arch/x86_64/gdt.rs
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
        
        // IMPORTANT: The order of segments MUST follow the SYSRET instruction requirements:
        // - user_code segment MUST be at (kernel_code + 16)
        // - user_data segment MUST be at (kernel_code + 24)
        //
        // Correct order (satisfies SYSRET):
        //   0x08: kernel_code (Ring 0 code)
        //   0x10: kernel_data (Ring 0 data)
        //   0x18: user_code   (Ring 3 code) = kernel_code + 16 ✓
        //   0x20: user_data   (Ring 3 data) = kernel_code + 24 ✓
        //
        // Reference: Intel SDM Vol. 2B, SYSRET instruction
        
        let kernel_code = gdt.append(Descriptor::kernel_code_segment());
        let kernel_data = gdt.append(Descriptor::kernel_data_segment());
        let user_code = gdt.append(Descriptor::user_code_segment());
        let user_data = gdt.append(Descriptor::user_data_segment());
        
        // TSS (can be anywhere after the required segments)
        let tss = gdt.append(Descriptor::tss_segment(unsafe { 
            &*core::ptr::addr_of!(TSS) 
        }));
        
        // Verify SYSRET offset requirements
        // Note: User segments have DPL=3 (Ring 3) in the lower 2 bits,
        // so we need to mask them out for comparison:
        //   user_code.0 = 0x1B (0b0001_1011) = base(0x18) + RPL(0x03)
        //   user_data.0 = 0x23 (0b0010_0011) = base(0x20) + RPL(0x03)
        let user_code_base = user_code.0 & !0x03;  // Mask out RPL bits
        let user_data_base = user_data.0 & !0x03;
        
        assert_eq!(
            user_code_base, 
            kernel_code.0 + 16,
            "SYSRET requirement violated: user_code must be kernel_code + 16"
        );
        assert_eq!(
            user_data_base, 
            kernel_code.0 + 24,
            "SYSRET requirement violated: user_data must be kernel_code + 24"
        );
        
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
