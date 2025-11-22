// src/serial/timeout.rs

//! Timeout handling for serial operations
//!
//! Provides robust timeout mechanisms that:
//! - Prevent infinite loops on hardware failures
//! - Allow graceful degradation
//! - Provide diagnostic information

use core::sync::atomic::{AtomicU64, Ordering};

/// Timeout configuration
#[derive(Debug, Clone, Copy)]
pub struct TimeoutConfig {
    /// Maximum iterations for polling operations
    pub max_iterations: u32,
    /// Backoff strategy
    pub backoff: BackoffStrategy,
}

impl TimeoutConfig {
    /// Default timeout (balanced for most hardware)
    pub const fn default_timeout() -> Self {
        Self {
            max_iterations: 1000,
            backoff: BackoffStrategy::Linear,
        }
    }

    /// Short timeout for quick operations
    pub const fn short_timeout() -> Self {
        Self {
            max_iterations: 100,
            backoff: BackoffStrategy::None,
        }
    }

    /// Long timeout for slow hardware
    pub const fn long_timeout() -> Self {
        Self {
            max_iterations: 10000,
            backoff: BackoffStrategy::Exponential { base: 2, max: 100 },
        }
    }
}

/// Backoff strategy for polling operations
#[derive(Debug, Clone, Copy)]
pub enum BackoffStrategy {
    /// No backoff, busy-wait
    None,
    /// Linear backoff (wait n iterations)
    Linear,
    /// Exponential backoff with max
    Exponential { base: u32, max: u32 },
}

/// Timeout context for tracking operation progress
pub struct TimeoutContext {
    config: TimeoutConfig,
    iteration: u32,
    total_waits: u64,
}

impl TimeoutContext {
    /// Create a new timeout context
    pub fn new(config: TimeoutConfig) -> Self {
        Self {
            config,
            iteration: 0,
            total_waits: 0,
        }
    }

    /// Check if timeout has been reached
    pub fn is_expired(&self) -> bool {
        self.iteration >= self.config.max_iterations
    }

    /// Perform one iteration with backoff
    ///
    /// Returns false if timeout expired
    pub fn tick(&mut self) -> bool {
        if self.is_expired() {
            TIMEOUT_STATS.record_timeout();
            return false;
        }

        self.iteration += 1;

        // Apply backoff
        let wait_iterations = self.calculate_backoff();
        self.total_waits += wait_iterations as u64;

        for _ in 0..wait_iterations {
            core::hint::spin_loop();
        }

        true
    }

    /// Calculate backoff delay for current iteration
    fn calculate_backoff(&self) -> u32 {
        match self.config.backoff {
            BackoffStrategy::None => 0,
            BackoffStrategy::Linear => self.iteration,
            BackoffStrategy::Exponential { base, max } => {
                let exp_value = base.saturating_pow(self.iteration);
                exp_value.min(max)
            }
        }
    }

    /// Get current iteration count
    pub fn iteration(&self) -> u32 {
        self.iteration
    }

    /// Get total wait cycles performed
    pub fn total_waits(&self) -> u64 {
        self.total_waits
    }

    /// Get remaining iterations
    pub fn remaining(&self) -> u32 {
        self.config.max_iterations.saturating_sub(self.iteration)
    }
}

/// Timeout operation result
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeoutResult<T> {
    /// Operation completed successfully
    Ok(T),
    /// Operation timed out
    Timeout { iterations: u32, total_waits: u64 },
}

impl<T> TimeoutResult<T> {
    /// Convert to standard Result
    pub fn into_result(self) -> Result<T, TimeoutError> {
        match self {
            TimeoutResult::Ok(v) => Ok(v),
            TimeoutResult::Timeout {
                iterations,
                total_waits,
            } => Err(TimeoutError {
                iterations,
                total_waits,
            }),
        }
    }

    /// Check if operation succeeded
    pub fn is_ok(&self) -> bool {
        matches!(self, TimeoutResult::Ok(_))
    }

    /// Check if operation timed out
    pub fn is_timeout(&self) -> bool {
        matches!(self, TimeoutResult::Timeout { .. })
    }
}

/// Timeout error with diagnostic information
#[derive(Debug, Clone, Copy)]
pub struct TimeoutError {
    pub iterations: u32,
    pub total_waits: u64,
}

