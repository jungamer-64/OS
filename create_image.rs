use std::path::PathBuf;
use bootloader::UefiBoot;

fn main() {
    let kernel_path = PathBuf::from("target/x86_64-blog_os/debug/tiny_os");
    let disk_image = PathBuf::from("target/x86_64-blog_os/debug/tiny_os.efi.img");

    if !kernel_path.exists() {
        eprintln!("Kernel not found at {}", kernel_path.display());
        std::process::exit(1);
    }

    println!("Creating UEFI boot image...");
    let uefi_boot = UefiBoot::new(&kernel_path);
    uefi_boot.create_disk_image(&disk_image)
        .expect("failed to create UEFI disk image");

    println!("✓ Created disk image at {}", disk_image.display());
}
