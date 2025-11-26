<#
  run_qemu.ps1

  Unified build and run script for tiny_os.

  Features:
    - Build kernel (quick or full with userland/initrd)
    - Create UEFI boot image
    - Run in QEMU with OVMF
    - Clean build artifacts
    - Interactive menu mode

  Usage examples:
    # Interactive menu
    powershell -ExecutionPolicy Bypass -File .\run_qemu.ps1 -Menu

    # Full pipeline (quick kernel build + image + run)
    powershell -ExecutionPolicy Bypass -File .\run_qemu.ps1

    # Full build mode (builds userland, initrd, kernel, and image)
    powershell -ExecutionPolicy Bypass -File .\run_qemu.ps1 -FullBuild

    # Skip build (assume kernel already built)
    powershell -ExecutionPolicy Bypass -File .\run_qemu.ps1 -SkipBuild

    # Release build
    powershell -ExecutionPolicy Bypass -File .\run_qemu.ps1 -Release

    # Enable GDB debugging
    powershell -ExecutionPolicy Bypass -File .\run_qemu.ps1 -Debug

    # Clean build artifacts
    powershell -ExecutionPolicy Bypass -File .\run_qemu.ps1 -Clean

    # Build only (no QEMU)
    powershell -ExecutionPolicy Bypass -File .\run_qemu.ps1 -BuildOnly

  Notes:
    - This script expects to be located in the `OS` directory of the repository.
    - It uses `rustup run nightly` to ensure the nightly toolchain is used.
    - The builder tool is located at ./builder within the OS directory.
#>

param(
    [switch]$Menu,
    [switch]$SkipBuild,
    [switch]$FullBuild,
    [switch]$Release,
    [switch]$Debug,
    [switch]$NoGraphic,
    [switch]$Clean,
    [switch]$BuildOnly,
    [string]$ExtraQemuArgs = ""
)

# ============================================================================
# Helper Functions
# ============================================================================

function Show-Menu {
    Clear-Host
    Write-Host "=======================================" -ForegroundColor Cyan
    Write-Host "       Tiny OS Build System           " -ForegroundColor Cyan
    Write-Host "=======================================" -ForegroundColor Cyan
    Write-Host ""
    Write-Host "  1. Quick build & run (kernel only)" -ForegroundColor Green
    Write-Host "  2. Full build & run (userland + kernel)" -ForegroundColor Green
    Write-Host "  3. Build only (no QEMU)" -ForegroundColor Yellow
    Write-Host "  4. Run only (skip build)" -ForegroundColor Yellow
    Write-Host "  5. Debug mode (GDB)" -ForegroundColor Magenta
    Write-Host "  6. Release build & run" -ForegroundColor Blue
    Write-Host "  7. Clean build artifacts" -ForegroundColor Red
    Write-Host "  8. Exit" -ForegroundColor Gray
    Write-Host ""
    Write-Host "=======================================" -ForegroundColor Cyan
}

function Clean-BuildArtifacts {
    param([string]$ScriptDir)
    
    Write-Host "Cleaning build artifacts..." -ForegroundColor Yellow
    
    # Clean cargo artifacts
    Push-Location $ScriptDir
    & cargo clean 2>$null
    Pop-Location
    
    # Clean builder artifacts
    $builderDir = Join-Path $ScriptDir "builder"
    if (Test-Path $builderDir) {
        Push-Location $builderDir
        & cargo clean 2>$null
        Pop-Location
    }
    
    # Clean kernel artifacts
    $kernelDir = Join-Path $ScriptDir "kernel"
    if (Test-Path $kernelDir) {
        Push-Location $kernelDir
        & cargo clean 2>$null
        Pop-Location
    }
    
    # Clean userland artifacts
    $userlandDir = Join-Path $ScriptDir "userland"
    if (Test-Path $userlandDir) {
        Get-ChildItem -Path $userlandDir -Recurse -Directory -Filter "target" | ForEach-Object {
            Remove-Item -Recurse -Force $_.FullName -ErrorAction SilentlyContinue
        }
    }
    
    # Clean initrd
    $initrdPath = Join-Path $ScriptDir "target\initrd.cpio"
    if (Test-Path $initrdPath) {
        Remove-Item -Force $initrdPath -ErrorAction SilentlyContinue
    }
    
    # Clean initrd_root
    $initrdRoot = Join-Path $ScriptDir "target\initrd_root"
    if (Test-Path $initrdRoot) {
        Remove-Item -Recurse -Force $initrdRoot -ErrorAction SilentlyContinue
    }
    
    Write-Host "Clean complete!" -ForegroundColor Green
}