impl core::fmt::Display for TimeoutError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "operation timeout after {} iterations ({} wait cycles)",
            self.iterations, self.total_waits
        )
    }
}

/// Poll a condition with timeout
///
/// # Example
///
/// ```
/// let result = poll_with_timeout(
///     TimeoutConfig::default_timeout(),
///     || hardware_ready()
/// );
/// ```
pub fn poll_with_timeout<F>(config: TimeoutConfig, mut condition: F) -> TimeoutResult<()>
where
    F: FnMut() -> bool,
{
    let mut ctx = TimeoutContext::new(config);

    while ctx.tick() {
        if condition() {
            return TimeoutResult::Ok(());
        }
    }

    TimeoutResult::Timeout {
        iterations: ctx.iteration(),
        total_waits: ctx.total_waits(),
    }
}

/// Poll a condition with timeout and return a value
///
/// The condition function returns Option<T>:
/// - Some(value) indicates success
/// - None indicates not ready yet
pub fn poll_with_timeout_value<F, T>(config: TimeoutConfig, mut condition: F) -> TimeoutResult<T>
where
    F: FnMut() -> Option<T>,
{
    let mut ctx = TimeoutContext::new(config);

    while ctx.tick() {
        if let Some(value) = condition() {
            return TimeoutResult::Ok(value);
        }
    }

    TimeoutResult::Timeout {
        iterations: ctx.iteration(),
        total_waits: ctx.total_waits(),
    }
}

/// Global timeout statistics
struct TimeoutStats {
    timeouts: AtomicU64,
    successful_polls: AtomicU64,
}

impl TimeoutStats {
    const fn new() -> Self {
        Self {
            timeouts: AtomicU64::new(0),
            successful_polls: AtomicU64::new(0),
        }
    }

    fn record_timeout(&self) {
        self.timeouts.fetch_add(1, Ordering::Relaxed);
    }

    fn record_success(&self) {
        self.successful_polls.fetch_add(1, Ordering::Relaxed);
    }

    pub fn get_stats(&self) -> (u64, u64) {
        (
            self.timeouts.load(Ordering::Relaxed),
            self.successful_polls.load(Ordering::Relaxed),
        )
    }
}

static TIMEOUT_STATS: TimeoutStats = TimeoutStats::new();

/// Get global timeout statistics
pub fn timeout_stats() -> (u64, u64) {
    TIMEOUT_STATS.get_stats()
}

/// Record successful poll operation
pub fn record_poll_success() {
    TIMEOUT_STATS.record_success();
}

#[cfg(all(test, feature = "std-tests"))]
mod tests {
    use super::*;

    #[test]
    fn test_timeout_context_creation() {
        let ctx = TimeoutContext::new(TimeoutConfig::default_timeout());
        assert_eq!(ctx.iteration(), 0);
        assert!(!ctx.is_expired());
    }

    #[test]
    fn test_timeout_expiration() {
        let mut ctx = TimeoutContext::new(TimeoutConfig {
            max_iterations: 5,
            backoff: BackoffStrategy::None,
        });

        for _ in 0..5 {
            assert!(ctx.tick());
        }

        assert!(!ctx.tick());
        assert!(ctx.is_expired());
    }

    #[test]
    fn test_backoff_calculation() {
        let mut ctx = TimeoutContext::new(TimeoutConfig {
            max_iterations: 10,
            backoff: BackoffStrategy::Linear,
        });

        assert_eq!(ctx.calculate_backoff(), 0);
        ctx.tick();
        assert_eq!(ctx.calculate_backoff(), 1);
        ctx.tick();
        assert_eq!(ctx.calculate_backoff(), 2);
    }

    #[test]
    fn test_exponential_backoff() {
        let mut ctx = TimeoutContext::new(TimeoutConfig {
            max_iterations: 10,
            backoff: BackoffStrategy::Exponential { base: 2, max: 8 },
        });

        ctx.tick();
        assert_eq!(ctx.calculate_backoff(), 2); // 2^1
        ctx.tick();
        assert_eq!(ctx.calculate_backoff(), 4); // 2^2
        ctx.tick();
        assert_eq!(ctx.calculate_backoff(), 8); // 2^3, capped at max
        ctx.tick();
        assert_eq!(ctx.calculate_backoff(), 8); // Still capped
    }

