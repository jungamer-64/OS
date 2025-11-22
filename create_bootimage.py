#!/usr/bin/env python3
"""
Create UEFI bootable image from kernel ELF file
"""
import subprocess
import sys
from pathlib import Path

def main():
    # Paths
    kernel_path = Path("target/x86_64-blog_os/debug/tiny_os")
    output_path = Path("target/x86_64-blog_os/debug/tiny_os.efi.img")
    
    if not kernel_path.exists():
        print(f"Error: Kernel not found at {kernel_path}")
        sys.exit(1)
    
    print(f"Creating UEFI boot image from {kernel_path}")
    
    # Use bootloader crate to create the image
    cmd = [
        "cargo", "builder",
        "--kernel-binary", str(kernel_path),
        "--out-dir", str(output_path.parent)
    ]
    
    try:
        result = subprocess.run(cmd, check=True, capture_output=True, text=True)
        print(result.stdout)
        if result.stderr:
            print("Warnings/Info:", result.stderr)
    except subprocess.CalledProcessError as e:
        print(f"Error creating boot image: {e}")
        print(e.stderr)
        sys.exit(1)
    
    if output_path.exists():
        print(f"✓ Boot image created: {output_path}")
    else:
        print("✗ Boot image was not created")
        sys.exit(1)

if __name__ == "__main__":
    main()
