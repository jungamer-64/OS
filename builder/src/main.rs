// builder/src/main.rs
//! Bootable image builder for Tiny OS
//!
//! Generates a UEFI bootable disk image from the kernel binary.

use std::path::PathBuf;
use std::process::Command;

fn main() {
    // カーネルバイナリのビルド
    println!("Building kernel...");
    let status = Command::new("cargo")
        .args(&["build", "--target", "x86_64-rany_os.json"])
        .status()
        .expect("Failed to build kernel");
    
    if !status.success() {
        eprintln!("Kernel build failed");
        std::process::exit(1);
    }

    // カーネルバイナリのパス
    let kernel_binary_path = PathBuf::from("target/x86_64-rany_os/debug/tiny_os");
    
    // ブートイメージの出力先
    let out_dir = PathBuf::from("target/x86_64-rany_os/debug");
    let uefi_path = out_dir.join("boot-uefi-tiny_os.img");
    let bios_path = out_dir.join("boot-bios-tiny_os.img");

    // UEFI ブートイメージの作成
    println!("Creating UEFI boot image...");
    bootloader::UefiBoot::new(&kernel_binary_path)
        .create_disk_image(&uefi_path)
        .expect("Failed to create UEFI boot image");

    // BIOS ブートイメージの作成
    println!("Creating BIOS boot image...");
    bootloader::BiosBoot::new(&kernel_binary_path)
        .create_disk_image(&bios_path)
        .expect("Failed to create BIOS boot image");

    println!("Build complete!");
    println!("  UEFI image: {}", uefi_path.display());
    println!("  BIOS image: {}", bios_path.display());
    
    // QEMUコマンドの例を表示
    println!("\nTo run in QEMU (UEFI):");
    println!("  qemu-system-x86_64 -bios OVMF.fd -drive format=raw,file={}", uefi_path.display());
    println!("\nTo run in QEMU (BIOS):");
    println!("  qemu-system-x86_64 -drive format=raw,file={}", bios_path.display());
}
