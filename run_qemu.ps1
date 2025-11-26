<#
.SYNOPSIS
    Unified build and run script for tiny_os (v2.4 Fixed)

.DESCRIPTION
    Advanced build system with custom argument parsing, logging, and multiple execution modes.
    v2.4 Fixed: Corrected syntax error in parser and hardened cleanup logic.
    
    Usage:
        .\run_qemu.ps1 -Menu                                  # Interactive Mode
        .\run_qemu.ps1                                        # Quick Build (Kernel) -> QEMU
        .\run_qemu.ps1 -FullBuild -Memory "512M" -Cores 2     # Custom Hardware Config
        .\run_qemu.ps1 -Debug                                 # Enable GDB Stub (localhost:1234)
        .\run_qemu.ps1 -SkipBuild -InlineQemu                 # Run inline with stdout mirroring
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
    [string]$Memory = "128M",
    [int]$Cores = 1,
    [switch]$InlineQemu # If set, runs QEMU in current console (Legacy Tee-Object mode)
)

$ErrorActionPreference = "Stop"

# Global state for cleanup on interruption
$script:currentQemuProc = $null
$script:currentLogJob = $null

# ============================================================================
# Helper Functions
# ============================================================================

function Parse-ArgumentString {
    <#
      Parses a command line string into an array of arguments, preserving quoted strings.
      Essential for passing complex -device or -drive arguments to QEMU.
    #>
    param([string]$Str)
    if ([string]::IsNullOrWhiteSpace($Str)) { return @() }
    
    $tokens = @()
    $sb = [System.Text.StringBuilder]::new()
    $inQuote = $false
    $quoteChar = $null
    $len = $Str.Length
    $i = 0

    while ($i -lt $len) {
        $c = $Str[$i]
        
        if ([char]::IsWhiteSpace($c) -and -not $inQuote) {
            if ($sb.Length -gt 0) {
                $tokens += $sb.ToString()
                $sb.Clear()
            }
        }
        elseif (($c -eq '"' -or $c -eq "'") -and -not $inQuote) {
            $inQuote = $true
            $quoteChar = $c
        }
        elseif ($c -eq $quoteChar -and $inQuote) {
            $inQuote = $false
            $quoteChar = $null
        }
        else {
            $null = $sb.Append($c)
        }
        $i++
    }
    if ($sb.Length -gt 0) { $tokens += $sb.ToString() }
    
    return $tokens
}

