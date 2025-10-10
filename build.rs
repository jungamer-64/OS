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
fn validate_target_spec() {
    let target_path = Path::new("x86_64-blog_os.json");

    assert!(
        target_path.exists(),
        "Target specification file not found: x86_64-blog_os.json"
    );

    // Read and validate JSON (simple string checks to avoid dependencies)
    let content = fs::read_to_string(target_path)
        .unwrap_or_else(|e| panic!("Failed to read target specification: {e}"));

    let spec: TargetSpec = serde_json::from_str(&content)
        .unwrap_or_else(|e| panic!("Target specification is not valid JSON: {e}"));

    assert!(
        !spec.llvm_target.trim().is_empty(),
        "Target specification is missing a valid 'llvm-target' value"
    );

    assert!(
        !spec.data_layout.trim().is_empty(),
        "Target specification is missing a valid 'data-layout' value"
    );

    assert!(
        spec.arch == "x86_64",
        "Target specification has unexpected architecture '{}' (expected 'x86_64')",
        spec.arch
    );

    assert_eq!(
        spec.target_pointer_width, 64,
        "Target specification uses unsupported pointer width {} (expected 64)",
        spec.target_pointer_width
    );

    assert!(
        spec.disable_redzone,
        "Target specification must set 'disable-redzone' to true"
    );

    assert_eq!(
        spec.panic_strategy.as_str(),
        "abort",
        "Target specification must set 'panic-strategy' to 'abort'"
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
    {
        if output.status.success() {
            if let Ok(commit) = String::from_utf8(output.stdout) {
                let commit = commit.trim();
                println!("cargo:rustc-env=BUILD_COMMIT={commit}");
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
