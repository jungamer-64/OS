// crates/kernel/src/arch/x86_64/gdt.rs
//! Global Descriptor Table (GDT)
//!
//! GDT と TSS (Task State Segment) を設定します。
//! カーネルモード（Ring 0）とユーザーモード（Ring 3）のセグメントを定義します。

use x86_64::structures::gdt::{GlobalDescriptorTable, Descriptor, SegmentSelector};
use x86_64::structures::tss::TaskStateSegment;
use x86_64::VirtAddr;
use spin::{Mutex, Lazy};

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
    // D/B: 0 (should be 0 for 64-bit mode compatibility)
    // G: 1 (granularity)
    //
    // Result: 0x00AF_F200_0000_FFFF (changed from 0x00CF to 0x00AF)
    //   Flags byte (bits 55-52): 0xA = 0b1010
    //     G=1, D/B=0, L=1, AVL=0
    //   Wait, for data segment L should be 0...
    //   Let's use: 0x008F_F200_0000_FFFF
    //     Flags: 0x8 = 0b1000 -> G=1, D/B=0, L=0, AVL=0
    //
    // Actually, the standard approach for 64-bit user data segment:
    // - Use same flags as kernel data but with DPL=3
    // - Kernel data uses 0x00CF9300... (D/B=1, G=1)
    // - For 64-bit, data segments can have D/B=1 (it's ignored)
    // 
    // The real issue might be elsewhere. Let's try D/B=0 just to be safe.
    // 0x008F_F200_0000_FFFF: G=1, D/B=0, L=0, AVL=0
    
    let descriptor_value: u64 = 0x008F_F200_0000_FFFF;
    
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

// Note: TSS is now managed by the tss.rs module (Phase 2)
// The old static TSS has been removed and replaced with tss::TSS

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

static GDT: Lazy<(GlobalDescriptorTable, Selectors)> = Lazy::new(|| {
    // GlobalDescriptorTable in x86_64 0.15.2 has a fixed size
    // We need to ensure it's large enough for all segments including TSS (2 entries)
    let mut gdt = GlobalDescriptorTable::new();
    
    // IMPORTANT: The order of segments MUST follow the SYSRET instruction requirements:
    // SYSRET (64-bit mode) behavior:
    //   CS = STAR[63:48] + 16 (with RPL=3)
    //   SS = STAR[63:48] + 8  (with RPL=3)
    //
    // We want: CS = user_code (0x18), SS = user_data (0x10)
    // So: STAR[63:48] = 0x08 (kernel_code)
    //     CS = 0x08 + 16 = 0x18 ✓
    //     SS = 0x08 + 8  = 0x10 ✓
    //
    // CORRECTED GDT layout for SYSRET compatibility:
    //   0x08: kernel_code (Ring 0 code) - SYSCALL entry CS
    //   0x10: user_data   (Ring 3 data) - SYSRET SS (= kernel_code + 8)
    //   0x18: user_code   (Ring 3 code) - SYSRET CS (= kernel_code + 16)
    //   0x20: kernel_data (Ring 0 data)
    //   0x28: TSS
    //
    // Reference: Intel SDM Vol. 2B, SYSRET instruction
    
    let kernel_code = gdt.append(Descriptor::kernel_code_segment());
    
    // User segments come BEFORE kernel_data for SYSRET compatibility
    crate::debug_println!("[GDT] Using SYSRET-compatible segment order");
    let user_data = gdt.append(create_user_data_descriptor());  // 0x10 (SYSRET SS)
    let user_code = gdt.append(create_user_code_descriptor());  // 0x18 (SYSRET CS)
    
    let kernel_data = gdt.append(Descriptor::kernel_data_segment()); // 0x20
    
    // TSS - now uses the new tss.rs module (Phase 2)
    // We get a reference to the TSS from the tss module
    let tss_ref = unsafe { &*(&*super::tss::TSS.lock() as *const _) };
    let tss = gdt.append(Descriptor::tss_segment(tss_ref));
    
    // DEBUG: Print selector values
    crate::debug_println!("[GDT DEBUG] Kernel code selector: {:#x}", kernel_code.0);
    crate::debug_println!("[GDT DEBUG] User data selector: {:#x}", user_data.0);
    crate::debug_println!("[GDT DEBUG] User code selector: {:#x}", user_code.0);
    crate::debug_println!("[GDT DEBUG] Kernel data selector: {:#x}", kernel_data.0);
    crate::debug_println!("[GDT DEBUG] TSS selector: {:#x}", tss.0);
    
    // Verify SYSRET offset requirements
    // Note: User segments have DPL=3 (Ring 3) in the lower 2 bits,
    // so we need to mask them out for comparison:
    let user_code_base = user_code.0 & !0x03;  // Mask out RPL bits
    let user_data_base = user_data.0 & !0x03;
    
    assert_eq!(
        user_data_base, 
        kernel_code.0 + 8,
        "SYSRET requirement violated: user_data must be kernel_code + 8"
    );
    assert_eq!(
        user_code_base, 
        kernel_code.0 + 16,
        "SYSRET requirement violated: user_code must be kernel_code + 16"
    );
    
    (gdt, Selectors {
        kernel_code,
        kernel_data,
        user_code,
        user_data,
        tss,
    })
});

