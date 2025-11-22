// build.rs

//! Build script for the Rust OS kernel
//!
//! This script runs at build time to:
//! - Validate build environment
//! - Set up linker configuration
//! - Generate build information
//! - Validate target specifications

use serde::de::{self, Deserializer};
use serde::Deserialize;
use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Deserialize)]
struct TargetSpec {
    #[serde(rename = "llvm-target")]
    llvm_target: String,
    #[serde(rename = "data-layout")]
    data_layout: String,
    arch: String,
    #[serde(
        rename = "target-pointer-width",
        deserialize_with = "deserialize_pointer_width"
    )]
    target_pointer_width: u16,
    #[serde(rename = "disable-redzone")]
    disable_redzone: bool,
    #[serde(rename = "panic-strategy")]
    panic_strategy: String,
}

fn deserialize_pointer_width<'de, D>(deserializer: D) -> Result<u16, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum PointerWidthRaw {
        Integer(u64),
        Text(String),
    }

    match PointerWidthRaw::deserialize(deserializer)? {
        PointerWidthRaw::Integer(value) => {
            u16::try_from(value).map_err(|_| de::Error::custom("target-pointer-width out of range"))
        }
        PointerWidthRaw::Text(text) => text.parse::<u16>().map_err(|_| {
            de::Error::custom(format!(
                "target-pointer-width must be numeric, received '{text}'"
            ))
        }),
    }
}

/// Validate that the architecture and pointer width are compatible
///
/// This ensures that the target specification makes sense for the
/// chosen architecture. Currently, only `x86_64` is fully implemented.
fn validate_architecture_compatibility(arch: &str, pointer_width: u16) -> bool {
    #[allow(clippy::match_same_arms)]
    match (arch, pointer_width) {
        // 64-bit architectures
        ("x86_64" | "aarch64" | "riscv64", 64) => true,
        // 32-bit architectures
        ("x86" | "arm" | "riscv32", 32) => true,
        // Unknown or incompatible combination
        _ => false,
    }
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=x86_64-blog_os.json");
    println!("cargo:rerun-if-changed=.cargo/config.toml");

    // Validate build environment
    validate_environment();

    // Check target specification
    validate_target_spec();

    // Print build information
    print_build_info();

    // Trigger bootloader build if needed
    setup_bootloader();
}

/// Validate the build environment
///
/// Checks for required tools and configurations.
fn validate_environment() {
    let rustc = env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    println!("cargo:rustc-env=RUSTC_PATH={rustc}");

    let rustc_version_output = Command::new(&rustc)
        .arg("--version")
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok());

    if let Some(version) = rustc_version_output.as_deref() {
        let version = version.trim();
        println!("cargo:rustc-env=RUSTC_VERSION={version}");
        if !version.contains("nightly") {
            println!(
                "cargo:warning=Rust nightly toolchain not detected (reported version: {version})."
            );
        }
    } else {
        println!("cargo:warning=Failed to determine rustc version via '{rustc} --version'.");
    }

    let sysroot_path = Command::new(&rustc)
        .args(["--print", "sysroot"])
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|s| s.trim().to_string());

    if let Some(sysroot) = sysroot_path {
        println!("cargo:rustc-env=RUST_SYSROOT={sysroot}");
        let rust_src = Path::new(&sysroot).join("lib/rustlib/src/rust/library");
        if !rust_src.exists() {
            println!(
                "cargo:warning=rust-src component not found at {}. install with `rustup component add rust-src`.",
                rust_src.display()
            );
        }
    } else {
        println!("cargo:warning=Failed to determine rustc sysroot via '{rustc} --print sysroot'.");
    }
}

