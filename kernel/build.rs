// kernel/build.rs

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
fn validate_architecture_compatibility(arch: &str, pointer_width: u16) -> bool {
    #[allow(clippy::match_same_arms)]
    match (arch, pointer_width) {
        ("x86_64" | "aarch64" | "riscv64", 64) => true,
        ("x86" | "arm" | "riscv32", 32) => true,
        _ => false,
    }
}

/// Compile assembly files
///
/// Assembles .asm files using NASM and links them with the kernel.
/// Compile assembly files with NASM
fn compile_assembly() {
    use std::process::Command;
    use std::path::PathBuf;
    
    let out_dir = env::var("OUT_DIR").expect("OUT_DIR not set");
    // Path is relative to kernel directory (where Cargo.toml is)
    let asm_file = PathBuf::from("../src/arch/x86_64/jump_to_usermode.asm");
    let obj_file = PathBuf::from(&out_dir).join("jump_to_usermode.o");
    
    println!("cargo:rerun-if-changed=../src/arch/x86_64/jump_to_usermode.asm");
    
    // Compile assembly with NASM
    // Use ELF64 format for rust-lld (GNU flavor)
    let status = Command::new("nasm")
        .args([
            "-f", "elf64",           // ELF 64-bit format (for rust-lld)
            "-o", obj_file.to_str().unwrap(),
            asm_file.to_str().unwrap(),
        ])
        .status()
        .expect("Failed to run NASM");
    
    assert!(status.success(), "NASM compilation failed!");
    
    // Link the object file directly
    println!("cargo:rustc-link-arg={}", obj_file.display());
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=x86_64-rany_os.json");
    println!("cargo:rerun-if-changed=.cargo/config.toml");

    // Validate build environment
    validate_environment();

    // Check target specification
    validate_target_spec();

    // Print build information
    print_build_info();

    // Compile assembly files
    compile_assembly();

    // Setup linker configuration (NEW: ここを変更)
    // リンカースクリプトの絶対パスを解決して cargo に伝えます
    setup_linker();

    // Build userland shell
    build_userland();
}

/// Configure the linker script
///
/// Finds the linker script (linker.ld) and passes its absolute path
/// to the linker. This is crucial when running builds from subdirectories.
fn setup_linker() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let manifest_path = Path::new(&manifest_dir);
    
    // Attempt to find linker.ld in the workspace root (one level up)
    // or in the current kernel directory.
    let possible_paths = [
        manifest_path.parent().unwrap().join("linker.ld"), // ../linker.ld (Workspace root)
        manifest_path.join("linker.ld"),                   // ./linker.ld (Kernel dir)
    ];
    
    let linker_script = possible_paths.iter()
        .find(|p| p.exists())
        .expect("Could not find linker.ld in workspace root or kernel directory!");

    println!("cargo:rerun-if-changed={}", linker_script.display());
    
    // Pass the ABSOLUTE path to the linker
    // This avoids "cannot find linker script" errors when running from subdirectories
    println!("cargo:rustc-link-arg=-T{}", linker_script.display());
}

/// Build the userland shell and convert it to a flat binary
fn build_userland() {
    let root = env::var("CARGO_MANIFEST_DIR").unwrap();
    let root_path = Path::new(&root);
    // Navigate up to workspace root then to shell dir
    let shell_dir = root_path.parent().unwrap().join("userland/programs/shell");
    
    println!("cargo:rerun-if-changed={}", shell_dir.join("src").display());
    println!("cargo:rerun-if-changed={}", shell_dir.join("Cargo.toml").display());

    // Check both local target dir and workspace target dir
    let local_target = shell_dir.join("target/x86_64-unknown-none/release/shell");
    let workspace_target = root_path.parent().unwrap().join("target/x86_64-unknown-none/release/shell");
    
    // 値を返す際は .clone() して所有権移動を防ぐ
    let elf_path = if local_target.exists() {
        local_target.clone()
    } else if workspace_target.exists() {
        workspace_target.clone()
    } else {
        // エラーメッセージでパスを表示するために、変数が移動していてはいけない
        Path::new("").to_path_buf() 
    };
    
    let bin_path = root_path.join("src/shell.bin");
    
    if !elf_path.exists() {
        println!("cargo:warning=Shell ELF not found at {} or {}. Using dummy binary.", local_target.display(), workspace_target.display());
        println!("cargo:warning=Run 'cargo build -p shell --release' before building the kernel.");
        create_dummy_binary(bin_path);
        return;
    }

    
    // Try to convert to binary using rust-objcopy
    let objcopy_commands = ["rust-objcopy", "llvm-objcopy", "objcopy"];
    let mut converted = false;
    
    for cmd in objcopy_commands {
        let status = Command::new(cmd)
            .args(["--output-target=binary", elf_path.to_str().unwrap(), bin_path.to_str().unwrap()])
            .status();
            
        if let Ok(s) = status {
            if s.success() {
                println!("cargo:warning=Successfully created shell.bin using {}", cmd);
                converted = true;
                break;
            }
        }
    }
    
    if !converted {
        println!("cargo:warning=Could not convert shell to binary (objcopy not found).");
        println!("cargo:warning=Using dummy shell binary (infinite loop).");
        create_dummy_binary(bin_path);
    }
}

fn create_dummy_binary(path: std::path::PathBuf) {
    let dummy_code: [u8; 2] = [0xeb, 0xfe];
    if let Err(e) = fs::write(&path, dummy_code) {
        println!("cargo:warning=Failed to write dummy binary: {}", e);
    }
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
    
    let target_filename = format!("{target}.json");
    let mut target_path = std::path::PathBuf::from(&target_filename);

    if !target_path.exists() {
        if let Some(name) = std::path::Path::new(&target).file_name() {
            let local_name = format!("{}.json", name.to_string_lossy());
            if std::path::Path::new(&local_name).exists() {
                target_path = std::path::PathBuf::from(local_name);
            } else {
                println!("cargo:warning=Using built-in target '{target}' (no custom target spec found)");
                return;
            }
        } else {
             println!("cargo:warning=Using built-in target '{target}'");
             return;
        }
    }

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
    let profile = env::var("PROFILE").unwrap_or_else(|_| "unknown".to_string());
    println!("cargo:rustc-env=BUILD_PROFILE={profile}");

    let target = env::var("TARGET").unwrap_or_else(|_| "unknown".to_string());
    println!("cargo:rustc-env=BUILD_TARGET={target}");

    let timestamp_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_secs();
    println!("cargo:rustc-env=BUILD_TIMESTAMP={timestamp_secs}");

    if let Ok(output) = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        && output.status.success()
        && let Ok(commit) = String::from_utf8(output.stdout)
    {
        let commit = commit.trim();
        println!("cargo:rustc-env=BUILD_COMMIT={commit}");
    }

    if profile == "release" {
        println!("cargo:warning=Building in RELEASE mode with optimizations");
    } else if profile == "debug" {
        println!("cargo:warning=Building in DEBUG mode - performance will be limited");
    }
}