function Show-Menu {
    Clear-Host
    Write-Host "=======================================" -ForegroundColor Cyan
    Write-Host "   Tiny OS Build System (v2.4 Fixed)   " -ForegroundColor Cyan
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
    
    $dirsToClean = @(
        $ScriptDir,                          # Root
        (Join-Path $ScriptDir "builder"),    # Builder
        (Join-Path $ScriptDir "kernel")      # Kernel
    )

    foreach ($dir in $dirsToClean) {
        if (Test-Path $dir) {
            Push-Location $dir
            try {
                Write-Host "  Cleaning $(Split-Path $dir -Leaf)..." -ForegroundColor DarkGray
                & cargo clean 2>$null
            }
            finally { Pop-Location }
        }
    }
    
    # Clean userland targets
    $userlandDir = Join-Path $ScriptDir "userland"
    if (Test-Path $userlandDir) {
        Get-ChildItem -Path $userlandDir -Recurse -Directory -Filter "target" | ForEach-Object {
            Remove-Item -LiteralPath $_.FullName -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
    
    # Clean output artifacts
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
    
    if ($IsFullBuild) {
        Write-Host "Running full build pipeline..." -ForegroundColor Cyan
        Push-Location $BuilderDir
        try {
            $cmdArgs = @("run", "nightly", "cargo", "run")
            if ($IsRelease) { $cmdArgs += "--release" }
            
            & rustup @cmdArgs | Out-Host
            return $LASTEXITCODE
        }
        finally { Pop-Location }
    }
    else {
        Write-Host "Building kernel (Quick Mode)..." -ForegroundColor Cyan
        Push-Location (Join-Path $ScriptDir "kernel")
        try {
            if (-not (Test-Path "x86_64-rany_os.json")) {
                Write-Host "Error: Target JSON not found." -ForegroundColor Red; return 1
            }

            $buildArgs = @("run", "nightly", "cargo", "build", "--target", "x86_64-rany_os.json")
            if ($IsRelease) { $buildArgs += "--release" }
            
            & rustup @buildArgs | Out-Host
            if ($LASTEXITCODE -ne 0) { return $LASTEXITCODE }
        }
        finally { Pop-Location }

        Write-Host "Creating EFI disk image..." -ForegroundColor Cyan
        Push-Location $BuilderDir
        try {
            $bArgs = @("run", "nightly", "cargo", "run")
            if ($IsRelease) { $bArgs += "--release" }
            $bArgs += "--"; $bArgs += "--kernel-path"; $bArgs += $KernelPath
            $bArgs += "--output-path"; $bArgs += $DiskImage
            
            if (Test-Path $InitrdPath) {
                Write-Host "  Including initrd: $InitrdPath" -ForegroundColor Green
                $bArgs += "--ramdisk"; $bArgs += $InitrdPath
            }
            else {
                Write-Host "  Warning: No initrd found." -ForegroundColor DarkYellow
            }

            & rustup @bArgs | Out-Host
            return $LASTEXITCODE
        }
        finally { Pop-Location }
    }
}

function Start-QEMU {
    param(
        [string]$DiskImage,
        [string]$OvmfPath,
        [string]$QemuExe,
        [bool]$IsDebug,
        [bool]$IsNoGraphic,
        [string]$Mem,
        [int]$CpuCores,
        [string[]]$ExtraArgs,
        [bool]$UseStartProcess
    )
    
    Write-Host "Starting QEMU..." -ForegroundColor Green
    
    $logDir = Join-Path $PSScriptRoot "logs"
    if (-not (Test-Path $logDir)) { New-Item -ItemType Directory -Path $logDir -Force | Out-Null }

    $qemuLog = Join-Path $logDir "qemu.debug.log"
    $stdoutLog = Join-Path $logDir "qemu.stdout.log"
    $stderrLog = Join-Path $logDir "qemu.stderr.log"

    # --- Log Rotation ---
    # Rename existing logs to .old to preserve history of the previous run
    $logsToRotate = @($qemuLog, $stdoutLog, $stderrLog)
    foreach ($log in $logsToRotate) {
        if (Test-Path $log) {
            Move-Item -Path $log -Destination "$log.old" -Force -ErrorAction SilentlyContinue
        }
    }

    # Ensure empty log files exist for redirection
    New-Item -Path $stdoutLog -ItemType File -Force | Out-Null
    New-Item -Path $stderrLog -ItemType File -Force | Out-Null

    $qemuArgs = @(
        "-drive", "format=raw,file=$DiskImage",
        "-bios", "$OvmfPath",
        "-serial", "stdio",
        "-m", $Mem,
        "-smp", $CpuCores,
        "-no-reboot",
        "-no-shutdown",
        "-d", "int,cpu_reset",
        "-D", $qemuLog
    )

    if ($IsDebug) {
        Write-Host "  GDB Stub: localhost:1234" -ForegroundColor Magenta
        $qemuArgs += "-s", "-S"
    }

    if ($IsNoGraphic) { $qemuArgs += "-nographic" }
    
    foreach ($arg in $ExtraArgs) {
        if ($arg -match "['`"]") { $qemuArgs += Parse-ArgumentString $arg }
        else { $qemuArgs += $arg }
    }

    Write-Host "Executing: $QemuExe $($qemuArgs -join ' ')" -ForegroundColor DarkGray

    # --- Preferred: Start-Process mode (separate process, accurate exit code) ---
    if ($UseStartProcess) {
        try {
            $proc = Start-Process -FilePath $QemuExe -ArgumentList $qemuArgs `
                -RedirectStandardOutput $stdoutLog `
                -RedirectStandardError $stderrLog `
                -NoNewWindow -PassThru
            
            # Track process globally for cleanup on interrupt
            $script:currentQemuProc = $proc
        }
        catch {
            Write-Host "Start-Process failed: $_" -ForegroundColor Red
            return 1
        }

        # Stream output live while process runs
        try {
            # Start streaming stdout (stderr captured to file)
            $stdReader = Start-Job -ScriptBlock {
                param($path)
                Get-Content -Path $path -Wait -Tail 0 -Encoding UTF8
            } -ArgumentList $stdoutLog
            
            # Track job globally for cleanup
            $script:currentLogJob = $stdReader

            # Wait for process to exit
            $proc | Wait-Process

            # Give some time for last buffer to flush
            Start-Sleep -Milliseconds 200

            # Graceful cleanup of job
            if ($stdReader -and ($stdReader.State -eq 'Running')) {
                Stop-Job $stdReader | Out-Null
            }
            Remove-Job $stdReader -ErrorAction SilentlyContinue | Out-Null
            $script:currentLogJob = $null

            # Clear process tracking since it exited normally
            $procExit = $proc.ExitCode
            $script:currentQemuProc = $null
            return $procExit

        }
        catch {
            Write-Host "Error while streaming logs: $_" -ForegroundColor Red
            # Cleanup will be handled by finally block if we throw here, 
            # but we can try local stop
            if ($script:currentQemuProc) { 
                Stop-Process -Id $script:currentQemuProc.Id -ErrorAction SilentlyContinue 
            }
            return 1
        }
    }
    else {
        # --- Inline mode: fallback that mirrors output (but exit code might be masked) ---
        Write-Host "  (Running inline - output mirrored to $stdoutLog)" -ForegroundColor DarkGray
        & $QemuExe @qemuArgs 2>&1 | Tee-Object -FilePath $stdoutLog
        return $LASTEXITCODE
    }
}

# ============================================================================
# Main Execution
# ============================================================================

$__pushedScriptDir = $false

try {
    $scriptDir = $PSScriptRoot
    if (-not $scriptDir) { $scriptDir = Get-Location }
    
    Push-Location $scriptDir
    $__pushedScriptDir = $true

    # --- Parameter Validation ---
    # Validate Memory format (e.g. 128M, 2G)
    if ($Memory -notmatch '^\d+[MG]$') {
        throw "Invalid memory format '$Memory'. Use '128M', '2G', etc."
    }
    # Validate Cores
    if ($Cores -lt 1) {
        Write-Warning "Cores cannot be less than 1. Resetting to 1."
        $Cores = 1
    }
    # ----------------------------

    # Configuration
    $buildProfile = if ($Release) { "release" } else { "debug" }
    $kernelPath = Join-Path $scriptDir "target\x86_64-rany_os\$buildProfile\tiny_os"
    $ovmfPath = if ($OverrideOvmfPath) { $OverrideOvmfPath } else { Join-Path $scriptDir "ovmf-x64\OVMF.fd" }
    $diskImage = Join-Path $scriptDir "target\x86_64-rany_os\$buildProfile\boot-uefi-tiny_os.img"
    $initrdPath = Join-Path $scriptDir "target\initrd.cpio"
    $builderDir = Join-Path $scriptDir "builder"
    
    # Use Start-Process by default unless InlineQemu is requested
    $UseStartProcess = -not $InlineQemu

    # Pre-flight Checks
    if (-not (Get-Command rustup -ErrorAction SilentlyContinue)) { throw "rustup not found in PATH." }
    if (-not (Test-Path $QemuPath) -and -not (Get-Command $QemuPath -ErrorAction SilentlyContinue)) {
        throw "QEMU executable not found: $QemuPath"
    }
    if (-not (Test-Path $ovmfPath)) { throw "OVMF firmware not found at: $ovmfPath" }

    # Bootstrap Info
    if (-not $Clean -and -not $Menu) {
        Write-Host "--- Configuration ---" -ForegroundColor DarkGray
        Write-Host "Profile: $buildProfile" -ForegroundColor DarkGray
        Write-Host "Hardware: $Memory RAM, $Cores Core(s)" -ForegroundColor DarkGray
        Write-Host "QEMU: $QemuPath" -ForegroundColor DarkGray
        Write-Host "OVMF: $(Split-Path $ovmfPath -Leaf)" -ForegroundColor DarkGray
        Write-Host "---------------------`n" -ForegroundColor DarkGray
    }

    # 1. Clean Mode
    if ($Clean) { Clear-BuildArtifacts -ScriptDir $scriptDir; exit 0 }

    # 2. Menu Mode
    if ($Menu) {
        while ($true) {
            Show-Menu
            $choice = Read-Host "Select option (1-8)"
            
            $mFull = $false; $mBuildOnly = $false; $mSkip = $false; $mDebug = $false; $mRelease = $false
            
            switch ($choice) {
                "1" { }
                "2" { $mFull = $true }
                "3" { $mBuildOnly = $true }
                "4" { $mSkip = $true }
                "5" { $mDebug = $true }
                "6" { $mRelease = $true }
                "7" { Clear-BuildArtifacts -ScriptDir $scriptDir; Pause; continue }
                "8" { Write-Host "Goodbye."; exit 0 }
                default { continue }
            }

            $mKernelPath = if ($mRelease) { Join-Path $scriptDir "target\x86_64-rany_os\release\tiny_os" } else { $kernelPath }
            $mDiskImage = if ($mRelease) { Join-Path $scriptDir "target\x86_64-rany_os\release\boot-uefi-tiny_os.img" } else { $diskImage }

            if (-not $mSkip) {
                $rc = Start-Build -ScriptDir $scriptDir -BuilderDir $builderDir -KernelPath $mKernelPath -DiskImage $mDiskImage -InitrdPath $initrdPath -IsFullBuild $mFull -IsRelease $mRelease
                if ($rc -ne 0) { Write-Host "Build Failed." -ForegroundColor Red; Pause; continue }
            }

            if (-not $mBuildOnly) {
                if (Test-Path $mDiskImage) {
                    $qrc = Start-QEMU -DiskImage $mDiskImage -OvmfPath $ovmfPath -QemuExe $QemuPath -IsDebug $mDebug -IsNoGraphic $false -Mem $Memory -CpuCores $Cores -ExtraArgs $ExtraQemuArgs -UseStartProcess $UseStartProcess
                    if ($qrc -ne 0) { Write-Host "QEMU Error: $qrc" -ForegroundColor Red }
                }
                else { Write-Host "Image not found." -ForegroundColor Red }
            }
            Pause
        }
    }

    # 3. CLI Mode
    if (-not $SkipBuild) {
        $rc = Start-Build -ScriptDir $scriptDir -BuilderDir $builderDir -KernelPath $kernelPath -DiskImage $diskImage -InitrdPath $initrdPath -IsFullBuild $FullBuild -IsRelease $Release
        if ($rc -ne 0) { throw "Build failed with exit code $rc" }
    }

    if ($BuildOnly) { Write-Host "Build complete."; exit 0 }

    if (-not (Test-Path $diskImage)) { throw "Disk image missing. Run without -SkipBuild." }

    $qrc = Start-QEMU -DiskImage $diskImage -OvmfPath $ovmfPath -QemuExe $QemuPath -IsDebug $Debug -IsNoGraphic $NoGraphic -Mem $Memory -CpuCores $Cores -ExtraArgs $ExtraQemuArgs -UseStartProcess $UseStartProcess
    if ($qrc -ne 0) { throw "QEMU exited with code $qrc" }

}
catch {
    Write-Host "Error: $_" -ForegroundColor Red
    exit 1
}
finally {
    # --- Robust Cleanup on Interruption (Ctrl+C) ---
    if ($script:currentLogJob) {
        try {
            if ($script:currentLogJob.State -eq 'Running') { Stop-Job $script:currentLogJob -Force -ErrorAction SilentlyContinue }
            Remove-Job $script:currentLogJob -ErrorAction SilentlyContinue
        }
        catch {}
    }
    if ($script:currentQemuProc) {
        try {
            if (-not $script:currentQemuProc.HasExited) {
                Write-Host "Stopping QEMU process..." -ForegroundColor Yellow
                Stop-Process -Id $script:currentQemuProc.Id -Force -ErrorAction SilentlyContinue
            }
        }
        catch {}
    }
    
    if ($__pushedScriptDir) {
        try { Pop-Location } catch {}
    }
}