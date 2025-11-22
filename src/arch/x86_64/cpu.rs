// src/arch/x86_64/cpu.rs

use crate::arch::Cpu;
use x86_64::instructions::{hlt, interrupts};

/// 割り込みフラグの状態
#[derive(Clone, Copy, Debug)]
pub struct InterruptFlags(u64);

pub struct X86Cpu;

impl Cpu for X86Cpu {
    fn halt() {
        hlt();
    }
    
    fn disable_interrupts() {
        interrupts::disable();
    }
    
    fn enable_interrupts() {
        interrupts::enable();
    }
    
    fn are_interrupts_enabled() -> bool {
        interrupts::are_enabled()
    }
}

impl X86Cpu {
    /// 現在の割り込みフラグを保存し、割り込みを無効化
    ///
    /// # 戻り値
    ///
    /// 保存された割り込みフラグの状態
    #[inline]
    pub fn save_and_disable_interrupts() -> InterruptFlags {
        let rflags: u64;
        unsafe {
            core::arch::asm!(
                "pushfq",
                "pop {}",
                "cli",
                out(reg) rflags,
                options(nomem, nostack, preserves_flags)
            );
        }
        InterruptFlags(rflags)
    }
    
    /// 保存された割り込みフラグを復元
    ///
    /// # Safety
    ///
    /// `flags` は `save_and_disable_interrupts` で取得した
    /// 正当な値である必要があります。
    #[inline]
    pub unsafe fn restore_interrupts(flags: InterruptFlags) {
        unsafe {
            core::arch::asm!(
                "push {}",
                "popfq",
                in(reg) flags.0,
                options(nomem, nostack)
            );
        }
    }
}

/// クリティカルセクションを実行（割り込みフラグを保存・復元）
///
/// パニック時でも割り込みフラグが正しく復元されることを保証します。
///
/// # 例
///
/// ```no_run
/// use crate::arch::x86_64::critical_section;
///
/// let result = critical_section(|| {
///     // クリティカルセクションで実行したい処理
///     42
/// });
/// ```
pub fn critical_section<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    // 現在の割り込みフラグを保存してから無効化
    let saved_flags = X86Cpu::save_and_disable_interrupts();
    
    // パニック時でも復元を保証するRAIIガード
    struct InterruptGuard(InterruptFlags);
    
    impl Drop for InterruptGuard {
        fn drop(&mut self) {
            // スコープを抜ける際に元の状態に戻す
            // パニック時も含めて必ず実行される
            unsafe {
                // Safety: save_and_disable_interrupts で保存した
                // 正当なフラグ値を復元している
                X86Cpu::restore_interrupts(self.0);
            }
        }
    }
    
    let _guard = InterruptGuard(saved_flags);
    
    // クリティカルセクションを実行
    // パニックしても _guard のドロップで割り込みフラグは復元される
    f()
}

/// Read the Time Stamp Counter (TSC).
#[must_use]
pub fn read_timestamp() -> u64 {
    unsafe { core::arch::x86_64::_rdtsc() }
}
