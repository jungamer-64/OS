// src/diagnostics.rs

//! システム診断とヘルスチェックモジュール（改善版）
//!
//! # 改善点
//! - より詳細なメトリクス収集
//! - スレッドセーフな統計情報
//! - パフォーマンス影響の最小化
//! - より正確なヘルスチェック判定

use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

/// システム診断情報
pub struct SystemDiagnostics {
    // VGA関連メトリクス
    vga_writes: AtomicU64,
    vga_write_failures: AtomicU64,
    vga_scrolls: AtomicU64,
    vga_color_changes: AtomicU64,

    // Serial関連メトリクス
    serial_writes: AtomicU64,
    serial_bytes_written: AtomicU64,
    serial_timeouts: AtomicU64,
    serial_reinit_attempts: AtomicU32,

    // パニック関連メトリクス
    panic_count: AtomicU32,
    nested_panic_detected: AtomicBool,
    last_panic_location: AtomicU64, // パック化されたファイル/行情報（将来の詳細パニックトレースで使用予定）

    // ロック関連メトリクス
    lock_contentions: AtomicU64,
    max_lock_hold_cycles: AtomicU64,
    total_lock_acquisitions: AtomicU64,

    // システムタイミング
    boot_timestamp: AtomicU64,
    last_health_check: AtomicU64,
}

#[derive(Debug, Clone, Copy)]
#[must_use = "Lock timing token must be finished to record hold duration"]
pub struct LockTimingToken {
    start_cycles: u64,
}

impl SystemDiagnostics {
    pub const fn new() -> Self {
        Self {
            vga_writes: AtomicU64::new(0),
            vga_write_failures: AtomicU64::new(0),
            vga_scrolls: AtomicU64::new(0),
            vga_color_changes: AtomicU64::new(0),
            serial_writes: AtomicU64::new(0),
            serial_bytes_written: AtomicU64::new(0),
            serial_timeouts: AtomicU64::new(0),
            serial_reinit_attempts: AtomicU32::new(0),
            panic_count: AtomicU32::new(0),
            nested_panic_detected: AtomicBool::new(false),
            last_panic_location: AtomicU64::new(0),
            lock_contentions: AtomicU64::new(0),
            max_lock_hold_cycles: AtomicU64::new(0),
            total_lock_acquisitions: AtomicU64::new(0),
            boot_timestamp: AtomicU64::new(0),
            last_health_check: AtomicU64::new(0),
        }
    }

    /// 起動時刻を記録
    #[inline]
    pub fn set_boot_time(&self) {
        let tsc = read_tsc();
        self.boot_timestamp.store(tsc, Ordering::Relaxed);
    }

