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

/// 手動でUser Code Segment descriptorを作成
/// Long mode (64-bit) 用の正しいdescriptorを確実に生成
fn create_user_code_descriptor() -> Descriptor {
    // 64-bit User Code Segment descriptor
    // Base: 0x00000000 (ignored in 64-bit mode)
    // Limit: 0xFFFFF (ignored in 64-bit mode)
    // Type: 0xA (1010 = Execute/Read, Accessed=0)
    // S: 1 (code/data segment, not system)
    // DPL: 3 (Ring 3)
    // P: 1 (Present) ← CRITICAL!
    // AVL: 0
    // L: 1 (64-bit code segment)
    // D/B: 0 (must be 0 for 64-bit code)
    // G: 1 (granularity, ignored but typically set)
    //
    // Descriptor format (64 bits):
    //   63-56: Base[31:24] = 0x00
    //   55:    G = 1
    //   54:    D/B = 0
    //   53:    L = 1
    //   52:    AVL = 0
    //   51-48: Limit[19:16] = 0xF
    //   47:    P = 1 ← MUST BE SET!
    //   46-45: DPL = 3 (11 binary)
    //   44:    S = 1
    //   43-40: Type = 0xA (1010)
    //   39-16: Base[23:0] = 0x000000
    //   15-0:  Limit[15:0] = 0xFFFF
    //
    // Result: 0x00AF_FA00_0000_FFFF
    //         = 0b0000_0000_1010_1111_1111_1010_0000_0000_0000_0000_0000_0000_1111_1111_1111_1111
    //
    // Breaking down bit 47 (Present):
    //   Bits 47-40 = 0xFA = 0b1111_1010
    //                        ^--- Bit 47 (P) = 1 ✓
    //                         ^^- Bits 46-45 (DPL) = 11 (3) ✓
    //                           ^- Bit 44 (S) = 1 ✓
    //                            ^^^^- Bits 43-40 (Type) = 1010 (0xA) ✓
    
    let descriptor_value: u64 = 0x00AF_FA00_0000_FFFF;
    
    unsafe {
        // SAFETY: We manually constructed a valid 64-bit code segment descriptor
        Descriptor::UserSegment(descriptor_value)
    }
}

/// 手動でUser Data Segment descriptorを作成
/// Long mode (64-bit) 用の正しいdescriptorを確実に生成
fn create_user_data_descriptor() -> Descriptor {
    // 64-bit User Data Segment descriptor
    // Base: 0x00000000 (ignored in 64-bit mode)
    // Limit: 0xFFFFF (ignored in 64-bit mode)
    // Type: 0x2 (0010 = Read/Write, Accessed=0)
    // S: 1 (code/data segment)
    // DPL: 3 (Ring 3)
    // P: 1 (Present) ← CRITICAL!
    // AVL: 0
    // L: 0 (data segments don't use L bit in 64-bit mode)
    // D/B: 1 (32-bit operands)
    // G: 1 (granularity)
    //
    // Result: 0x00CF_F200_0000_FFFF
    //         = 0b0000_0000_1100_1111_1111_0010_0000_0000_0000_0000_0000_0000_1111_1111_1111_1111
    //
    // Breaking down bit 47 (Present):
    //   Bits 47-40 = 0xF2 = 0b1111_0010
    //                        ^--- Bit 47 (P) = 1 ✓
    //                         ^^- Bits 46-45 (DPL) = 11 (3) ✓
    //                           ^- Bit 44 (S) = 1 ✓
    //                            ^^^^- Bits 43-40 (Type) = 0010 (0x2) ✓
    
    let descriptor_value: u64 = 0x00CF_F200_0000_FFFF;
    
    unsafe {
        // SAFETY: We manually constructed a valid data segment descriptor
        Descriptor::UserSegment(descriptor_value)
    }
}

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
        // GlobalDescriptorTable in x86_64 0.15.2 has a fixed size
        // We need to ensure it's large enough for all segments including TSS (2 entries)
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
        
        // Use manually created user segments with guaranteed Present bit
        // This ensures compatibility with Long Mode (64-bit) requirements
        crate::debug_println!("[GDT] Using manually created user segment descriptors");
        let user_code = gdt.append(create_user_code_descriptor());
        let user_data = gdt.append(create_user_data_descriptor());
        
        // TSS (can be anywhere after the required segments)
        let tss = gdt.append(Descriptor::tss_segment(unsafe { 
            &*core::ptr::addr_of!(TSS) 
        }));
        
        // DEBUG: Print selector values
        crate::debug_println!("[GDT DEBUG] Kernel code selector: {:#x}", kernel_code.0);
        crate::debug_println!("[GDT DEBUG] Kernel data selector: {:#x}", kernel_data.0);
        crate::debug_println!("[GDT DEBUG] User code selector: {:#x}", user_code.0);
        crate::debug_println!("[GDT DEBUG] User data selector: {:#x}", user_data.0);
        crate::debug_println!("[GDT DEBUG] TSS selector: {:#x}", tss.0);
        
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
