<#
.SYNOPSIS
    Unified build and run script for tiny_os (v2.5)

.DESCRIPTION
    Advanced build system with custom argument parsing, logging, and multiple execution modes.
    v2.5: Added Hardware Acceleration, Network support, Log History, and Clippy integration.
    
    Usage:
        .\run_qemu.ps1 -Menu                                  # Interactive Mode
        .\run_qemu.ps1                                        # Quick Build (Kernel) -> QEMU
        .\run_qemu.ps1 -FullBuild -Accel -Network             # Full Build with WHPX & Network
        .\run_qemu.ps1 -Check -BuildOnly                      # Run Clippy & Build only
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
    [string]$ExtraQemuArgStr = "",
    [string]$QemuPath = "qemu-system-x86_64",
    [string]$OverrideOvmfPath = "",
    [string]$Memory = "128M",
    [int]$Cores = 1,
    [switch]$InlineQemu, # If set, runs QEMU in current console (Legacy Tee-Object mode)
    [switch]$KeepAlive,  # If set, QEMU stays open after crash/shutdown (for debugging)
    [int]$Timeout = 0,   # Timeout in seconds (0 = no timeout, wait indefinitely)
    
    # --- v2.5 New Features ---
    [switch]$Accel,      # Enable hardware acceleration (WHPX)
    [switch]$Network,    # Enable user networking (e1000)
    [switch]$Check       # Run cargo clippy before build
)

$ErrorActionPreference = "Stop"

# Global state for cleanup on interruption
$script:currentQemuProc = $null
$script:currentLogJob = $null

# Ctrl+C handler - ensure QEMU is killed when user interrupts
[Console]::TreatControlCAsInput = $false
$null = Register-EngineEvent -SourceIdentifier PowerShell.Exiting -Action {
    if ($script:currentQemuProc -and -not $script:currentQemuProc.HasExited) {
        Stop-Process -Id $script:currentQemuProc.Id -Force -ErrorAction SilentlyContinue
    }
}

# ============================================================================
# Helper Functions
# ============================================================================