/// Validate the target specification file
///
/// Ensures the target JSON is well-formed and contains required fields.
/// Uses the TARGET environment variable to determine which target spec to validate.
fn validate_target_spec() {
    let target = env::var("TARGET").unwrap_or_else(|_| "x86_64-blog_os".to_string());
    
    // Try custom target JSON first, fall back to default for built-in targets
    let target_filename = format!("{target}.json");
    let target_path = Path::new(&target_filename);

    if !target_path.exists() {
        // Built-in target (e.g., x86_64-unknown-linux-gnu), no validation needed
        println!("cargo:warning=Using built-in target '{target}' (no custom target spec)");
        return;
    }

    // Read and validate custom target JSON
    let content = fs::read_to_string(target_path)
        .unwrap_or_else(|e| panic!("Failed to read target specification: {e}"));

    let spec: TargetSpec = serde_json::from_str(&content)
        .unwrap_or_else(|e| panic!("Target specification is not valid JSON: {e}"));
    
    println!("cargo:rustc-env=TARGET_ARCH={}", spec.arch);

    assert!(
        !spec.llvm_target.trim().is_empty(),
        "Target specification is missing a valid 'llvm-target' value"
    );

    assert!(
        !spec.data_layout.trim().is_empty(),
        "Target specification is missing a valid 'data-layout' value"
    );

    // Validate architecture is specified (but don't restrict which architecture)
    assert!(
        !spec.arch.trim().is_empty(),
        "Target specification is missing a valid 'arch' value"
    );

    // Validate pointer width matches architecture expectations
    // Most common values: 32 (x86, ARM32), 64 (x86_64, ARM64, RISC-V 64)
    assert!(
        spec.target_pointer_width == 32 || spec.target_pointer_width == 64,
        "Target specification uses unsupported pointer width {} (expected 32 or 64)",
        spec.target_pointer_width
    );

    // Validate architecture and pointer width compatibility
    assert!(
        validate_architecture_compatibility(&spec.arch, spec.target_pointer_width),
        "Architecture '{}' is incompatible with pointer width {}",
        spec.arch,
        spec.target_pointer_width
    );

    // Kernel code on x86/x86_64 requires red-zone to be disabled
    // Other architectures may not have a red-zone concept
    #[cfg(any(target_arch = "x86_64", target_arch = "x86"))]
    assert!(
        spec.disable_redzone,
        "Target specification must set 'disable-redzone' to true for x86/x86_64 kernel code"
    );

    // Kernel panic strategy must be abort (no unwinding support in no_std)
    assert_eq!(
        spec.panic_strategy.as_str(),
        "abort",
        "Target specification must set 'panic-strategy' to 'abort' for kernel code"
    );
}

/// Print build information
///
/// Displays useful information about the build configuration.
fn print_build_info() {
    // Profile (debug/release)
    let profile = env::var("PROFILE").unwrap_or_else(|_| "unknown".to_string());
    println!("cargo:rustc-env=BUILD_PROFILE={profile}");

    // Target triple
    let target = env::var("TARGET").unwrap_or_else(|_| "unknown".to_string());
    println!("cargo:rustc-env=BUILD_TARGET={target}");

    // Build timestamp (seconds since UNIX_EPOCH)
    let timestamp_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_secs();
    println!("cargo:rustc-env=BUILD_TIMESTAMP={timestamp_secs}");

    // Git commit (if available)
    if let Ok(output) = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        && output.status.success()
        && let Ok(commit) = String::from_utf8(output.stdout)
    {
        let commit = commit.trim();
        println!("cargo:rustc-env=BUILD_COMMIT={commit}");
    }

    // Optimization level reporting
    if profile == "release" {
        println!("cargo:warning=Building in RELEASE mode with optimizations");
        println!("cargo:warning=  - LTO: fat (maximum cross-crate optimization)");
        println!("cargo:warning=  - opt-level: 3 (maximum performance)");
        println!("cargo:warning=  - codegen-units: 1 (better optimization)");
    } else if profile == "debug" {
        println!("cargo:warning=Building in DEBUG mode - performance will be limited");
        println!("cargo:warning=Use --release for production builds");
    }
}

/// Setup bootloader configuration
///
/// The bootloader crate handles most of its configuration automatically,
/// but we can trigger rebuilds when needed.
fn setup_bootloader() {
    // Bootloader will be built as a dependency
    // We don't need to do anything special here

    // Rerun if bootloader config changes
    println!("cargo:rerun-if-changed=Cargo.toml");
}
