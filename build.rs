// build.rs

//! Build script for the Rust OS kernel
//!
//! This script runs at build time to:
//! - Validate build environment
//! - Set up linker configuration
//! - Generate build information
//! - Validate target specifications

use std::env;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

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
    // Check for nightly toolchain
    let rustc_version = env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    println!("cargo:rustc-env=RUSTC_PATH={}", rustc_version);

    // Verify we're using nightly (required for unstable features)
    let channel = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if !channel.is_empty() {
        println!("cargo:warning=Build channel: {}", channel);
    }

    // Check for rust-src component
    let sysroot = env::var("RUSTUP_TOOLCHAIN").unwrap_or_else(|_| "unknown".to_string());
    println!("cargo:rustc-env=RUST_SYSROOT={}", sysroot);
}

/// Validate the target specification file
///
/// Ensures the target JSON is well-formed and contains required fields.
fn validate_target_spec() {
    let target_path = Path::new("x86_64-blog_os.json");

    if !target_path.exists() {
        panic!("Target specification file not found: x86_64-blog_os.json");
    }

    // Read and validate JSON (simple string checks to avoid dependencies)
    match std::fs::read_to_string(target_path) {
        Ok(content) => {
            // Basic validation - check for required fields
            let required_fields = [
                "llvm-target",
                "data-layout",
                "arch",
                "target-pointer-width",
                "disable-redzone",
                "panic-strategy",
            ];

            for field in &required_fields {
                if !content.contains(field) {
                    println!("cargo:warning=Target spec may be missing field: {}", field);
                }
            }

            // Validate critical settings
            if !content.contains("\"panic-strategy\": \"abort\"") {
                println!("cargo:warning=Panic strategy should be 'abort' for kernel");
            }

            if !content.contains("\"disable-redzone\": true") {
                println!("cargo:warning=disable-redzone should be true for kernel");
            }
        }
        Err(e) => {
            panic!("Failed to read target specification: {}", e);
        }
    }
}

/// Print build information
///
/// Displays useful information about the build configuration.
fn print_build_info() {
    // Profile (debug/release)
    let profile = env::var("PROFILE").unwrap_or_else(|_| "unknown".to_string());
    println!("cargo:rustc-env=BUILD_PROFILE={}", profile);

    // Target triple
    let target = env::var("TARGET").unwrap_or_else(|_| "unknown".to_string());
    println!("cargo:rustc-env=BUILD_TARGET={}", target);

    // Build timestamp (seconds since UNIX_EPOCH)
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string());
    println!("cargo:rustc-env=BUILD_TIMESTAMP={}", timestamp);

    // Git commit (if available)
    if let Ok(output) = std::process::Command::new("git")
        .args(&["rev-parse", "--short", "HEAD"])
        .output()
    {
        if output.status.success() {
            if let Ok(commit) = String::from_utf8(output.stdout) {
                let commit = commit.trim();
                println!("cargo:rustc-env=BUILD_COMMIT={}", commit);
            }
        }
    }

    // Warning for debug builds
    if profile == "debug" {
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

// Additional dependencies for build script
//
// Add these to Cargo.toml [build-dependencies] if needed:
// ```toml
// [build-dependencies]
// serde_json = "1.0"
// chrono = "0.4"
// ```
//
// For now, we use a simplified version without these dependencies.