function Parse-ArgumentString {
    <#
      Parses a command line string into an array of arguments, preserving quoted strings.
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
                $null = $sb.Clear()
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
    Write-Host "      Tiny OS Build System (v2.5)      " -ForegroundColor Cyan
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
        [bool]$IsRelease,
        [bool]$RunCheck
    )
    
    # --- Static Analysis (Clippy) ---
    if ($RunCheck) {
        Write-Host "Running Cargo Clippy..." -ForegroundColor Cyan
        Push-Location (Join-Path $ScriptDir "kernel")
        try {
            $clippyArgs = @("clippy")
            if (Test-Path "x86_64-rany_os.json") {
                $clippyArgs += "--target", "x86_64-rany_os.json"
            }
            & cargo @clippyArgs
            if ($LASTEXITCODE -ne 0) {
                Write-Warning "Clippy found issues (or failed). Continuing build..."
            }
        }
        catch { Write-Warning "Failed to run cargo clippy: $_" }
        finally { Pop-Location }
    }
    # --------------------------------

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
            # Builder needs nightly (for bootloader crate) but WITHOUT build-std
            $bArgs = @("run", "nightly", "cargo", "-Zbuild-std=", "run")
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
        catch {
            Write-Host "Error running builder: $_" -ForegroundColor Red
            return 1
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
        [string]$ExtraArgString = "",
        [bool]$UseStartProcess,
        [bool]$KeepAlive = $false,
        [int]$TimeoutSec = 0,
        [bool]$EnableAccel = $false,
        [bool]$EnableNet = $false
    )
    
    Write-Host "Starting QEMU..." -ForegroundColor Green
    
    $logDir = Join-Path $PSScriptRoot "logs"
    if (-not (Test-Path $logDir)) { New-Item -ItemType Directory -Path $logDir -Force | Out-Null }
    
    # --- Enhanced Log Management (History) ---
    $historyDir = Join-Path $logDir "history"
    if (-not (Test-Path $historyDir)) { New-Item -ItemType Directory -Path $historyDir -Force | Out-Null }
    
    $qemuLog = Join-Path $logDir "qemu.debug.log"
    $stdoutLog = Join-Path $logDir "qemu.stdout.log"
    $stderrLog = Join-Path $logDir "qemu.stderr.log"

    $timestamp = Get-Date -Format "yyyyMMdd-HHmmss"
    $logsToRotate = @($qemuLog, $stdoutLog, $stderrLog)
    
    foreach ($log in $logsToRotate) {
        if (Test-Path $log) {
            $logName = Split-Path $log -Leaf
            $backupPath = Join-Path $historyDir "$logName.$timestamp.bak"
            try {
                Copy-Item -Path $log -Destination $backupPath -Force -ErrorAction SilentlyContinue
            }
            catch {}
        }
    }
    
    # Cleanup old logs (keep last 20)
    try {
        Get-ChildItem -Path $historyDir | Sort-Object CreationTime -Descending | 
        Select-Object -Skip 60 | Remove-Item -Force -ErrorAction SilentlyContinue
        # 3 files per run * 20 runs = 60 files
    }
    catch {}

    # Ensure empty log files exist
    New-Item -Path $stdoutLog -ItemType File -Force | Out-Null
    New-Item -Path $stderrLog -ItemType File -Force | Out-Null

    $qemuArgs = @(
        "-drive", "format=raw,file=$DiskImage",
        "-bios", "$OvmfPath",
        "-m", $Mem,
        "-smp", $CpuCores,
        "-no-reboot",
        "-d", "int,cpu_reset",
        "-D", $qemuLog
    )
    
    # --- Acceleration ---
    if ($EnableAccel) {
        Write-Host "  Acceleration: Enabled (WHPX)" -ForegroundColor Cyan
        $qemuArgs += "-accel", "whpx"
        # Fallback note: if WHPX fails, try "-accel", "hax" (HAXM) or remove this flag
    }

    # --- Networking ---
    if ($EnableNet) {
        Write-Host "  Network: Enabled (User/NAT, e1000)" -ForegroundColor Cyan
        $qemuArgs += "-netdev", "user,id=net0"
        $qemuArgs += "-device", "e1000,netdev=net0"
    }
    
    # Serial port configuration
    if ($IsNoGraphic) {
        $qemuArgs += "-serial", "mon:stdio"
    }
    else {
        $qemuArgs += "-serial", "stdio"
    }

    if ($KeepAlive) {
        $qemuArgs += "-no-shutdown"
    }

    if ($IsDebug) {
        Write-Host "  GDB Stub: localhost:1234" -ForegroundColor Magenta
        $qemuArgs += "-s", "-S"
    }

    # Normalize -nographic usage
    $hasNographicInExtra = $false
    if ($ExtraArgString -ne "") { $hasNographicInExtra = ($ExtraArgString -match '(?i)\b-nographic\b') }
    if (-not $hasNographicInExtra -and ($null -ne $ExtraArgs)) { $hasNographicInExtra = ($ExtraArgs -contains '-nographic') }
    if ($IsNoGraphic -and -not $hasNographicInExtra) { $qemuArgs += "-nographic" }
    
    # Append additional args
    if (($null -ne $ExtraArgs) -and ($ExtraArgs.Count -gt 0)) {
        $qemuArgs += $ExtraArgs
    }

    Write-Host "Executing: $QemuExe $($qemuArgs -join ' ')" -ForegroundColor DarkGray

    # --- Preferred: Start-Process mode ---
    if ($UseStartProcess) {
        try {
            $argList = @($qemuArgs)
            $proc = Start-Process -FilePath $QemuExe -ArgumentList $argList `
                -RedirectStandardOutput $stdoutLog `
                -RedirectStandardError $stderrLog `
                -NoNewWindow -PassThru
            
            $null = $proc.Handle
            $script:currentQemuProc = $proc
        }
        catch {
            Write-Host "Start-Process failed: $_" -ForegroundColor Red
            return 1
        }

        try {
            if ($TimeoutSec -gt 0) {
                Write-Host "  (Timeout: ${TimeoutSec}s - Output logged to $stdoutLog)" -ForegroundColor DarkGray
                $exited = $proc.WaitForExit($TimeoutSec * 1000)
                if (-not $exited) {
                    Write-Host "Timeout reached (${TimeoutSec}s). Killing QEMU..." -ForegroundColor Yellow
                    Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
                    Start-Sleep -Milliseconds 500
                }
            }
            else {
                Write-Host "  (Output logged to $stdoutLog - Press Ctrl+C to stop)" -ForegroundColor DarkGray
                # Poll-based wait to allow Ctrl+C interruption
                while (-not $proc.HasExited) {
                    Start-Sleep -Milliseconds 200
                }
            }

            $procExit = 0
            if ($proc.HasExited) { $procExit = $proc.ExitCode }
            $script:currentQemuProc = $null
            
            if ($script:DebugPreference -eq 'Continue' -or $env:DEBUG) {
                Write-Host "DEBUG: Process exited with code: $procExit" -ForegroundColor Yellow
            }
            
            if (Test-Path $stdoutLog) {
                Write-Host "`n--- QEMU Output (last 50 lines) ---" -ForegroundColor Cyan
                Get-Content -Path $stdoutLog -Tail 50 -ErrorAction SilentlyContinue | Out-Host
                Write-Host "--- End of Output ---`n" -ForegroundColor Cyan
            }
            return $procExit
        }
        catch {
            Write-Host "Error while waiting for QEMU: $_" -ForegroundColor Red
            if ($script:currentQemuProc) { 
                Stop-Process -Id $script:currentQemuProc.Id -ErrorAction SilentlyContinue 
            }
            return 1
        }
    }
    else {
        # --- Inline mode (direct execution, Ctrl+C works) ---
        Write-Host "  (Running inline - Press Ctrl+C to stop)" -ForegroundColor DarkGray
        
        if ($TimeoutSec -gt 0) {
            # Timeout mode: run as job and wait with timeout
            Write-Host "  (Timeout: ${TimeoutSec}s)" -ForegroundColor DarkGray
            $job = Start-Job -ScriptBlock {
                param($exe, $args, $logFile)
                & $exe @args 2>&1 | Tee-Object -FilePath $logFile
            } -ArgumentList $QemuExe, $qemuArgs, $stdoutLog
            
            $completed = Wait-Job $job -Timeout $TimeoutSec
            if ($null -eq $completed) {
                Write-Host "Timeout reached (${TimeoutSec}s). Killing QEMU..." -ForegroundColor Yellow
                Stop-Job $job -Force
                # Kill any QEMU processes started by the job
                Get-Process -Name "qemu-system-*" -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
            }
            Receive-Job $job | Out-Host
            Remove-Job $job -Force -ErrorAction SilentlyContinue
            return 0
        }
        else {
            # No timeout: direct execution with Ctrl+C support
            & $QemuExe @qemuArgs 2>&1 | Tee-Object -FilePath $stdoutLog | Out-Host
            return $LASTEXITCODE
        }
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
    if ($Memory -notmatch '^\d+[MG]$') {
        throw "Invalid memory format '$Memory'. Use '128M', '2G', etc."
    }
    if ($Cores -lt 1) {
        Write-Warning "Cores cannot be less than 1. Resetting to 1."
        $Cores = 1
    }

    # Configuration
    $buildProfile = if ($Release) { "release" } else { "debug" }
    $kernelPath = Join-Path $scriptDir "target\x86_64-rany_os\$buildProfile\tiny_os"
    $ovmfPath = if ($OverrideOvmfPath) { $OverrideOvmfPath } else { Join-Path $scriptDir "ovmf-x64\OVMF.fd" }
    $diskImage = Join-Path $scriptDir "target\x86_64-rany_os\$buildProfile\boot-uefi-tiny_os.img"
    $initrdPath = Join-Path $scriptDir "target\initrd.cpio"
    $builderDir = Join-Path $scriptDir "builder"
    
    # -NoGraphic の場合は直接実行モードを使用（Ctrl+Cが効くように）
    $UseStartProcess = (-not $InlineQemu) -and (-not $NoGraphic)

    # Pre-flight Checks
    if (-not (Get-Command rustup -ErrorAction SilentlyContinue)) { throw "rustup not found in PATH." }
    if (-not (Test-Path $QemuPath) -and -not (Get-Command $QemuPath -ErrorAction SilentlyContinue)) {
        throw "QEMU executable not found: $QemuPath"
    }
    if (-not (Test-Path $ovmfPath)) { throw "OVMF firmware not found at: $ovmfPath" }

    # Normalize Extra QEMU args
    if ($Debug) { Write-Host "DEBUG (before parse): ExtraQemuArgStr='$ExtraQemuArgStr' ExtraQemuArgs=[$($ExtraQemuArgs -join ', ')]" -ForegroundColor Yellow }
    $combinedExtraArgStr = ""
    if ($ExtraQemuArgStr -ne "") { $combinedExtraArgStr = $ExtraQemuArgStr.Trim() }
    if (($null -ne $ExtraQemuArgs) -and ($ExtraQemuArgs.Count -gt 0)) {
        $eaJoined = $ExtraQemuArgs -join ' '
        if ($combinedExtraArgStr -eq "") { $combinedExtraArgStr = $eaJoined } else { $combinedExtraArgStr = "$combinedExtraArgStr $eaJoined" }
    }
    $EffectiveExtraArgs = @()
    if ($combinedExtraArgStr -ne "") {
        $normalized = [regex]::Replace($combinedExtraArgStr, '(-nographic)(\s*-nographic)+', '$1', [System.Text.RegularExpressions.RegexOptions]::IgnoreCase)
        $EffectiveExtraArgs = Parse-ArgumentString $normalized
    }
    
    if ($NoGraphic -and ($null -ne $EffectiveExtraArgs)) {
        $EffectiveExtraArgs = $EffectiveExtraArgs | Where-Object { $_ -ne '-nographic' }
    }

    # Bootstrap Info
    if (-not $Clean -and -not $Menu) {
        Write-Host "--- Configuration ---" -ForegroundColor DarkGray
        Write-Host "Profile: $buildProfile" -ForegroundColor DarkGray
        Write-Host "Hardware: $Memory RAM, $Cores Core(s)" -ForegroundColor DarkGray
        Write-Host "Features: $(if($Accel){'Accel '}else{''})$(if($Network){'Net '}else{''})$(if($Check){'Clippy'}else{''})" -ForegroundColor DarkGray
        Write-Host "QEMU: $QemuPath" -ForegroundColor DarkGray
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
                $rc = Start-Build -ScriptDir $scriptDir -BuilderDir $builderDir -KernelPath $mKernelPath -DiskImage $mDiskImage -InitrdPath $initrdPath -IsFullBuild $mFull -IsRelease $mRelease -RunCheck $Check
                if ($rc -ne 0) { Write-Host "Build Failed." -ForegroundColor Red; Pause; continue }
            }

            if (-not $mBuildOnly) {
                if (Test-Path $mDiskImage) {
                    $qrc = Start-QEMU -DiskImage $mDiskImage -OvmfPath $ovmfPath -QemuExe $QemuPath -IsDebug $mDebug -IsNoGraphic $false -Mem $Memory -CpuCores $Cores -ExtraArgs $EffectiveExtraArgs -ExtraArgString $combinedExtraArgStr -UseStartProcess $UseStartProcess -KeepAlive $KeepAlive -TimeoutSec $Timeout -EnableAccel $Accel -EnableNet $Network
                    if ($qrc -ne 0) { Write-Host "QEMU Error: $qrc" -ForegroundColor Red }
                }
                else { Write-Host "Image not found." -ForegroundColor Red }
            }
            Pause
        }
    }

    # 3. CLI Mode
    if (-not $SkipBuild) {
        $rc = Start-Build -ScriptDir $scriptDir -BuilderDir $builderDir -KernelPath $kernelPath -DiskImage $diskImage -InitrdPath $initrdPath -IsFullBuild $FullBuild -IsRelease $Release -RunCheck $Check
        if ($rc -ne 0) { throw "Build failed with exit code $rc" }
    }

    if ($BuildOnly) { Write-Host "Build complete."; exit 0 }

    if (-not (Test-Path $diskImage)) { throw "Disk image missing. Run without -SkipBuild." }

    $qrc = Start-QEMU -DiskImage $diskImage -OvmfPath $ovmfPath -QemuExe $QemuPath -IsDebug $Debug -IsNoGraphic $NoGraphic -Mem $Memory -CpuCores $Cores -ExtraArgs $EffectiveExtraArgs -ExtraArgString $combinedExtraArgStr -UseStartProcess $UseStartProcess -KeepAlive $KeepAlive -TimeoutSec $Timeout -EnableAccel $Accel -EnableNet $Network
    if ($qrc -ne 0) { throw "QEMU exited with code $qrc" }

}
catch {
    Write-Host "Error: $_" -ForegroundColor Red
    exit 1
}
finally {
    # --- Robust Cleanup on Interruption ---
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