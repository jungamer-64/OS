<#
  run_qemu.ps1

  Combined pipeline for tiny_os:
    1) Build the kernel
    2) Create an EFI disk image using the `os_builder` tool
    3) Launch QEMU with OVMF and the created image (or fall back to direct kernel load)

  Usage examples:
    # Full pipeline (build, image, run)
    powershell -ExecutionPolicy Bypass -File .\run_qemu.ps1

    # Skip build (assume kernel already built)
    powershell -ExecutionPolicy Bypass -File .\run_qemu.ps1 -SkipBuild

    # Skip image creation (attempt direct kernel load)
    powershell -ExecutionPolicy Bypass -File .\run_qemu.ps1 -SkipImage

  Notes:
    - This script expects to be located in the `OS` directory of the repository.
    - It uses `rustup run nightly` to ensure the nightly toolchain is used when required
      (os_builder / bootloader requires nightly in this repository).
    - To view QEMU serial output in the current terminal we invoke qemu directly.
#>

param(
    [switch]$SkipBuild,
    [switch]$SkipImage,
    [string]$ExtraQemuArgs = ""
)

try {
    # Make script work even when invoked from a different CWD
    $scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
    if (-not $scriptDir) { $scriptDir = Get-Location }
    Push-Location $scriptDir

    $kernelPath = Join-Path $scriptDir "target\x86_64-rany_os\debug\tiny_os"
    $ovmfPath   = Join-Path $scriptDir "ovmf-x64\OVMF.fd"
    $diskImage  = Join-Path $scriptDir "target\x86_64-rany_os\debug\tiny_os.efi.img"

    Write-Host "=== tiny_os: build -> image -> run pipeline ===" -ForegroundColor Cyan

    # Basic command availability checks
    if (-not (Get-Command rustup -ErrorAction SilentlyContinue)) {
        Write-Host "Error: 'rustup' not found in PATH. Install rustup and try again." -ForegroundColor Red
        Pop-Location; exit 1
    }
    if (-not (Get-Command qemu-system-x86_64 -ErrorAction SilentlyContinue)) {
        Write-Host "Error: 'qemu-system-x86_64' not found in PATH. Install QEMU and try again." -ForegroundColor Red
        Pop-Location; exit 1
    }

    if (!(Test-Path $ovmfPath)) {
        Write-Host "Error: OVMF firmware not found at $ovmfPath" -ForegroundColor Red
        Pop-Location; exit 1
    }

    # 1) Build kernel (uses nightly to match the repository's build config)
    if (-not $SkipBuild) {
        Write-Host "Building kernel (rustup run nightly cargo build --target x86_64-rany_os.json) ..." -ForegroundColor Cyan
        & rustup run nightly cargo build --target x86_64-rany_os.json
        if ($LASTEXITCODE -ne 0) {
            Write-Host "Kernel build failed (exit $LASTEXITCODE). Aborting." -ForegroundColor Red
            Pop-Location; exit $LASTEXITCODE
        }
    } else {
        Write-Host "Skipping kernel build ( -SkipBuild was provided )." -ForegroundColor Yellow
    }

    if (!(Test-Path $kernelPath)) {
        Write-Host "Error: Kernel not found at $kernelPath" -ForegroundColor Red
        Write-Host "Try running: cargo +nightly build --target x86_64-rany_os.json" -ForegroundColor Yellow
        Pop-Location; exit 1
    }

    # 2) Create EFI disk image using os_builder (uses its own cargo context; run inside os_builder)
    if (-not $SkipImage) {
        Write-Host "Creating EFI disk image using os_builder..." -ForegroundColor Cyan
        $builderDir = Join-Path $scriptDir "..\os_builder"
        if (-not (Test-Path $builderDir)) {
            Write-Host "Error: os_builder not found at $builderDir" -ForegroundColor Red
            Pop-Location; exit 1
        }

        Push-Location $builderDir
        # Run the builder with nightly (bootloader build scripts require nightly)
        & rustup run nightly cargo run -- --kernel-path "$kernelPath" --output-path "$diskImage"
        $builderRc = $LASTEXITCODE
        Pop-Location

        if ($builderRc -ne 0) {
            Write-Host "os_builder failed (exit $builderRc). Aborting." -ForegroundColor Red
            Pop-Location; exit $builderRc
        }
    } else {
        Write-Host "Skipping image creation ( -SkipImage was provided )." -ForegroundColor Yellow
    }

    # 3) Run QEMU (prefer disk image; fallback to direct kernel load)
    $useImage = Test-Path $diskImage
    if ($useImage) { Write-Host "Found disk image: $diskImage" -ForegroundColor Green }
    else { Write-Host "Disk image not found; will attempt direct kernel load." -ForegroundColor Yellow }

    Write-Host "Starting QEMU with OVMF (serial -> stdio). Press Ctrl+C to exit QEMU." -ForegroundColor Cyan

    $qemuArgs = @()
    if ($useImage) {
        $qemuArgs += "-drive"
        $qemuArgs += "format=raw,file=$diskImage"
    } else {
        $qemuArgs += "-kernel"
        $qemuArgs += "$kernelPath"
    }
    $qemuArgs += "-bios"
    $qemuArgs += "$ovmfPath"
    $qemuArgs += "-serial"
    $qemuArgs += "stdio"
    $qemuArgs += "-m"
    $qemuArgs += "128M"
    $qemuArgs += "-no-reboot"
    $qemuArgs += "-no-shutdown"

    if ($ExtraQemuArgs -ne "") {
        # Split extra args by spaces (simple handling); advanced parsing is left to the caller
        $extra = $ExtraQemuArgs -split ' '
        $qemuArgs += $extra
    }

    Write-Host "qemu-system-x86_64 $($qemuArgs -join ' ')" -ForegroundColor Green
    & qemu-system-x86_64 @qemuArgs

    # Return to original directory
    Pop-Location

} catch {
    Write-Host "An unexpected error occurred: $_" -ForegroundColor Red
    try { Pop-Location } catch { }
    exit 1
}