    #[test]
    fn test_poll_with_timeout_success() {
        let mut counter = 0;
        let result = poll_with_timeout(TimeoutConfig::short_timeout(), || {
            counter += 1;
            counter >= 5
        });

        assert!(result.is_ok());
    }

    #[test]
    fn test_poll_with_timeout_failure() {
        let result = poll_with_timeout(
            TimeoutConfig {
                max_iterations: 10,
                backoff: BackoffStrategy::None,
            },
            || false,
        );

        assert!(result.is_timeout());
    }

    #[test]
    fn test_poll_with_value_success() {
        let mut counter = 0;
        let result = poll_with_timeout_value(TimeoutConfig::short_timeout(), || {
            counter += 1;
            if counter >= 3 {
                Some(42)
            } else {
                None
            }
        });

        assert!(result.is_ok());
        if let TimeoutResult::Ok(value) = result {
            assert_eq!(value, 42);
        }
    }

    #[test]
    fn test_remaining_iterations() {
        let mut ctx = TimeoutContext::new(TimeoutConfig {
            max_iterations: 10,
            backoff: BackoffStrategy::None,
        });

        assert_eq!(ctx.remaining(), 10);
        ctx.tick();
        assert_eq!(ctx.remaining(), 9);
        ctx.tick();
        assert_eq!(ctx.remaining(), 8);
    }
}

#[cfg(test)]
mod kernel_tests {
    use super::*;

    #[test_case]
    fn test_timeout_config_defaults() {
        let config = TimeoutConfig::default_timeout();
        assert_eq!(config.max_iterations, 1000);
    }

    #[test_case]
    fn test_timeout_config_short() {
        let config = TimeoutConfig::short_timeout();
        assert_eq!(config.max_iterations, 100);
    }
}

/// Default maximum number of retry attempts for standard operations
const DEFAULT_MAX_RETRIES: u32 = 3;
/// Quick retry maximum attempts for time-sensitive operations
const QUICK_MAX_RETRIES: u32 = 5;
/// Persistent retry maximum attempts for critical operations
const PERSISTENT_MAX_RETRIES: u32 = 10;

/// Default delay between retries (in spin loop iterations)
const DEFAULT_RETRY_DELAY: u32 = 1000;
/// Quick retry delay for minimal latency
const QUICK_RETRY_DELAY: u32 = 100;
/// Persistent retry delay for critical operations
const PERSISTENT_RETRY_DELAY: u32 = 5000;

/// Retry configuration for operations that may fail transiently
#[derive(Debug, Clone, Copy)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_retries: u32,
    /// Timeout config for each attempt
    pub timeout: TimeoutConfig,
    /// Delay between retries (in spin loop iterations)
    pub retry_delay: u32,
}

impl RetryConfig {
    /// Default retry configuration
    pub const fn default_retry() -> Self {
        Self {
            max_retries: DEFAULT_MAX_RETRIES,
            timeout: TimeoutConfig::default_timeout(),
            retry_delay: DEFAULT_RETRY_DELAY,
        }
    }

    /// Quick retry with minimal delay
    pub const fn quick_retry() -> Self {
        Self {
            max_retries: QUICK_MAX_RETRIES,
            timeout: TimeoutConfig::short_timeout(),
            retry_delay: QUICK_RETRY_DELAY,
        }
    }

    /// Persistent retry for critical operations
    pub const fn persistent_retry() -> Self {
        Self {
            max_retries: PERSISTENT_MAX_RETRIES,
            timeout: TimeoutConfig::long_timeout(),
            retry_delay: PERSISTENT_RETRY_DELAY,
        }
    }
}

/// Result of a retry operation
#[derive(Debug, Clone, Copy)]
pub enum RetryResult<T> {
    /// Operation succeeded
    Ok(T),
    /// All retries exhausted
    Failed {
        attempts: u32,
        last_error: TimeoutError,
    },
}

impl<T> RetryResult<T> {
    /// Convert to standard Result
    pub fn into_result(self) -> Result<T, (u32, TimeoutError)> {
        match self {
            RetryResult::Ok(v) => Ok(v),
            RetryResult::Failed {
                attempts,
                last_error,
            } => Err((attempts, last_error)),
        }
    }

    /// Check if operation succeeded
    pub fn is_ok(&self) -> bool {
        matches!(self, RetryResult::Ok(_))
    }

    /// Check if operation failed
    pub fn is_failed(&self) -> bool {
        matches!(self, RetryResult::Failed { .. })
    }
}

