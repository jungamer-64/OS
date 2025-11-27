// builder/src/main.rs
//! Bootable image builder for Tiny OS
//!
//! Generates a UEFI bootable disk image from the kernel binary.
//!
//! # Usage
//!
//! ## Full Build Mode (default)
//! ```
//! cargo run --release
//! ```
//! Builds userland programs, creates initrd, builds kernel, and creates boot image.
//!
//! ## Quick Image Mode (specify kernel path directly)
//! ```
//! cargo run --release -- --kernel-path <KERNEL_ELF> --output-path <OUTPUT_IMG>
//! ```
//! Creates a UEFI boot image from an existing kernel ELF.

use std::path::PathBuf;
use std::process::Command;

/// Command line arguments for quick image creation mode
struct QuickImageArgs {
    kernel_path: PathBuf,
    output_path: PathBuf,
    ramdisk_path: Option<PathBuf>,
}

fn parse_args() -> Option<QuickImageArgs> {
    let args: Vec<String> = std::env::args().collect();
    
    let mut kernel_path = None;
    let mut output_path = None;
    let mut ramdisk_path = None;
    
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--kernel-path" if i + 1 < args.len() => {
                kernel_path = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--output-path" if i + 1 < args.len() => {
                output_path = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--ramdisk" if i + 1 < args.len() => {
                ramdisk_path = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            _ => i += 1,
        }
    }
    
    // If both kernel and output paths are provided, use quick mode
    if let (Some(kernel), Some(output)) = (kernel_path, output_path) {
        Some(QuickImageArgs {
            kernel_path: kernel,
            output_path: output,
            ramdisk_path,
        })
    } else {
        None
    }
}

fn print_help() {
    println!("Tiny OS Boot Image Builder");
    println!();
    println!("USAGE:");
    println!("  builder                                    Full build (userland + kernel + image)");
    println!("  builder --kernel-path <ELF> --output-path <IMG>  Quick image creation");
    println!();
    println!("OPTIONS:");
    println!("  --kernel-path <PATH>   Path to the kernel ELF binary");
    println!("  --output-path <PATH>   Output path for the UEFI boot image");
    println!("  --ramdisk <PATH>       Optional: Path to ramdisk/initrd file");
    println!("  -h, --help             Print this help message");
}

/// Quick image creation mode - just creates boot image from existing kernel
fn quick_image_mode(args: QuickImageArgs) {
    if !args.kernel_path.exists() {
        eprintln!("Kernel ELF not found at {}", args.kernel_path.display());
        std::process::exit(1);
    }
    
    println!("Creating UEFI boot image...");
    println!("  Kernel: {}", args.kernel_path.display());
    println!("  Output: {}", args.output_path.display());
    
    let mut uefi_boot = bootloader::UefiBoot::new(&args.kernel_path);
    
    if let Some(ref ramdisk) = args.ramdisk_path {
        if !ramdisk.exists() {
            eprintln!("Ramdisk not found at {}", ramdisk.display());
            std::process::exit(1);
        }
        println!("  Ramdisk: {}", ramdisk.display());
        uefi_boot.set_ramdisk(ramdisk);
    }
    
    uefi_boot
        .create_disk_image(&args.output_path)
        .expect("Failed to create UEFI disk image");
    
    println!("Created EFI image at {}", args.output_path.display());
}

fn main() {
    // Check for quick image mode
    if let Some(args) = parse_args() {
        quick_image_mode(args);
        return;
    }
    
    // Full build mode
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let builder_dir = PathBuf::from(manifest_dir);
    let root_dir = builder_dir.parent().unwrap();
    
    // 1. Build Userland Programs
    println!("Building userland programs...");
    let user_programs = ["shell", "init", "syscall_test"];
    
    for prog in user_programs {
        println!("  Building {}...", prog);
        let status = Command::new("cargo")
            .current_dir(root_dir.join("userland/programs").join(prog))
            .args(&["build", "--release", "--target", "x86_64-unknown-none"])
            .status()
            .expect("Failed to build userland program");
            
        if !status.success() {
            eprintln!("Failed to build {}", prog);
            std::process::exit(1);
        }
    }
    
    // 2. Prepare Initrd Directory
    println!("Preparing initrd content...");
    let initrd_root = root_dir.join("target/initrd_root");
    if initrd_root.exists() {
        std::fs::remove_dir_all(&initrd_root).expect("Failed to clean initrd root");
    }
    std::fs::create_dir_all(initrd_root.join("bin")).expect("Failed to create initrd bin dir");
    
    // Copy binaries
    for prog in user_programs {
        let src = root_dir.join("target/x86_64-unknown-none/release").join(prog);
        let dst = initrd_root.join("bin").join(prog);
        std::fs::copy(&src, &dst).expect("Failed to copy userland binary");
    }
    
    // 3. Create Initrd CPIO
    let initrd_path = root_dir.join("target/initrd.cpio");
    println!("Creating initrd archive at {}", initrd_path.display());
    
    let mkcpio_dir = root_dir.join("tools/build/mkcpio");
    let status = Command::new("cargo")
        .current_dir(&mkcpio_dir)
        .env("CARGO_BUILD_TARGET", "") // Override workspace default target
        .args(&["run", "--release", "--", initrd_root.to_str().unwrap(), initrd_path.to_str().unwrap()])
        .status()
        .expect("Failed to run mkcpio");
        
    if !status.success() {
        eprintln!("Failed to create initrd");
        std::process::exit(1);
    }

    // 4. Build Kernel
    println!("Building kernel...");
    let status = Command::new("cargo")
        .current_dir(root_dir)
        .args(&["build", "--package", "tiny_os", "--target", "x86_64-rany_os.json"])
        .status()
        .expect("Failed to build kernel");
    
    if !status.success() {
        eprintln!("Kernel build failed");
        std::process::exit(1);
    }

    // カーネルバイナリのパス
    let kernel_binary_path = root_dir.join("target/x86_64-rany_os/debug/tiny_os");
    
    // ブートイメージの出力先
    let out_dir = root_dir.join("target/x86_64-rany_os/debug");
    let uefi_path = out_dir.join("boot-uefi-tiny_os.img");
    let bios_path = out_dir.join("boot-bios-tiny_os.img");

    // UEFI ブートイメージの作成
    println!("Creating UEFI boot image...");
    bootloader::UefiBoot::new(&kernel_binary_path)
        .set_ramdisk(&initrd_path)
        .create_disk_image(&uefi_path)
        .expect("Failed to create UEFI boot image");

    // Skip BIOS build due to bootloader issues with 16-bit stage size
    // BIOS boot is not needed for development/testing
    // println!("Creating BIOS boot image...");
    // bootloader::BiosBoot::new(&kernel_binary_path)
    //     .set_ramdisk(&initrd_path)
    //     .create_disk_image(&bios_path)
    //     .expect("Failed to create BIOS boot image");

    println!("Build complete!");
    println!("  UEFI image: {}", uefi_path.display());
    
    // QEMUコマンドの例を表示
    println!("\nTo run in QEMU (UEFI):");
    println!("  run_qemu.ps1");
}
