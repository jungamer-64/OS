// crates/kernel/build.rs

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

/// アセンブリファイルのメタデータ
struct AsmFile {
    source: &'static str,
    output: &'static str,
    format: &'static str, // "elf64" or "bin"
}

const ASM_FILES: &[AsmFile] = &[
    AsmFile {
        source: "src/arch/x86_64/jump_to_usermode.asm",
        output: "jump_to_usermode.o",
        format: "elf64",
    },
    AsmFile {
        source: "src/arch/x86_64/cr3_test.asm",
        output: "cr3_test.o",
        format: "elf64",
    },
];

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=.cargo/config.toml");
    println!("cargo:rerun-if-changed=linker.ld");

    // 並列アセンブリコンパイル
    compile_assembly_parallel();
    
    validate_target_env();
    setup_linker();
    print_build_info();
}

/// 並列アセンブリコンパイル（エラー報告を丁寧に）
fn compile_assembly_parallel() {
    use std::thread;

    let out_dir = env::var("OUT_DIR").expect("OUT_DIR not set");
    let out_path = PathBuf::from(&out_dir);

    // spawn 全部
    let handles: Vec<_> = ASM_FILES
        .iter()
        .map(|asm| {
            let asm_source = asm.source.to_string();
            let asm_output = asm.output.to_string();
            let asm_format = asm.format.to_string();
            let out_dir_clone = out_path.clone();

            thread::spawn(move || -> Result<(), String> {
                compile_asm_file(&asm_source, &asm_output, &asm_format, &out_dir_clone)
            })
        })
        .collect();

    // join して、スレッドパニックとコンパイル失敗を分けて報告
    let mut errors: Vec<String> = Vec::new();
    for (i, handle) in handles.into_iter().enumerate() {
        match handle.join() {
            Ok(Ok(())) => { /* 成功 */ }
            Ok(Err(e)) => {
                errors.push(format!("ASM file #{i} failed: {e}"));
            }
            Err(join_err) => {
                // スレッド自体がパニックした場合。情報を出来るだけ拾う。
                if let Some(s) = join_err.downcast_ref::<&str>() {
                    errors.push(format!("ASM thread #{i} panicked: {s}"));
                } else if let Some(s) = join_err.downcast_ref::<String>() {
                    errors.push(format!("ASM thread #{i} panicked: {s}"));
                } else {
                    errors.push(format!("ASM thread #{i} panicked with unknown payload"));
                }
            }
        }
    }

    if !errors.is_empty() {
        eprintln!("Assembly compilation encountered errors:");
        for e in &errors {
            eprintln!("  - {e}");
        }
        panic!("Assembly compilation failed (see error list above).");
    }
}

/// 単一アセンブリファイルのコンパイル（エラーメッセージを詳細に）
fn compile_asm_file(
    source: &str,
    output: &str,
    format: &str,
    out_dir: &Path,
) -> Result<(), String> {
    // ビルド依存関係として Cargo に知らせる
    println!("cargo:rerun-if-changed={source}");

    // ソース存在チェック（早期エラー）
    if !Path::new(source).exists() {
        return Err(format!("ASM source not found: {source}"));
    }

    let obj_file = out_dir.join(output);

    // NASM 実行前に存在確認（より丁寧なエラー報告）
    let nasm_check = Command::new("nasm").arg("--version").output();
    match nasm_check {
        Ok(o) if o.status.success() => { /* OK */ }
        Ok(o) => {
            // NASM が呼べたが非ゼロ終了（珍しい）
            let status = o.status;
            return Err(format!(
                "`nasm --version` failed (status: {status}). stdout/stderr may contain details."
            ));
        }
        Err(_) => {
            return Err("`nasm` not found or failed to run. Install NASM and ensure it's on PATH.".into());
        }
    }

    let obj_path_arg = obj_file.to_string_lossy().into_owned();

    let status = Command::new("nasm")
        .args(["-f", format, "-o"])
        .arg(&obj_path_arg)
        .arg(source)
        .status()
        .map_err(|e| format!("Failed to run NASM: {e}"))?;

    if !status.success() {
        let code = status.code().map_or_else(|| "unknown".into(), |c| c.to_string());
        return Err(format!("NASM exited with status {code} for {source}"));
    }

    // リンカ引数はここで出力
    let obj_display = obj_file.display();
    println!("cargo:rustc-link-arg={obj_display}");

    Ok(())
}

fn validate_target_env() {
    let target = env::var("TARGET").unwrap_or_else(|_| "unknown".to_string());
    
    if !target.contains("x86_64") {
        println!(
            "cargo:warning=Target '{target}' is not x86_64. \
            This kernel is designed for x86_64 only."
        );
    }
}

fn setup_linker() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let manifest_path = Path::new(&manifest_dir);
    
    let linker_script = find_linker_script(manifest_path)
        .expect("Could not find linker.ld");

    println!("cargo:rerun-if-changed={}", linker_script.display());
    
    // Rustcの情報を環境変数に設定
    set_rustc_env();
}

/// 指定パスから上方向に向かって linker.ld を探す（ワークスペースルートやさらに上も探索）
fn find_linker_script(start: &Path) -> Option<PathBuf> {
    let mut cur = start.to_path_buf();

    loop {
        let candidate = cur.join("linker.ld");
        if candidate.exists() {
            return Some(candidate);
        }

        // 親に遡る
        if let Some(parent) = cur.parent() {
            // parent が same as cur の可能性は基本ないが安全対策
            if parent == cur {
                break;
            }
            cur = parent.to_path_buf();
        } else {
            break;
        }
    }

    None
}

#[allow(clippy::collapsible_if)]
fn set_rustc_env() {
    let rustc = env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    println!("cargo:rustc-env=RUSTC_PATH={rustc}");

    // Rustcバージョン取得
    if let Ok(output) = Command::new(&rustc).arg("--version").output() {
        if let Ok(version_str) = String::from_utf8(output.stdout) {
            let version = version_str.trim();
            println!("cargo:rustc-env=RUSTC_VERSION={version}");
            
            if !version.contains("nightly") {
                println!("cargo:warning=Nightly toolchain required. Current: {version}");
            }
        }
    }

    // Sysroot取得
    if let Ok(output) = Command::new(&rustc).args(["--print", "sysroot"]).output() {
        if let Ok(sysroot_str) = String::from_utf8(output.stdout) {
            let sysroot = sysroot_str.trim();
            println!("cargo:rustc-env=RUST_SYSROOT={sysroot}");
            
            let rust_src = Path::new(sysroot).join("lib/rustlib/src/rust/library");
            if !rust_src.exists() {
                println!(
                    "cargo:warning=rust-src component not found. \
                    Install with: rustup component add rust-src"
                );
            }
        }
    }
}

#[allow(clippy::collapsible_if)]
fn print_build_info() {
    use std::time::{SystemTime, UNIX_EPOCH};
    
    let profile = env::var("PROFILE").unwrap_or_else(|_| "unknown".to_string());
    println!("cargo:rustc-env=BUILD_PROFILE={profile}");

    let target = env::var("TARGET").unwrap_or_else(|_| "unknown".to_string());
    println!("cargo:rustc-env=BUILD_TARGET={target}");

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    println!("cargo:rustc-env=BUILD_TIMESTAMP={timestamp}");

    // Gitコミットハッシュ
    if let Ok(output) = Command::new("git")
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

    if profile == "release" {
        println!("cargo:warning=Building RELEASE mode (optimizations enabled)");
    }
}