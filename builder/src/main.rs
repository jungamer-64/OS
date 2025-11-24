// builder/src/main.rs
//! Bootable image builder for Tiny OS
//!
//! Generates a UEFI bootable disk image from the kernel binary.

use std::path::PathBuf;
use std::process::Command;

fn main() {
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
    
    let mkcpio_dir = root_dir.join("tools/mkcpio");
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
