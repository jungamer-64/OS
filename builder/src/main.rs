use bootloader::UefiBoot;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    // 1. Build the kernel
    let mut build_cmd = Command::new("cargo");
    build_cmd.arg("build")
        .arg("--package").arg("tiny_os")
        .arg("--bin").arg("tiny_os")
        .arg("--target").arg("x86_64-blog_os.json")
        .arg("-Z").arg("build-std=core,compiler_builtins,alloc")
        .arg("-Z").arg("build-std-features=compiler-builtins-mem");

    let status = build_cmd.status().expect("failed to execute cargo build");
    if !status.success() {
        eprintln!("Kernel build failed");
        std::process::exit(1);
    }

    // 2. Locate the kernel ELF
    let kernel_path = PathBuf::from("target/x86_64-blog_os/debug/tiny_os");

    if !kernel_path.exists() {
        eprintln!("Kernel ELF not found at {}", kernel_path.display());
        std::process::exit(1);
    }

    // 3. Create disk image path
    let disk_image = kernel_path.with_extension("efi.img");

    // 4. Create UEFI disk image
    let uefi_boot = UefiBoot::new(&kernel_path);
    uefi_boot.create_disk_image(&disk_image).expect("failed to create UEFI disk image");

    println!("Created disk image at {}", disk_image.display());

    // 5. Run QEMU (UEFI)
    // Note: This requires OVMF.fd to be available.
    // We'll try to run it, but it might fail if OVMF is not found.
    // For now, let's just verify image creation.
    let mut qemu = Command::new("qemu-system-x86_64");
    qemu.arg("-drive").arg(format!("format=raw,file={}", disk_image.display()));
    
    let ovmf_path = Path::new("ovmf-x64/OVMF.fd");
    if ovmf_path.exists() {
        qemu.arg("-bios").arg(ovmf_path);
    } else {
        println!("WARNING: OVMF firmware not found at {}. QEMU may fail to boot UEFI image.", ovmf_path.display());
        // Try to use default BIOS/UEFI if available, or just let QEMU fail/fallback
    }
    
    qemu.arg("-serial").arg("stdio");
    
    println!("Running QEMU (Note: Ensure OVMF is available if this fails to boot)...");
    let exit_status = qemu.status().expect("failed to run qemu");
    if !exit_status.success() {
       std::process::exit(exit_status.code().unwrap_or(1));
    }
}