/// Retry an operation with timeout
///
/// # Example
///
/// ```
/// let result = retry_with_timeout(
///     RetryConfig::default_retry(),
///     || {
///         // Attempt operation
///         if hardware_ready() {
///             Some(read_data())
///         } else {
///             None
///         }
///     }
/// );
/// ```
pub fn retry_with_timeout<F, T>(config: RetryConfig, mut operation: F) -> RetryResult<T>
where
    F: FnMut() -> Option<T>,
{
    let mut last_error = None;

    for attempt in 0..=config.max_retries {
        let result = poll_with_timeout_value(config.timeout, &mut operation);

        match result {
            TimeoutResult::Ok(value) => {
                if attempt > 0 {
                    TIMEOUT_STATS.record_success();
                }
                return RetryResult::Ok(value);
            }
            TimeoutResult::Timeout {
                iterations,
                total_waits,
            } => {
                last_error = Some(TimeoutError {
                    iterations,
                    total_waits,
                });

                // Don't delay after the last attempt
                if attempt < config.max_retries {
                    for _ in 0..config.retry_delay {
                        core::hint::spin_loop();
                    }
                }
            }
        }
    }

    // last_error is guaranteed to be Some because we always execute at least one attempt
    // This match is safer than expect() and provides explicit error handling
    match last_error {
        Some(err) => RetryResult::Failed {
            attempts: config.max_retries + 1,
            last_error: err,
        },
        None => {
            // This should never happen due to loop logic, but handle gracefully
            // Use default TimeoutError if somehow last_error is None
            RetryResult::Failed {
                attempts: config.max_retries + 1,
                last_error: TimeoutError {
                    iterations: 0,
                    total_waits: 0,
                },
            }
        }
    }
}

/// Adaptive timeout that adjusts based on success rate
pub struct AdaptiveTimeout {
    base_config: TimeoutConfig,
    successes: u32,
    failures: u32,
    current_multiplier: u32,
}

impl AdaptiveTimeout {
    /// Create a new adaptive timeout
    #[must_use]
    pub const fn new(base_config: TimeoutConfig) -> Self {
        Self {
            base_config,
            successes: 0,
            failures: 0,
            current_multiplier: 100, // Start at 100% (1.0x)
        }
    }

    /// Get current timeout configuration
    #[must_use]
    #[allow(clippy::cast_possible_truncation)] // max_iterations is bounded by design
    pub fn current_config(&self) -> TimeoutConfig {
        let mut config = self.base_config;
        config.max_iterations =
            (u64::from(config.max_iterations) * u64::from(self.current_multiplier) / 100) as u32;
        config
    }

    /// Record a successful operation
    pub fn record_success(&mut self) {
        self.successes += 1;
        self.adjust_timeout();
    }

    /// Record a failed operation
    pub fn record_failure(&mut self) {
        self.failures += 1;
        self.adjust_timeout();
    }

    /// Adjust timeout based on success/failure ratio
    fn adjust_timeout(&mut self) {
        let total = self.successes + self.failures;
        if total == 0 {
            return;
        }

        let success_rate = (self.successes * 100) / total;

        // Adjust multiplier based on success rate
        self.current_multiplier = if success_rate > 95 {
            // Very high success rate: reduce timeout
            50.max(self.current_multiplier.saturating_sub(10))
        } else if success_rate > 80 {
            // Good success rate: slightly reduce timeout
            80.max(self.current_multiplier.saturating_sub(5))
        } else if success_rate < 50 {
            // Poor success rate: increase timeout significantly
            300.min(self.current_multiplier.saturating_add(50))
        } else if success_rate < 70 {
            // Moderate success rate: increase timeout
            200.min(self.current_multiplier.saturating_add(20))
        } else {
            // Acceptable success rate: keep current multiplier
            self.current_multiplier
        };

        // Reset counters periodically to adapt to changing conditions
        if total >= 100 {
            self.successes /= 2;
            self.failures /= 2;
        }
    }

    /// Get current statistics
    #[must_use]
    pub const fn stats(&self) -> (u32, u32, u32) {
        (self.successes, self.failures, self.current_multiplier)
    }

    /// Reset statistics
    pub const fn reset(&mut self) {
        self.successes = 0;
        self.failures = 0;
        self.current_multiplier = 100;
    }
}
