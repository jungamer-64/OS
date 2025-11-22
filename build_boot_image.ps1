# build_boot_image.ps1
# PowerShell script to build a bootable UEFI/BIOS image for Tiny OS

Write-Host "Building Tiny OS bootable image..." -ForegroundColor Cyan

# Step 1: Build the kernel
Write-Host "[1/3] Building kernel..." -ForegroundColor Yellow
$buildResult = cargo build --target x86_64-rany_os.json
if ($LASTEXITCODE -ne 0) {
    Write-Host "Kernel build failed!" -ForegroundColor Red
    exit 1
}
Write-Host "  ✓ Kernel built" -ForegroundColor Green

# Step 2: Install bootloader tool if not present
Write-Host "[2/3] Checking bootloader tool..." -ForegroundColor Yellow
$hasBootloader = cargo install --list | Select-String "bootloader"
if (-not $hasBootloader) {
    Write-Host "  Installing bootloader CLI..." -ForegroundColor Yellow
    cargo install bootloader
}
Write-Host "  ✓ Bootloader tool ready" -ForegroundColor Green

# Step 3: Create boot images
Write-Host "[3/3] Creating boot images..." -ForegroundColor Yellow
$kernelPath = "target\x86_64-rany_os\debug\tiny_os"
$outputDir = "target\x86_64-rany_os\debug"

# Create UEFI boot image
bootloader uefi $kernelPath -o "$outputDir\boot-uefi-tiny_os.img"
if ($LASTEXITCODE -eq 0) {
    Write-Host "  ✓ UEFI image: $outputDir\boot-uefi-tiny_os.img" -ForegroundColor Green
}

# Create BIOS boot image
bootloader bios $kernelPath -o "$outputDir\boot-bios-tiny_os.img"
if ($LASTEXITCODE -eq 0) {
    Write-Host "  ✓ BIOS image: $outputDir\boot-bios-tiny_os.img" -ForegroundColor Green
}

Write-Host ""
Write-Host "Build complete!" -ForegroundColor Green
Write-Host ""
Write-Host "To run in QEMU (BIOS):" -ForegroundColor Cyan
Write-Host "  qemu-system-x86_64 -drive format=raw,file=$outputDir\boot-bios-tiny_os.img -serial stdio"
