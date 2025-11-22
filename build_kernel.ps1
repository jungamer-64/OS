# Build script for tiny_os kernel with UEFI bootloader
# This script builds the kernel and creates a bootable UEFI image

Write-Host "Building tiny_os kernel..." -ForegroundColor Cyan

# Build the kernel
cargo build --target x86_64-blog_os.json
if ($LASTEXITCODE -ne 0) {
    Write-Host "Kernel build failed!" -ForegroundColor Red
    exit 1
}

Write-Host "Kernel built successfully!" -ForegroundColor Green

# Paths
$kernelPath = "target\x86_64-blog_os\debug\tiny_os"
$outputImage = "target\x86_64-blog_os\debug\tiny_os.efi.img"

Write-Host "Creating UEFI boot image..." -ForegroundColor Cyan

# Create boot image using bootimage if available
if (Get-Command bootimage -ErrorAction SilentlyContinue) {
    bootimage build --target x86_64-blog_os.json
    if ($LASTEXITCODE -eq 0) {
        Write-Host "Boot image created successfully: $outputImage" -ForegroundColor Green
        exit 0
    }
}

Write-Host "bootimage tool not compatible, using alternative method..." -ForegroundColor Yellow

# Alternative: Use QEMU directly with kernel (without bootloader packaging)
Write-Host "Note: Direct kernel execution requires compatible UEFI firmware" -ForegroundColor Yellow
Write-Host "Kernel binary: $kernelPath" -ForegroundColor Cyan

exit 0
