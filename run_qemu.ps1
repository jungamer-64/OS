<#
  run_qemu.ps1

  Unified build and run script for tiny_os.
  
    Usage:
        .\run_qemu.ps1 -Menu                 # Interactive Mode
        .\run_qemu.ps1                       # Quick Build & Run (kernel only)
        .\run_qemu.ps1 -FullBuild            # Rebuild Userland/Initrd + Kernel + Run
        .\run_qemu.ps1 -Debug                # Enable GDB Stub (GDB stub: localhost:1234)
        .\run_qemu.ps1 -SkipBuild -ExtraQemuArgs "-nographic"   # Run disk image with qemu and capture logs (Start-Process used by default)
        .\run_qemu.ps1 -QemuPath "C:\Program Files\qemu\qemu-system-x86_64.exe" -OverrideOvmfPath "C:\OVMF\OVMF.fd" # Use explicit qemu and OVMF paths
        .\run_qemu.ps1 -SkipBuild -NoStartQemuProcess -ExtraQemuArgs "-nographic"  # Run inline (no Start-Process) and use Tee-Object for stdout capture
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
    [string[]]$ExtraQemuArgs = @(), 
    [string]$QemuPath = "qemu-system-x86_64",
    [string]$OverrideOvmfPath = "",
    [switch]$NoStartQemuProcess
)

$ErrorActionPreference = "Stop"

# ============================================================================
# Helper Functions
# ============================================================================

function Parse-ArgumentString {
    param([string]$Str)
    if ([string]::IsNullOrWhiteSpace($Str)) { return @() }
    $s = $Str
    $len = $s.Length
    $i = 0
    $tokens = @()
    while ($i -lt $len) {
        # skip whitespace
        while ($i -lt $len -and [char]::IsWhiteSpace($s[$i])) { $i++ }
        if ($i -ge $len) { break }
        $c = $s[$i]
        $token = ''
        if ($c -eq '"' -or $c -eq "'") {
            # quoted token
            $quote = $c
            $i++ # skip opening quote
            while ($i -lt $len) {
                $ch = $s[$i]
                if ($ch -eq '\\' -and $i + 1 -lt $len) { $token += $s[$i + 1]; $i += 2; continue }
                if ($ch -eq $quote) { $i++; break }
                $token += $ch; $i++
            }
            $tokens += $token
            continue
        }
        else {
            # unquoted token, but if we have an '=' followed by a quoted string, then absorb the quoted string into the token
            while ($i -lt $len -and -not [char]::IsWhiteSpace($s[$i])) {
                $ch = $s[$i]
                if ($ch -eq '=' -and $i + 1 -lt $len -and ($s[$i + 1] -eq '"' -or $s[$i + 1] -eq "'")) {
                    # include '='
                    $token += '='; $i++;
                    $quote = $s[$i]
                    $i++ # skip opening quote
                    while ($i -lt $len) {
                        $ch2 = $s[$i]
                        if ($ch2 -eq '\\' -and $i + 1 -lt $len) { $token += $s[$i + 1]; $i += 2; continue }
                        if ($ch2 -eq $quote) { $i++; break }
                        $token += $ch2; $i++
                    }
                    continue
                }
                $token += $ch; $i++
            }
            $tokens += $token
            continue
        }
    }
    return $tokens
}

function Parse-ExtraArgs {
    param([object]$Args)
    $result = @()
    if ($Args -eq $null) { return $result }
    if ($Args -is [System.Array]) {
        foreach ($a in $Args) {
            if ($a -is [string]) {
                $result += Parse-ArgumentString $a
            } else {
                $result += $a.ToString()
            }
        }
    } else {
        $result += Parse-ArgumentString $Args.ToString()
    }
    return $result
}

