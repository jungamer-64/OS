// src/diagnostics.rs

//! システム診断とヘルスチェックモジュール
//!
//! このモジュールは以下の機能を提供します:
//! - ハードウェア状態の継続的な監視
//! - エラーレート追跡
//! - パフォーマンスメトリクス
//! - 自己診断機能

use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

/// システム診断情報
pub struct SystemDiagnostics {
    // VGA関連
    vga_writes: AtomicU64,
    vga_write_failures: AtomicU64,
    vga_scrolls: AtomicU64,

    // Serial関連
    serial_writes: AtomicU64,
    serial_timeouts: AtomicU64,
    serial_reinit_attempts: AtomicU32,

    // パニック関連
    panic_count: AtomicU32,
    nested_panic_detected: AtomicBool,

    // ロック関連
    lock_contentions: AtomicU64,
    max_lock_hold_cycles: AtomicU64,

    // 起動時刻（TSCサイクル）
    boot_timestamp: AtomicU64,
}

impl SystemDiagnostics {
    pub const fn new() -> Self {
        Self {
            vga_writes: AtomicU64::new(0),
            vga_write_failures: AtomicU64::new(0),
            vga_scrolls: AtomicU64::new(0),
            serial_writes: AtomicU64::new(0),
            serial_timeouts: AtomicU64::new(0),
            serial_reinit_attempts: AtomicU32::new(0),
            panic_count: AtomicU32::new(0),
            nested_panic_detected: AtomicBool::new(false),
            lock_contentions: AtomicU64::new(0),
            max_lock_hold_cycles: AtomicU64::new(0),
            boot_timestamp: AtomicU64::new(0),
        }
    }

    pub fn set_boot_time(&self) {
        let tsc = read_tsc();
        self.boot_timestamp.store(tsc, Ordering::Relaxed);
    }