function Run-Build {
    param(
        [string]$ScriptDir,
        [string]$BuilderDir,
        [string]$KernelPath,
        [string]$DiskImage,
        [string]$InitrdPath,
        [bool]$IsFullBuild,
        [bool]$IsRelease
    )
    
    $profileFlag = if ($IsRelease) { "--release" } else { "" }
    
    if ($IsFullBuild) {
        # Full build: use builder tool which builds userland, initrd, kernel, and creates image
        Write-Host "Running full build (userland + initrd + kernel + image)..." -ForegroundColor Cyan
        Push-Location $BuilderDir
        & rustup run nightly cargo run $profileFlag
        $buildExit = $LASTEXITCODE
        Pop-Location
        return $buildExit
    } else {
        # Quick build: just kernel, then create image
        Write-Host "Building kernel..." -ForegroundColor Cyan
        Push-Location (Join-Path $ScriptDir "kernel")
        $buildArgs = @("run", "nightly", "cargo", "build", "--target", "x86_64-rany_os.json")
        if ($IsRelease) { $buildArgs += "--release" }
        & rustup @buildArgs
        $buildExit = $LASTEXITCODE
        Pop-Location
        if ($buildExit -ne 0) {
            return $buildExit
        }

        # Create boot image using builder's quick mode
        Write-Host "Creating EFI disk image..." -ForegroundColor Cyan
        Push-Location $BuilderDir
        $builderArgs = @("run", "nightly", "cargo", "run")
        if ($IsRelease) { $builderArgs += "--release" }
        $builderArgs += "--"
        $builderArgs += "--kernel-path"
        $builderArgs += $KernelPath
        $builderArgs += "--output-path"
        $builderArgs += $DiskImage
        if (Test-Path $InitrdPath) {
            Write-Host "  Including initrd: $InitrdPath" -ForegroundColor Green
            $builderArgs += "--ramdisk"
            $builderArgs += $InitrdPath
        }
        & rustup @builderArgs
        $builderRc = $LASTEXITCODE
        Pop-Location
        return $builderRc
    }
}

function Run-QEMU {
    param(
        [string]$DiskImage,
        [string]$OvmfPath,
        [bool]$IsDebug,
        [bool]$IsNoGraphic,
        [string]$ExtraArgs
    )
    
    Write-Host "Found disk image: $DiskImage" -ForegroundColor Green
    Write-Host "Starting QEMU with OVMF (serial -> stdio). Press Ctrl+C to exit QEMU." -ForegroundColor Cyan

    $qemuArgs = @()
    $qemuArgs += "-drive"
    $qemuArgs += "format=raw,file=$DiskImage"
    $qemuArgs += "-bios"
    $qemuArgs += "$OvmfPath"
    $qemuArgs += "-serial"
    $qemuArgs += "stdio"
    $qemuArgs += "-m"
    $qemuArgs += "128M"
    $qemuArgs += "-no-reboot"
    $qemuArgs += "-no-shutdown"
    $qemuArgs += "-d"
    $qemuArgs += "int,cpu_reset"
    $qemuArgs += "-D"
    $qemuArgs += "qemu.log"

    # GDB debugging support
    if ($IsDebug) {
        Write-Host "GDB debugging enabled. Connect with: target remote localhost:1234" -ForegroundColor Yellow
        $qemuArgs += "-s"
        $qemuArgs += "-S"
    }

    # No graphic mode (headless)
    if ($IsNoGraphic) {
        $qemuArgs += "-nographic"
    }

    if ($ExtraArgs -ne "") {
        $extra = $ExtraArgs -split ' '
        $qemuArgs += $extra
    }

    Write-Host "qemu-system-x86_64 $($qemuArgs -join ' ')" -ForegroundColor Green
    & qemu-system-x86_64 @qemuArgs
}