function Show-Menu {
    Clear-Host
    Write-Host "=======================================" -ForegroundColor Cyan
    Write-Host "      Tiny OS Build System            " -ForegroundColor Cyan
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

function Clear-BuildArtifacts {
    param([string]$ScriptDir)
    
    Write-Host "Cleaning build artifacts..." -ForegroundColor Yellow
    
    # Define directories to clean
    $dirsToClean = @(
        $ScriptDir,                          # Root
        (Join-Path $ScriptDir "builder"),    # Builder tool
        (Join-Path $ScriptDir "kernel")      # Kernel
    )

    foreach ($dir in $dirsToClean) {
        if (Test-Path $dir) {
            Push-Location $dir
            try {
                Write-Host "  Cleaning $(Split-Path $dir -Leaf)..." -ForegroundColor DarkGray
                & cargo clean 2>$null
            } finally {
                Pop-Location
            }
        }
    }
    
    # Clean userland target specifically
    $userlandDir = Join-Path $ScriptDir "userland"
    if (Test-Path $userlandDir) {
        Get-ChildItem -Path $userlandDir -Recurse -Directory -Filter "target" | ForEach-Object {
            Remove-Item -LiteralPath $_.FullName -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
    
    # Clean specific build targets
    $artifacts = @("target\initrd.cpio", "target\initrd_root")
    foreach ($art in $artifacts) {
        $p = Join-Path $ScriptDir $art
        if (Test-Path $p) { Remove-Item -LiteralPath $p -Recurse -Force -ErrorAction SilentlyContinue }
    }
    
    Write-Host "Clean complete!" -ForegroundColor Green
}

function Start-Build {
    param(
        [string]$ScriptDir,
        [string]$BuilderDir,
        [string]$KernelPath,
        [string]$DiskImage,
        [string]$InitrdPath,
        [bool]$IsFullBuild,
        [bool]$IsRelease
    )
    
    # The release/profile is handled by appending '--release' to args when $IsRelease is true
    
    if ($IsFullBuild) {
        # Full build: Builder tool handles userland -> initrd -> kernel -> image
        Write-Host "Running full build pipeline..." -ForegroundColor Cyan
        Push-Location $BuilderDir
        try {
            # Use invoke-expression or direct call
            $cmdArgs = @("run", "nightly", "cargo", "run")
            if ($IsRelease) { $cmdArgs += "--release" }

            & rustup @cmdArgs | Out-Host
            $buildExit = $LASTEXITCODE
        } finally {
            Pop-Location
        }
        return $buildExit
    }
    else {
        # Quick build: Kernel Direct -> Image
        Write-Host "Building kernel (Quick Mode)..." -ForegroundColor Cyan
        $kernelDir = Join-Path $ScriptDir "kernel"
        Push-Location $kernelDir
        try {
            # Check for target spec
            if (-not (Test-Path "x86_64-rany_os.json")) {
                Write-Host "Error: x86_64-rany_os.json not found in kernel directory." -ForegroundColor Red
                return 1
            }

            $buildArgs = @("run", "nightly", "cargo", "build", "--target", "x86_64-rany_os.json")
            if ($IsRelease) { $buildArgs += "--release" }
            
            & rustup @buildArgs | Out-Host
            $buildExit = $LASTEXITCODE
        } finally {
            Pop-Location
        }
        
        if ($buildExit -ne 0) { return $buildExit }

        # Create boot image
        Write-Host "Creating EFI disk image..." -ForegroundColor Cyan
        Push-Location $BuilderDir
        try {
            $builderArgs = @("run", "nightly", "cargo", "run")
            if ($IsRelease) { $builderArgs += "--release" }
            $builderArgs += "--"
            $builderArgs += "--kernel-path"; $builderArgs += $KernelPath
            $builderArgs += "--output-path"; $builderArgs += $DiskImage
        
        if (Test-Path $InitrdPath) {
            Write-Host "  Including existing initrd: $InitrdPath" -ForegroundColor Green
            $builderArgs += "--ramdisk"; $builderArgs += $InitrdPath
        }
        else {
            Write-Host "  Warning: No initrd found. Booting kernel only." -ForegroundColor DarkYellow
        }

            & rustup @builderArgs | Out-Host
            $builderRc = $LASTEXITCODE
        } finally {
            Pop-Location
        }
        return $builderRc
    }
}

function Start-QEMU {
    param(
        [string]$DiskImage,
        [string]$OvmfPath,
        [string]$QemuExe,
        [bool]$IsDebug,
        [bool]$IsNoGraphic,
        [string[]]$ExtraArgs,
        [bool]$UseStartProcess
    )
    
    Write-Host "Starting QEMU..." -ForegroundColor Green
    
    $qemuLogPath = Join-Path $PSScriptRoot "qemu.log"
    $stdoutLogPath = Join-Path $PSScriptRoot "qemu.stdout.log"

    $qemuArgs = @(
        "-drive", "format=raw,file=$DiskImage",
        "-bios", "$OvmfPath",
        "-serial", "stdio",
        "-m", "128M",
        "-no-reboot",
        "-no-shutdown",
        "-d", "int,cpu_reset",
        "-D", $qemuLogPath
    )

    if ($IsDebug) {
        Write-Host "  GDB Stub: localhost:1234" -ForegroundColor Magenta
        $qemuArgs += "-s", "-S"
    }

    if ($IsNoGraphic) { $qemuArgs += "-nographic" }
    
    if ($ExtraArgs -ne $null -and $ExtraArgs.Count -gt 0) { $qemuArgs += $ExtraArgs }

    Write-Host "Executing: $QemuExe $($qemuArgs -join ' ')" -ForegroundColor DarkGray
    # If requested, use Start-Process for better control/capture, otherwise run inline and Tee
    if ($UseStartProcess) {
        $stderrLogPath = Join-Path $PSScriptRoot "qemu.stderr.log"
        try {
            $proc = Start-Process -FilePath $QemuExe -ArgumentList $qemuArgs -RedirectStandardOutput $stdoutLogPath -RedirectStandardError $stderrLogPath -NoNewWindow -PassThru -Wait
            $qemuExit = $proc.ExitCode
        } catch {
            Write-Host "Start-Process failed: $_" -ForegroundColor Red
            return 1
        }
        } else {
        # Capture stdout & stderr to a log file while preserving interactive console output
        $null = & $QemuExe @qemuArgs 2>&1 | Tee-Object -FilePath $stdoutLogPath
        $qemuExit = $LASTEXITCODE
    }
    return $qemuExit
}

# ============================================================================
# Main Execution
# ============================================================================

try {
    # Reliable path resolution
    $scriptDir = $PSScriptRoot
    if (-not $scriptDir) { $scriptDir = Get-Location }
    Push-Location $scriptDir

    # Configuration
    $buildProfile = if ($Release) { "release" } else { "debug" }
    $kernelPath = Join-Path $scriptDir "target\x86_64-rany_os\$buildProfile\tiny_os"
    $ovmfPath = if ($OverrideOvmfPath -ne "") { $OverrideOvmfPath } else { Join-Path $scriptDir "ovmf-x64\OVMF.fd" }
    $diskImage = Join-Path $scriptDir "target\x86_64-rany_os\$buildProfile\boot-uefi-tiny_os.img"
    $initrdPath = Join-Path $scriptDir "target\initrd.cpio"
    $builderDir = Join-Path $scriptDir "builder"

    # Parse ExtraQemuArgs into an argument array (supports quotes and spaces)
    $parsedExtraQemuArgs = Parse-ExtraArgs $ExtraQemuArgs

    # Pre-flight Checks
    # Decide whether to use Start-Process by default unless explicitly disabled
    $StartQemuProcess = -not $NoStartQemuProcess
    if (-not (Get-Command rustup -ErrorAction SilentlyContinue)) { throw "rustup not found in PATH." }
    # Validate qemu executable - either the file path exists or it's available on PATH
    $qemuCmdExists = $false
    if ($QemuPath -ne "") {
        if (Test-Path $QemuPath) { $qemuCmdExists = $true }
        elseif (Get-Command $QemuPath -ErrorAction SilentlyContinue) { $qemuCmdExists = $true }
    }
    if (-not $qemuCmdExists) { throw "$QemuPath not found in PATH or not a valid path." }
    if (-not (Test-Path $ovmfPath)) { throw "OVMF firmware not found at: $ovmfPath" }

    # 1. Clean Mode
    if ($Clean) {
        Clear-BuildArtifacts -ScriptDir $scriptDir
        exit 0
    }

    # 2. Menu Mode
    if ($Menu) {
        while ($true) {
            Show-Menu
            $choice = Read-Host "Select option (1-8)"
            
            # Map choices to flags
            $mFull = $false; $mBuildOnly = $false; $mSkip = $false; $mDebug = $false; $mRelease = $false
            
            switch ($choice) {
                "1" { } # Default quick
                "2" { $mFull = $true }
                "3" { $mBuildOnly = $true }
                "4" { $mSkip = $true }
                "5" { $mDebug = $true }
                "6" { $mRelease = $true }
                "7" { Clear-BuildArtifacts -ScriptDir $scriptDir; continue }
                "8" { Write-Host "Goodbye."; exit 0 }
                default { continue }
            }

            # Handle Release Path Overrides for Menu
            $mKernelPath = if ($mRelease) { Join-Path $scriptDir "target\x86_64-rany_os\release\tiny_os" } else { $kernelPath }
            $mDiskImage = if ($mRelease) { Join-Path $scriptDir "target\x86_64-rany_os\release\boot-uefi-tiny_os.img" } else { $diskImage }

            if (-not $mSkip) {
                $rc = Start-Build -ScriptDir $scriptDir -BuilderDir $builderDir -KernelPath $mKernelPath -DiskImage $mDiskImage -InitrdPath $initrdPath -IsFullBuild $mFull -IsRelease $mRelease
                if ($rc -ne 0) { 
                    Write-Host "Build Failed." -ForegroundColor Red; Pause; continue 
                }
            }

            if (-not $mBuildOnly) {
                if (Test-Path $mDiskImage) {
                    $qrc = Start-QEMU -DiskImage $mDiskImage -OvmfPath $ovmfPath -QemuExe $QemuPath -IsDebug $mDebug -IsNoGraphic $false -ExtraArgs $parsedExtraQemuArgs -UseStartProcess $StartQemuProcess
                    if ($qrc -ne 0) {
                        Write-Host "QEMU exited with code $qrc" -ForegroundColor Red
                    }
                }
                else {
                    Write-Host "Image not found." -ForegroundColor Red
                }
            }
            Pause
        }
    }

    # 3. CLI Mode
    Write-Host "=== tiny_os: $buildProfile mode ===" -ForegroundColor Cyan
    
    if (-not $SkipBuild) {
        $rc = Start-Build -ScriptDir $scriptDir -BuilderDir $builderDir -KernelPath $kernelPath -DiskImage $diskImage -InitrdPath $initrdPath -IsFullBuild $FullBuild -IsRelease $Release
        if ($rc -ne 0) { throw "Build failed with exit code $rc" }
    }

    if ($BuildOnly) {
        Write-Host "Build complete. Exiting." -ForegroundColor Green
        exit 0
    }

    if (-not (Test-Path $diskImage)) { throw "Disk image missing. Run without -SkipBuild." }

    $qrc = Start-QEMU -DiskImage $diskImage -OvmfPath $ovmfPath -QemuExe $QemuPath -IsDebug $Debug -IsNoGraphic $NoGraphic -ExtraArgs $parsedExtraQemuArgs -UseStartProcess $StartQemuProcess
    if ($qrc -ne 0) { throw "QEMU exited with code $qrc" }

}
catch {
    Write-Host "Error: $_" -ForegroundColor Red
    exit 1
}
finally {
    try { Pop-Location } catch {}
}