/// システムコール用のカーネルスタック（後でプロセスごとに管理）
pub static SYSCALL_KERNEL_STACK: Lazy<Mutex<VirtAddr>> = Lazy::new(|| Mutex::new(VirtAddr::new(0)));

/// セグメントセレクタを取得
pub fn selectors() -> &'static Selectors {
    &GDT.1
}

/// GDT descriptor contentをdump（デバッグ用）
pub fn dump_gdt_descriptors() {
    use x86_64::instructions::tables::sgdt;
    
    let gdtr = sgdt();
    let gdt_base = gdtr.base.as_u64();
    let gdt_limit = gdtr.limit as u64;
    
    crate::debug_println!("[GDT DUMP] Base: {:#x}, Limit: {:#x}", gdt_base, gdt_limit);
    
    // Each descriptor is 8 bytes (except TSS which is 16 bytes)
    let num_entries = (gdt_limit + 1) / 8;
    
    for i in 0..num_entries {
        let entry_addr = gdt_base + (i * 8);
        let descriptor = unsafe { *(entry_addr as *const u64) };
        
        crate::debug_println!("[GDT DUMP] Entry {}: offset={:#x}, value={:#018x}", i, i * 8, descriptor);
        
        // Parse descriptor bits for non-null entries
        if descriptor != 0 {
            let limit_low = descriptor & 0xFFFF;
            let base_low = (descriptor >> 16) & 0xFFFF;
            let base_mid = (descriptor >> 32) & 0xFF;
            let access = (descriptor >> 40) & 0xFF;
            let limit_high = (descriptor >> 48) & 0x0F;
            let flags = (descriptor >> 52) & 0x0F;
            let base_high = (descriptor >> 56) & 0xFF;
            
            let present = (access >> 7) & 1;
            let dpl = (access >> 5) & 3;
            let segment_type = access & 0x1F;
            
            crate::debug_println!("    Access={:#04x} (P={}, DPL={}, Type={:#03x}), Flags={:#03x}",
                access, present, dpl, segment_type, flags);
        }
    }
}

/// GDT を初期化
pub fn init() {
    use x86_64::instructions::tables::load_tss;
    use x86_64::instructions::segmentation::{CS, Segment};

    // TSS の初期化は tss.rs モジュールで行う（Phase 2）
    // Double fault スタックのみここで設定
    {
        let mut tss = super::tss::TSS.lock();
        tss.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] = {
            let stack_start = VirtAddr::from_ptr(unsafe { core::ptr::addr_of!(DOUBLE_FAULT_STACK) });
            stack_start + (STACK_SIZE as u64)
        };
        // privilege_stack_table[0] は tss.rs で管理される
        // プロセス切り替え時に update_kernel_stack() で更新される
    }
    
    // GDT をロード
    GDT.0.load();
    unsafe {
        CS::set_reg(GDT.1.kernel_code);
        load_tss(GDT.1.tss);
    }
    
    // DEBUG: Dump GDT content after initialization
    dump_gdt_descriptors();
    
    crate::debug_println!("[GDT] Initialized with new tss.rs module integration");
}
