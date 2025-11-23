//! カーネルベンチマークフレームワーク
//!
//! TSC (Time Stamp Counter) を使用した高精度なパフォーマンス測定を提供します。

use core::fmt;

/// TSC (Time Stamp Counter) を読み取る
///
/// x86_64 の RDTSC 命令を使用してCPUサイクル数を取得します。
#[inline(always)]
pub fn read_tsc() -> u64 {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        core::arch::x86_64::_rdtsc()
    }
    
    #[cfg(not(target_arch = "x86_64"))]
    {
        // 他のアーキテクチャでは0を返す（将来的に実装）
        0
    }
}

/// ベンチマーク結果
#[derive(Debug, Clone, Copy)]
pub struct BenchmarkResult {
    /// ベンチマーク名
    pub name: &'static str,
    /// 実行に要したCPUサイクル数
    pub cycles: u64,
    /// 実行回数（平均を取る場合）
    pub iterations: u64,
}

impl BenchmarkResult {
    /// 1回あたりの平均サイクル数を計算
    #[inline]
    pub fn avg_cycles(&self) -> u64 {
        if self.iterations > 0 {
            self.cycles / self.iterations
        } else {
            self.cycles
        }
    }
}

impl fmt::Display for BenchmarkResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.iterations > 1 {
            write!(
                f,
                "{}: {} cycles (avg: {} cycles over {} iterations)",
                self.name,
                self.cycles,
                self.avg_cycles(),
                self.iterations
            )
        } else {
            write!(f, "{}: {} cycles", self.name, self.cycles)
        }
    }
}

/// 関数を1回実行してベンチマーク
///
/// # Examples
///
/// ```no_run
/// use tiny_os::kernel::bench::benchmark;
///
/// let result = benchmark("memory_allocation", || {
///     // ベンチマーク対象の処理
/// });
/// println!("{}", result);
/// ```
#[inline(never)]
pub fn benchmark<F>(name: &'static str, f: F) -> BenchmarkResult
where
    F: FnOnce(),
{
    let start = read_tsc();
    f();
    let end = read_tsc();

    BenchmarkResult {
        name,
        cycles: end.saturating_sub(start),
        iterations: 1,
    }
}

/// 関数を複数回実行してベンチマーク（平均を取る）
///
/// # Examples
///
/// ```no_run
/// use tiny_os::kernel::bench::benchmark_avg;
///
/// let result = benchmark_avg("memory_allocation", 100, || {
///     // ベンチマーク対象の処理
/// });
/// println!("{}", result);
/// ```
#[inline(never)]
pub fn benchmark_avg<F>(name: &'static str, iterations: u64, mut f: F) -> BenchmarkResult
where
    F: FnMut(),
{
    let start = read_tsc();
    for _ in 0..iterations {
        f();
    }
    let end = read_tsc();

    BenchmarkResult {
        name,
        cycles: end.saturating_sub(start),
        iterations,
    }
}

/// ベンチマークマクロ
///
/// # Examples
///
/// ```no_run
/// bench!("test", {
///     // ベンチマーク対象の処理
/// });
/// ```
#[macro_export]
macro_rules! bench {
    ($name:expr, $body:expr) => {{
        use $crate::kernel::bench::benchmark;
        let result = benchmark($name, || $body);
        $crate::println!("[BENCH] {}", result);
        result
    }};
}

/// 平均ベンチマークマクロ
///
/// # Examples
///
/// ```no_run
/// bench_avg!("test", 100, {
///     // ベンチマーク対象の処理
/// });
/// ```
#[macro_export]
macro_rules! bench_avg {
    ($name:expr, $iterations:expr, $body:expr) => {{
        use $crate::kernel::bench::benchmark_avg;
        let result = benchmark_avg($name, $iterations, || $body);
        $crate::println!("[BENCH] {}", result);
        result
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn test_read_tsc() {
        let tsc1 = read_tsc();
        let tsc2 = read_tsc();
        
        // TSCは単調増加するはず（ただしアーキテクチャによっては0の場合もある）
        #[cfg(target_arch = "x86_64")]
        assert!(tsc2 >= tsc1);
    }

    #[test_case]
    fn test_benchmark() {
        let result = benchmark("test_noop", || {
            // 何もしない
        });
        
        assert_eq!(result.name, "test_noop");
        assert_eq!(result.iterations, 1);
        // サイクル数は0以上
        assert!(result.cycles >= 0);
    }

    #[test_case]
    fn test_benchmark_avg() {
        let result = benchmark_avg("test_avg", 10, || {
            // 何もしない
        });
        
        assert_eq!(result.name, "test_avg");
        assert_eq!(result.iterations, 10);
        assert!(result.cycles >= 0);
        assert!(result.avg_cycles() >= 0);
    }
}
