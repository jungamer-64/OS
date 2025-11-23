# Build Userland Programs
# 
# This script builds all userland programs and prepares them for
# embedding into the kernel.
#
# Usage:
#   .\build_userland.ps1 [Release|Debug]
#
# Output:
#   - Builds userland programs
#   - Converts ELF to flat binary
#   - Copies binaries to kernel/src/

param(
    [Parameter(Mandatory = $false)]
    [ValidateSet("Release", "Debug")]
    [string]$BuildType = "Release"
)

$ErrorActionPreference = "Stop"

# Colors
$Green = "Green"
$Yellow = "Yellow"
$Red = "Red"
$Cyan = "Cyan"

function Write-Step {
    param([string]$Message)
    Write-Host "==> $Message" -ForegroundColor $Cyan
}

function Write-Success {
    param([string]$Message)
    Write-Host "[OK] $Message" -ForegroundColor $Green
}

function Write-Warning {
    param([string]$Message)
    Write-Host "[WARN] $Message" -ForegroundColor $Yellow
}

function Write-Error-Custom {
    param([string]$Message)
    Write-Host "[ERROR] $Message" -ForegroundColor $Red
}

# Get workspace root
$WorkspaceRoot = Split-Path -Parent $PSScriptRoot
if (Test-Path "$WorkspaceRoot\Cargo.toml") {
    $WorkspaceRoot = $PSScriptRoot
}

Write-Step "Building userland programs ($BuildType mode)"
Write-Host "Workspace: $WorkspaceRoot"
Write-Host ""

# Determine target and profile
$Target = "x86_64-rany_os"
$Profile = if ($BuildType -eq "Release") { "release" } else { "debug" }
$ProfileFlag = if ($BuildType -eq "Release") { "--release" } else { "" }

# ============================================================================
# Build libuser
# ============================================================================

Write-Step "Building libuser..."
# libuser doesn't need a specific target (it's used by the kernel)
$libuser_result = cargo build -p libuser 2>&1
if ($LASTEXITCODE -ne 0) {
    Write-Error-Custom "Failed to build libuser"
    Write-Host $libuser_result
    exit 1
}
Write-Success "libuser built successfully"

# ============================================================================
# Build shell
# ============================================================================

Write-Step "Building shell..."
$shell_result = cargo build --target $Target -p shell $ProfileFlag 2>&1
if ($LASTEXITCODE -ne 0) {
    Write-Error-Custom "Failed to build shell"
    Write-Host $shell_result
    exit 1
}
Write-Success "shell built successfully"

# ============================================================================
# Convert ELF to flat binary
# ============================================================================

$ShellELF = "$WorkspaceRoot\target\$Target\$Profile\shell"
$ShellBin = "$WorkspaceRoot\kernel\src\shell.bin"

if (-not (Test-Path $ShellELF)) {
    Write-Error-Custom "Shell ELF not found: $ShellELF"
    exit 1
}

Write-Step "Converting shell ELF to flat binary..."

# Try different objcopy commands
$objcopy_commands = @("rust-objcopy", "llvm-objcopy", "objcopy")
$converted = $false

foreach ($cmd in $objcopy_commands) {
    $CommandPath = Get-Command $cmd -ErrorAction SilentlyContinue
    if ($CommandPath) {
        Write-Host "Trying $cmd..."
        & $cmd --output-target=binary $ShellELF $ShellBin 2>&1 | Out-Null
        if ($LASTEXITCODE -eq 0) {
            Write-Success "Created shell.bin using $cmd"
            $converted = $true
            break
        }
    }
}

if (-not $converted) {
    Write-Warning "Could not convert shell to binary (objcopy not found)"
    Write-Warning "Creating dummy binary (infinite loop)"
    
    # Create dummy binary: 0xeb 0xfe (jmp $)
    $dummy = [byte[]](0xeb, 0xfe)
    [System.IO.File]::WriteAllBytes($ShellBin, $dummy)
}

# ============================================================================
# Summary
# ============================================================================

Write-Host ""
Write-Host "============================================" -ForegroundColor $Green
Write-Host "  Userland Build Complete" -ForegroundColor $Green
Write-Host "============================================" -ForegroundColor $Green
Write-Host ""
Write-Host "Built programs:"
Write-Host "  - libuser"
Write-Host "  - shell"
Write-Host ""
Write-Host "Output:"
$binSize = (Get-Item $ShellBin).Length
Write-Host "  - $ShellBin ($binSize bytes)"
Write-Host ""
Write-Host "Next step: Build the kernel"
Write-Host "  cargo build --target x86_64-rany_os.json"
Write-Host ""
