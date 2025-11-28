//! CPU feature detection using raw-cpuid
//!
//! This module provides a centralized way to detect and cache CPU features
//! using the raw-cpuid crate. This improves code readability and reduces
//! redundant CPUID calls.

use raw_cpuid::CpuId;
use spin::{Mutex, Lazy};

/// CPU feature information
#[derive(Debug, Clone, Copy)]
pub struct CpuFeatures {
    /// x87 FPU support
    pub has_fpu: bool,
    /// SSE support
    pub has_sse: bool,
    /// SSE2 support
    pub has_sse2: bool,
    /// AVX support
    pub has_avx: bool,
    /// XSAVE/XRSTOR support
    pub has_xsave: bool,
    /// Time Stamp Counter support
    pub has_tsc: bool,
}

/// Cached CPU features
static CPU_FEATURES: Lazy<Mutex<Option<CpuFeatures>>> = Lazy::new(|| Mutex::new(None));

/// Detect CPU features and cache the results
///
/// This function queries the CPU using CPUID and caches the results.
/// Subsequent calls will return the cached data without re-querying.
///
/// # Examples
///
/// ```no_run
/// use crate::arch::x86_64::cpu_features;
///
/// let features = cpu_features::detect();
/// if features.has_avx {
///     println!("AVX is supported");
/// }
/// ```
pub fn detect() -> CpuFeatures {
    let mut cache = CPU_FEATURES.lock();
    
    if let Some(features) = *cache {
        return features;
    }
    
    let cpuid = CpuId::new();
    
    // Get feature information once
    let feature_info = cpuid.get_feature_info();
    
    let features = CpuFeatures {
        has_fpu: feature_info.as_ref().map_or(false, |f| f.has_fpu()),
        has_sse: feature_info.as_ref().map_or(false, |f| f.has_sse()),
        has_sse2: feature_info.as_ref().map_or(false, |f| f.has_sse2()),
        has_avx: feature_info.as_ref().map_or(false, |f| f.has_avx()),
        has_xsave: feature_info.as_ref().map_or(false, |f| f.has_xsave()),
        has_tsc: feature_info.as_ref().map_or(false, |f| f.has_tsc()),
    };
    
    *cache = Some(features);
    features
}

/// Get cached CPU features
///
/// Returns the cached CPU features if they have been detected,
/// otherwise detects and caches them first.
///
/// This is a convenience function equivalent to calling `detect()`.
pub fn get() -> CpuFeatures {
    detect()
}