# ============================================================================
# Main Script
# ============================================================================

try {
    # Make script work even when invoked from a different CWD
    $scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
    if (-not $scriptDir) { $scriptDir = Get-Location }
    Push-Location $scriptDir

    # Determine build profile
    $profile = if ($Release) { "release" } else { "debug" }

    $kernelPath = Join-Path $scriptDir "target\x86_64-rany_os\$profile\tiny_os"
    $ovmfPath   = Join-Path $scriptDir "ovmf-x64\OVMF.fd"
    $diskImage  = Join-Path $scriptDir "target\x86_64-rany_os\$profile\boot-uefi-tiny_os.img"
    $initrdPath = Join-Path $scriptDir "target\initrd.cpio"
    $builderDir = Join-Path $scriptDir "builder"

    # Handle -Clean flag
    if ($Clean) {
        Clean-BuildArtifacts -ScriptDir $scriptDir
        Pop-Location
        exit 0
    }

    # Handle -Menu flag (interactive mode)
    if ($Menu) {
        while ($true) {
            Show-Menu
            $choice = Read-Host "Select option (1-8)"
            
            switch ($choice) {
                "1" {
                    # Quick build & run
                    $buildExit = Run-Build -ScriptDir $scriptDir -BuilderDir $builderDir -KernelPath $kernelPath -DiskImage $diskImage -InitrdPath $initrdPath -IsFullBuild $false -IsRelease $false
                    if ($buildExit -eq 0 -and (Test-Path $diskImage)) {
                        Run-QEMU -DiskImage $diskImage -OvmfPath $ovmfPath -IsDebug $false -IsNoGraphic $false -ExtraArgs ""
                    } else {
                        Write-Host "Build failed (exit $buildExit)" -ForegroundColor Red
                    }
                }
                "2" {
                    # Full build & run
                    $buildExit = Run-Build -ScriptDir $scriptDir -BuilderDir $builderDir -KernelPath $kernelPath -DiskImage $diskImage -InitrdPath $initrdPath -IsFullBuild $true -IsRelease $false
                    if ($buildExit -eq 0 -and (Test-Path $diskImage)) {
                        Run-QEMU -DiskImage $diskImage -OvmfPath $ovmfPath -IsDebug $false -IsNoGraphic $false -ExtraArgs ""
                    } else {
                        Write-Host "Build failed (exit $buildExit)" -ForegroundColor Red
                    }
                }
                "3" {
                    # Build only
                    $buildExit = Run-Build -ScriptDir $scriptDir -BuilderDir $builderDir -KernelPath $kernelPath -DiskImage $diskImage -InitrdPath $initrdPath -IsFullBuild $false -IsRelease $false
                    if ($buildExit -eq 0) {
                        Write-Host "Build complete!" -ForegroundColor Green
                    } else {
                        Write-Host "Build failed (exit $buildExit)" -ForegroundColor Red
                    }
                }
                "4" {
                    # Run only
                    if (Test-Path $diskImage) {
                        Run-QEMU -DiskImage $diskImage -OvmfPath $ovmfPath -IsDebug $false -IsNoGraphic $false -ExtraArgs ""
                    } else {
                        Write-Host "Disk image not found. Build first." -ForegroundColor Red
                    }
                }
                "5" {
                    # Debug mode
                    $buildExit = Run-Build -ScriptDir $scriptDir -BuilderDir $builderDir -KernelPath $kernelPath -DiskImage $diskImage -InitrdPath $initrdPath -IsFullBuild $false -IsRelease $false
                    if ($buildExit -eq 0 -and (Test-Path $diskImage)) {
                        Run-QEMU -DiskImage $diskImage -OvmfPath $ovmfPath -IsDebug $true -IsNoGraphic $false -ExtraArgs ""
                    } else {
                        Write-Host "Build failed (exit $buildExit)" -ForegroundColor Red
                    }
                }
                "6" {
                    # Release build & run
                    $releaseKernelPath = Join-Path $scriptDir "target\x86_64-rany_os\release\tiny_os"
                    $releaseDiskImage = Join-Path $scriptDir "target\x86_64-rany_os\release\boot-uefi-tiny_os.img"
                    $buildExit = Run-Build -ScriptDir $scriptDir -BuilderDir $builderDir -KernelPath $releaseKernelPath -DiskImage $releaseDiskImage -InitrdPath $initrdPath -IsFullBuild $false -IsRelease $true
                    if ($buildExit -eq 0 -and (Test-Path $releaseDiskImage)) {
                        Run-QEMU -DiskImage $releaseDiskImage -OvmfPath $ovmfPath -IsDebug $false -IsNoGraphic $false -ExtraArgs ""
                    } else {
                        Write-Host "Build failed (exit $buildExit)" -ForegroundColor Red
                    }
                }
                "7" {
                    # Clean
                    Clean-BuildArtifacts -ScriptDir $scriptDir
                }
                "8" {
                    Write-Host "Goodbye!" -ForegroundColor Cyan
                    Pop-Location
                    exit 0
                }
                default {
                    Write-Host "Invalid option" -ForegroundColor Red
                }
            }
            
            Write-Host ""
            Write-Host "Press any key to continue..."
            $null = $Host.UI.RawUI.ReadKey("NoEcho,IncludeKeyDown")
        }
    }

    # Non-interactive mode
    Write-Host "=== tiny_os: build -> image -> run pipeline ===" -ForegroundColor Cyan
    Write-Host "  Profile: $profile" -ForegroundColor Cyan

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

    # Check builder exists
    if (-not (Test-Path $builderDir)) {
        Write-Host "Error: builder not found at $builderDir" -ForegroundColor Red
        Pop-Location; exit 1
    }

    # Build step
    if (-not $SkipBuild) {
        $buildExit = Run-Build -ScriptDir $scriptDir -BuilderDir $builderDir -KernelPath $kernelPath -DiskImage $diskImage -InitrdPath $initrdPath -IsFullBuild $FullBuild -IsRelease $Release
        if ($buildExit -ne 0) {
            Write-Host "Build failed (exit $buildExit). Aborting." -ForegroundColor Red
            Pop-Location; exit $buildExit
        }
    } else {
        Write-Host "Skipping build (-SkipBuild was provided)." -ForegroundColor Yellow
    }

    # Build only mode - skip QEMU
    if ($BuildOnly) {
        Write-Host "Build complete! (-BuildOnly was provided, skipping QEMU)" -ForegroundColor Green
        Pop-Location
        exit 0
    }

    if (!(Test-Path $kernelPath)) {
        Write-Host "Error: Kernel not found at $kernelPath" -ForegroundColor Red
        Write-Host "Try running without -SkipBuild flag" -ForegroundColor Yellow
        Pop-Location; exit 1
    }

    # Run QEMU
    if (!(Test-Path $diskImage)) {
        Write-Host "Error: Disk image not found at $diskImage" -ForegroundColor Red
        Write-Host "Try running without -SkipBuild flag" -ForegroundColor Yellow
        Pop-Location; exit 1
    }

    Run-QEMU -DiskImage $diskImage -OvmfPath $ovmfPath -IsDebug $Debug -IsNoGraphic $NoGraphic -ExtraArgs $ExtraQemuArgs

    # Return to original directory
    Pop-Location

} catch {
    Write-Host "An unexpected error occurred: $_" -ForegroundColor Red
    try { Pop-Location } catch { }
    exit 1
}
