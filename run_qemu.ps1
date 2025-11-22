# QEMU UEFI Boot Script for tiny_os
# This script directly boots the kernel ELF using OVMF UEFI firmware

$kernelPath = "target\x86_64-blog_os\debug\tiny_os"
$ovmfPath = "ovmf-x64\OVMF.fd"

if (!(Test-Path $kernelPath)) {
    Write-Host "Error: Kernel not found at $kernelPath" -ForegroundColor Red
    Write-Host "Please run: cargo build --target x86_64-blog_os.json" -ForegroundColor Yellow
    exit 1
}

if (!(Test-Path $ovmfPath)) {
    Write-Host "Error: OVMF firmware not found at $ovmfPath" -ForegroundColor Red
    exit 1
}

Write-Host "Starting QEMU with UEFI firmware..." -ForegroundColor Cyan
Write-Host "Kernel: $kernelPath" -ForegroundColor Green
Write-Host "OVMF: $ovmfPath" -ForegroundColor Green

# Note: Direct ELF loading with OVMF requires special setup
# For now, try to boot with available disk image or create a minimal setup

# Check if disk image exists
$diskImage = "target\x86_64-blog_os\debug\tiny_os.efi.img"
if (Test-Path $diskImage) {
    Write-Host "Found disk image: $diskImage" -ForegroundColor Green
    qemu-system-x86_64 `
        -drive format=raw,file=$diskImage `
        -bios $ovmfPath `
        -serial stdio `
        -m 128M `
        -no-reboot `
        -no-shutdown
} else {
    Write-Host "Warning: No disk image found." -ForegroundColor Yellow
    Write-Host "UEFI requires a disk image with bootloader." -ForegroundColor Yellow
    Write-Host "Attempting direct kernel load (may not work with UEFI)..." -ForegroundColor Yellow
    
    qemu-system-x86_64 `
        -kernel $kernelPath `
        -bios $ovmfPath `
        -serial stdio `
        -m 128M `
        -no-reboot `
        -no-shutdown
}