    pub fn record_vga_write(&self, success: bool) {
        self.vga_writes.fetch_add(1, Ordering::Relaxed);
        if !success {
            self.vga_write_failures.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn record_vga_scroll(&self) {
        self.vga_scrolls.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_serial_write(&self) {
        self.serial_writes.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_serial_timeout(&self) {
        self.serial_timeouts.fetch_add(1, Ordering::Relaxed);
    }

    #[allow(dead_code)]
    pub fn record_serial_reinit(&self) {
        self.serial_reinit_attempts.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_panic(&self) -> u32 {
        self.panic_count.fetch_add(1, Ordering::SeqCst)
    }

    pub fn mark_nested_panic(&self) {
        self.nested_panic_detected.store(true, Ordering::SeqCst);
    }

    #[allow(dead_code)]
    pub fn record_lock_contention(&self) {
        self.lock_contentions.fetch_add(1, Ordering::Relaxed);
    }

    #[allow(dead_code)]
    pub fn update_max_lock_hold(&self, cycles: u64) {
        self.max_lock_hold_cycles
            .fetch_max(cycles, Ordering::Relaxed);
    }

    /// 診断情報のスナップショットを取得
    pub fn snapshot(&self) -> DiagnosticSnapshot {
        DiagnosticSnapshot {
            vga_writes: self.vga_writes.load(Ordering::Relaxed),
            vga_write_failures: self.vga_write_failures.load(Ordering::Relaxed),
            vga_scrolls: self.vga_scrolls.load(Ordering::Relaxed),
            serial_writes: self.serial_writes.load(Ordering::Relaxed),
            serial_timeouts: self.serial_timeouts.load(Ordering::Relaxed),
            serial_reinit_attempts: self.serial_reinit_attempts.load(Ordering::Relaxed),
            panic_count: self.panic_count.load(Ordering::SeqCst),
            nested_panic_detected: self.nested_panic_detected.load(Ordering::SeqCst),
            lock_contentions: self.lock_contentions.load(Ordering::Relaxed),
            max_lock_hold_cycles: self.max_lock_hold_cycles.load(Ordering::Relaxed),
            uptime_cycles: read_tsc().saturating_sub(self.boot_timestamp.load(Ordering::Relaxed)),
        }
    }

    /// システムヘルスチェック
    pub fn health_check(&self) -> HealthStatus {
        let snap = self.snapshot();
        let mut issues = HealthIssues::default();

        // VGAエラーレートチェック
        if snap.vga_writes > 0 {
            let error_rate = (snap.vga_write_failures as f32) / (snap.vga_writes as f32);
            if error_rate > 0.1 {
                issues.high_vga_error_rate = true;
            }
        }

        // Serialタイムアウトチェック
        if snap.serial_writes > 0 {
            let timeout_rate = (snap.serial_timeouts as f32) / (snap.serial_writes as f32);
            if timeout_rate > 0.05 {
                issues.high_serial_timeout_rate = true;
            }
        }

        // パニックチェック
        if snap.panic_count > 0 {
            issues.panic_occurred = true;
        }

        if snap.nested_panic_detected {
            issues.nested_panic = true;
        }

        // ロック競合チェック
        if snap.lock_contentions > 1000 {
            issues.high_lock_contention = true;
        }

        // 長時間ロック保持チェック
        // 仮定: 2GHz CPU、1msを超えるロック保持は異常
        if snap.max_lock_hold_cycles > 2_000_000 {
            issues.long_lock_holds = true;
        }

        HealthStatus {
            snapshot: snap,
            issues,
        }
    }
}

/// 診断情報のスナップショット
#[derive(Debug, Clone, Copy)]
pub struct DiagnosticSnapshot {
    pub vga_writes: u64,
    pub vga_write_failures: u64,
    pub vga_scrolls: u64,
    pub serial_writes: u64,
    pub serial_timeouts: u64,
    pub serial_reinit_attempts: u32,
    pub panic_count: u32,
    pub nested_panic_detected: bool,
    pub lock_contentions: u64,
    pub max_lock_hold_cycles: u64,
    pub uptime_cycles: u64,
}

/// ヘルス状態
#[derive(Debug)]
pub struct HealthStatus {
    pub snapshot: DiagnosticSnapshot,
    pub issues: HealthIssues,
}

/// 検出された問題
#[derive(Debug, Default)]
pub struct HealthIssues {
    pub high_vga_error_rate: bool,
    pub high_serial_timeout_rate: bool,
    pub panic_occurred: bool,
    pub nested_panic: bool,
    pub high_lock_contention: bool,
    pub long_lock_holds: bool,
}

impl HealthIssues {
    pub fn is_healthy(&self) -> bool {
        !self.high_vga_error_rate
            && !self.high_serial_timeout_rate
            && !self.panic_occurred
            && !self.nested_panic
            && !self.high_lock_contention
            && !self.long_lock_holds
    }

    pub fn severity(&self) -> Severity {
        if self.nested_panic {
            return Severity::Critical;
        }

        if self.panic_occurred || self.high_vga_error_rate {
            return Severity::High;
        }

        if self.high_serial_timeout_rate || self.long_lock_holds {
            return Severity::Medium;
        }

        if self.high_lock_contention {
            return Severity::Low;
        }

        Severity::None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    None,
    Low,
    Medium,
    High,
    Critical,
}

/// グローバル診断インスタンス
pub static DIAGNOSTICS: SystemDiagnostics = SystemDiagnostics::new();

/// TSCを読み取る
#[inline]
fn read_tsc() -> u64 {
    unsafe { core::arch::x86_64::_rdtsc() }
}

/// ヘルスチェックレポートの出力
pub fn print_health_report() {
    let health = DIAGNOSTICS.health_check();

    if crate::serial::is_available() {
        crate::serial_println!("\n=== System Health Report ===");
        crate::serial_println!(
            "VGA Writes: {} (failures: {})",
            health.snapshot.vga_writes,
            health.snapshot.vga_write_failures
        );
        crate::serial_println!("VGA Scrolls: {}", health.snapshot.vga_scrolls);
        crate::serial_println!(
            "Serial Writes: {} (timeouts: {})",
            health.snapshot.serial_writes,
            health.snapshot.serial_timeouts
        );
        crate::serial_println!(
            "Serial Reinit Attempts: {}",
            health.snapshot.serial_reinit_attempts
        );
        crate::serial_println!("Panic Count: {}", health.snapshot.panic_count);
        crate::serial_println!("Lock Contentions: {}", health.snapshot.lock_contentions);
        crate::serial_println!(
            "Max Lock Hold Cycles: {}",
            health.snapshot.max_lock_hold_cycles
        );
        crate::serial_println!("Uptime Cycles: {}", health.snapshot.uptime_cycles);

        let severity = health.issues.severity();
        crate::serial_println!("\nOverall Status: {:?}", severity);

        if !health.issues.is_healthy() {
            crate::serial_println!("\nIssues Detected:");
            if health.issues.high_vga_error_rate {
                crate::serial_println!("  - High VGA error rate");
            }
            if health.issues.high_serial_timeout_rate {
                crate::serial_println!("  - High serial timeout rate");
            }
            if health.issues.panic_occurred {
                crate::serial_println!("  - Panic occurred");
            }
            if health.issues.nested_panic {
                crate::serial_println!("  - Nested panic detected");
            }
            if health.issues.high_lock_contention {
                crate::serial_println!("  - High lock contention");
            }
            if health.issues.long_lock_holds {
                crate::serial_println!("  - Long lock holds detected");
            }
        }

        crate::serial_println!("===========================\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diagnostics_recording() {
        let diag = SystemDiagnostics::new();

        diag.record_vga_write(true);
        diag.record_vga_write(false);

        let snap = diag.snapshot();
        assert_eq!(snap.vga_writes, 2);
        assert_eq!(snap.vga_write_failures, 1);
    }

    #[test]
    fn test_health_check_clean_state() {
        let diag = SystemDiagnostics::new();
        let health = diag.health_check();

        assert!(health.issues.is_healthy());
        assert_eq!(health.issues.severity(), Severity::None);
    }

    #[test]
    fn test_health_check_with_issues() {
        let diag = SystemDiagnostics::new();

        // 多数のVGA書き込みエラーを記録
        for _ in 0..100 {
            diag.record_vga_write(false);
        }

        let health = diag.health_check();
        assert!(!health.issues.is_healthy());
        assert!(health.issues.high_vga_error_rate);
    }
}