    /// VGA書き込みを記録（成否を含む）
    #[inline]
    pub fn record_vga_write(&self, success: bool) {
        self.vga_writes.fetch_add(1, Ordering::Relaxed);
        if !success {
            self.vga_write_failures.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// VGAスクロールを記録
    #[inline]
    pub fn record_vga_scroll(&self) {
        self.vga_scrolls.fetch_add(1, Ordering::Relaxed);
    }

    /// VGA色変更を記録（将来のパフォーマンス最適化で使用予定）
    #[inline]
    pub fn record_vga_color_change(&self) {
        self.vga_color_changes.fetch_add(1, Ordering::Relaxed);
    }

    /// Serial書き込みバイト数を記録（将来のスループット分析で使用予定）
    #[inline]
    pub fn record_serial_bytes(&self, bytes: u64) {
        self.serial_bytes_written
            .fetch_add(bytes, Ordering::Relaxed);
    }

    /// Serial書き込みをまとめて記録
    #[inline]
    pub fn record_serial_writes(&self, count: u64) {
        if count > 0 {
            self.serial_writes.fetch_add(count, Ordering::Relaxed);
        }
    }

    /// Serialタイムアウトをまとめて記録
    #[inline]
    pub fn record_serial_timeouts(&self, count: u64) {
        if count > 0 {
            self.serial_timeouts.fetch_add(count, Ordering::Relaxed);
        }
    }

    /// Serial再初期化を記録（将来のエラーリカバリで使用予定）
    #[inline]
    pub fn record_serial_reinit(&self) {
        self.serial_reinit_attempts.fetch_add(1, Ordering::Relaxed);
    }

    /// パニックを記録（カウントを返す）
    pub fn record_panic(&self) -> u32 {
        let count = self.panic_count.fetch_add(1, Ordering::SeqCst);
        self.last_health_check.store(read_tsc(), Ordering::Relaxed);
        count
    }

    /// パニック発生位置を記録
    #[inline]
    pub fn record_panic_location(&self, line: u32, column: u32) {
        let encoded = ((line as u64) << 32) | (column as u64);
        self.last_panic_location.store(encoded, Ordering::Relaxed);
    }

    /// ネストされたパニックをマーク
    pub fn mark_nested_panic(&self) {
        self.nested_panic_detected.store(true, Ordering::SeqCst);
    }

    /// ロック競合を記録
    #[inline]
    pub fn record_lock_contention(&self) {
        self.lock_contentions.fetch_add(1, Ordering::Relaxed);
    }

    /// ロック取得を記録
    #[inline]
    pub fn record_lock_acquisition(&self) {
        self.total_lock_acquisitions.fetch_add(1, Ordering::Relaxed);
    }

    /// 最大ロック保持時間を更新
    #[inline]
    pub fn update_max_lock_hold(&self, cycles: u64) {
        self.max_lock_hold_cycles
            .fetch_max(cycles, Ordering::Relaxed);
    }

    /// ロック計測を開始
    #[inline]
    pub fn begin_lock_timing(&self) -> LockTimingToken {
        LockTimingToken {
            start_cycles: read_tsc(),
        }
    }

    /// ロック解放時に計測を終了
    #[inline]
    pub fn finish_lock_timing(&self, token: LockTimingToken) {
        let elapsed = read_tsc().saturating_sub(token.start_cycles);
        self.update_max_lock_hold(elapsed);
    }

    /// 診断情報のスナップショットを取得
    pub fn snapshot(&self) -> DiagnosticSnapshot {
        let current_tsc = read_tsc();

        DiagnosticSnapshot {
            vga_writes: self.vga_writes.load(Ordering::Relaxed),
            vga_write_failures: self.vga_write_failures.load(Ordering::Relaxed),
            vga_scrolls: self.vga_scrolls.load(Ordering::Relaxed),
            vga_color_changes: self.vga_color_changes.load(Ordering::Relaxed),
            serial_writes: self.serial_writes.load(Ordering::Relaxed),
            serial_bytes_written: self.serial_bytes_written.load(Ordering::Relaxed),
            serial_timeouts: self.serial_timeouts.load(Ordering::Relaxed),
            serial_reinit_attempts: self.serial_reinit_attempts.load(Ordering::Relaxed),
            panic_count: self.panic_count.load(Ordering::SeqCst),
            last_panic_location: self.last_panic_location.load(Ordering::Relaxed),
            nested_panic_detected: self.nested_panic_detected.load(Ordering::SeqCst),
            lock_contentions: self.lock_contentions.load(Ordering::Relaxed),
            max_lock_hold_cycles: self.max_lock_hold_cycles.load(Ordering::Relaxed),
            total_lock_acquisitions: self.total_lock_acquisitions.load(Ordering::Relaxed),
            uptime_cycles: current_tsc.saturating_sub(self.boot_timestamp.load(Ordering::Relaxed)),
        }
    }

    /// システムヘルスチェック（改善版）
    pub fn health_check(&self) -> HealthStatus {
        let snap = self.snapshot();
        let mut issues = HealthIssues::default();

        // VGAエラーレートチェック（改善: 最小サンプル数を要求）
        if snap.vga_writes > 10 {
            let error_rate = (snap.vga_write_failures as f32) / (snap.vga_writes as f32);
            if error_rate > 0.1 {
                issues.high_vga_error_rate = true;
                issues.vga_error_rate = error_rate;
            }
        }

        // Serialタイムアウトチェック（改善: より正確な判定）
        if snap.serial_writes > 10 {
            let timeout_rate = (snap.serial_timeouts as f32) / (snap.serial_writes as f32);
            if timeout_rate > 0.05 {
                issues.high_serial_timeout_rate = true;
                issues.serial_timeout_rate = timeout_rate;
            }
        }

        // パニックチェック
        if snap.panic_count > 0 {
            issues.panic_occurred = true;
        }

        if snap.nested_panic_detected {
            issues.nested_panic = true;
        }

        // ロック競合チェック（改善: 競合率で判定）
        if snap.total_lock_acquisitions > 100 {
            let contention_rate =
                (snap.lock_contentions as f32) / (snap.total_lock_acquisitions as f32);
            if contention_rate > 0.1 {
                issues.high_lock_contention = true;
                issues.lock_contention_rate = contention_rate;
            }
        }

        // 長時間ロック保持チェック（改善: 2GHz CPU、1msを超える）
        if snap.max_lock_hold_cycles > 2_000_000 {
            issues.long_lock_holds = true;
            issues.max_lock_hold_ms = (snap.max_lock_hold_cycles as f32) / 2_000_000.0;
        }

        // 異常なスクロール頻度チェック（新規）
        if snap.vga_writes > 0 {
            let scroll_rate = (snap.vga_scrolls as f32) / (snap.vga_writes as f32);
            if scroll_rate > 0.5 {
                issues.excessive_scrolling = true;
            }
        }

        // ヘルスチェック時刻を更新
        self.last_health_check.store(read_tsc(), Ordering::Relaxed);

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
    pub vga_color_changes: u64,
    pub serial_writes: u64,
    pub serial_bytes_written: u64,
    pub serial_timeouts: u64,
    pub serial_reinit_attempts: u32,
    pub panic_count: u32,
    pub last_panic_location: u64,
    pub nested_panic_detected: bool,
    pub lock_contentions: u64,
    pub max_lock_hold_cycles: u64,
    pub total_lock_acquisitions: u64,
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
    pub vga_error_rate: f32,
    pub high_serial_timeout_rate: bool,
    pub serial_timeout_rate: f32,
    pub panic_occurred: bool,
    pub nested_panic: bool,
    pub high_lock_contention: bool,
    pub lock_contention_rate: f32,
    pub long_lock_holds: bool,
    pub max_lock_hold_ms: f32,
    pub excessive_scrolling: bool,
}

impl HealthIssues {
    pub fn is_healthy(&self) -> bool {
        !self.high_vga_error_rate
            && !self.high_serial_timeout_rate
            && !self.panic_occurred
            && !self.nested_panic
            && !self.high_lock_contention
            && !self.long_lock_holds
            && !self.excessive_scrolling
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

        if self.high_lock_contention || self.excessive_scrolling {
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
        crate::serial_println!("\n=== System Health Report (Enhanced) ===");
        let init_status = crate::init::detailed_status();
        crate::serial_println!(
            "Init Phase: {:?} (operational: {}, output: {}, lock held: {})",
            init_status.phase,
            init_status.is_operational(),
            init_status.has_output(),
            init_status.lock_held
        );
        crate::serial_println!(
            "VGA: {} writes ({} failures, {:.2}% error rate)",
            health.snapshot.vga_writes,
            health.snapshot.vga_write_failures,
            if health.snapshot.vga_writes > 0 {
                (health.snapshot.vga_write_failures as f32 / health.snapshot.vga_writes as f32)
                    * 100.0
            } else {
                0.0
            }
        );
        crate::serial_println!(
            "     {} scrolls, {} color changes",
            health.snapshot.vga_scrolls,
            health.snapshot.vga_color_changes
        );
        crate::serial_println!(
            "Serial: {} writes, {} bytes ({} timeouts, {:.2}% timeout rate)",
            health.snapshot.serial_writes,
            health.snapshot.serial_bytes_written,
            health.snapshot.serial_timeouts,
            if health.snapshot.serial_writes > 0 {
                (health.snapshot.serial_timeouts as f32 / health.snapshot.serial_writes as f32)
                    * 100.0
            } else {
                0.0
            }
        );
        if health.snapshot.serial_reinit_attempts > 0 {
            crate::serial_println!(
                "       Serial reinit attempts: {}",
                health.snapshot.serial_reinit_attempts
            );
        }
        crate::serial_println!(
            "Locks: {} acquisitions, {} contentions ({:.2}% contention rate)",
            health.snapshot.total_lock_acquisitions,
            health.snapshot.lock_contentions,
            if health.snapshot.total_lock_acquisitions > 0 {
                (health.snapshot.lock_contentions as f32
                    / health.snapshot.total_lock_acquisitions as f32)
                    * 100.0
            } else {
                0.0
            }
        );
        crate::serial_println!(
            "       Max lock hold: {} cycles ({:.2} ms @ 2GHz)",
            health.snapshot.max_lock_hold_cycles,
            health.snapshot.max_lock_hold_cycles as f32 / 2_000_000.0
        );
        crate::serial_println!("Panic Count: {}", health.snapshot.panic_count);
        if let Some((line, column)) = decode_panic_location(health.snapshot.last_panic_location) {
            crate::serial_println!(
                "       Last panic location: line {}, column {}",
                line,
                column
            );
        }
        crate::serial_println!("Uptime: {} cycles", health.snapshot.uptime_cycles);

        let severity = health.issues.severity();
        crate::serial_println!("\nOverall Status: {:?}", severity);

        fn decode_panic_location(encoded: u64) -> Option<(u32, u32)> {
            if encoded == 0 {
                return None;
            }

            let line = (encoded >> 32) as u32;
            let column = encoded as u32;

            if line == 0 && column == 0 {
                None
            } else {
                Some((line, column))
            }
        }

        if !health.issues.is_healthy() {
            crate::serial_println!("\nIssues Detected:");
            if health.issues.high_vga_error_rate {
                crate::serial_println!(
                    "  - High VGA error rate: {:.2}%",
                    health.issues.vga_error_rate * 100.0
                );
            }
            if health.issues.high_serial_timeout_rate {
                crate::serial_println!(
                    "  - High serial timeout rate: {:.2}%",
                    health.issues.serial_timeout_rate * 100.0
                );
            }
            if health.issues.panic_occurred {
                crate::serial_println!("  - Panic occurred");
            }
            if health.issues.nested_panic {
                crate::serial_println!("  - Nested panic detected");
            }
            if health.issues.high_lock_contention {
                crate::serial_println!(
                    "  - High lock contention: {:.2}%",
                    health.issues.lock_contention_rate * 100.0
                );
            }
            if health.issues.long_lock_holds {
                crate::serial_println!(
                    "  - Long lock holds: {:.2} ms max",
                    health.issues.max_lock_hold_ms
                );
            }
            if health.issues.excessive_scrolling {
                crate::serial_println!("  - Excessive scrolling detected");
            }
        }

        crate::serial_println!("========================================\n");
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
    fn test_health_check_with_vga_errors() {
        let diag = SystemDiagnostics::new();

        // 多数のVGA書き込みエラーを記録
        for _ in 0..100 {
            diag.record_vga_write(false);
        }

        let health = diag.health_check();
        assert!(!health.issues.is_healthy());
        assert!(health.issues.high_vga_error_rate);
    }

    #[test]
    fn test_lock_acquisition_tracking() {
        let diag = SystemDiagnostics::new();

        for _ in 0..200 {
            diag.record_lock_acquisition();
        }
        for _ in 0..30 {
            diag.record_lock_contention();
        }

        let snap = diag.snapshot();
        assert_eq!(snap.total_lock_acquisitions, 200);
        assert_eq!(snap.lock_contentions, 30);

        let health = diag.health_check();
        assert!(health.issues.high_lock_contention);
    }
}
