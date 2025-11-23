# Build and Run Helper
#
# Quick commands for development

Write-Host "=== Tiny OS Build Helper ===" -ForegroundColor Cyan
Write-Host ""

function Show-Menu {
    Write-Host "1. Build userland (libuser + shell)" -ForegroundColor Green
    Write-Host "2. Build kernel" -ForegroundColor Green
    Write-Host "3. Build all (userland + kernel)" -ForegroundColor Green
    Write-Host "4. Run in QEMU" -ForegroundColor Yellow
    Write-Host "5. Clean build" -ForegroundColor Red
    Write-Host "6. Exit" -ForegroundColor Gray
    Write-Host ""
}

function Build-Userland {
    Write-Host "[1/2] Building shell..." -ForegroundColor Cyan
    cargo build --target x86_64-rany_os -p shell --release
    if ($LASTEXITCODE -ne 0) {
        Write-Host "Failed to build shell" -ForegroundColor Red
        return $false
    }
    
    Write-Host "[2/2] Converting to binary..." -ForegroundColor Cyan
    rust-objcopy --output-target=binary target\x86_64-rany_os\release\shell kernel\src\shell.bin
    if ($LASTEXITCODE -ne 0) {
        Write-Host "Warning: objcopy failed, kernel will use fallback" -ForegroundColor Yellow
    }
    
    Write-Host "Userland build complete!" -ForegroundColor Green
    return $true
}

function Build-Kernel {
    Write-Host "Building kernel..." -ForegroundColor Cyan
    cargo build -p tiny_os
    if ($LASTEXITCODE -ne 0) {
        Write-Host "Failed to build kernel" -ForegroundColor Red
        return $false
    }
    
    Write-Host "Kernel build complete!" -ForegroundColor Green
    return $true
}

function Build-All {
    if (Build-Userland) {
        Build-Kernel
    }
}

function Run-QEMU {
    if (Test-Path ".\run_qemu.ps1") {
        Write-Host "Starting QEMU..." -ForegroundColor Cyan
        .\run_qemu.ps1
    }
    else {
        Write-Host "run_qemu.ps1 not found" -ForegroundColor Red
        Write-Host "Please create run_qemu.ps1 or run QEMU manually:" -ForegroundColor Yellow
        Write-Host "  qemu-system-x86_64 -drive format=raw,file=target/x86_64-rany_os/debug/bootimage-tiny_os.bin" -ForegroundColor Gray
    }
}

function Clean-Build {
    Write-Host "Cleaning build artifacts..." -ForegroundColor Yellow
    cargo clean
    Remove-Item -Path "kernel\src\shell.bin" -ErrorAction SilentlyContinue
    Write-Host "Clean complete!" -ForegroundColor Green
}

# Main loop
while ($true) {
    Show-Menu
    $choice = Read-Host "Select option"
    
    switch ($choice) {
        "1" { Build-Userland }
        "2" { Build-Kernel }
        "3" { Build-All }
        "4" { Run-QEMU }
        "5" { Clean-Build }
        "6" {
            Write-Host "Goodbye!" -ForegroundColor Cyan
            exit
        }
        default {
            Write-Host "Invalid option" -ForegroundColor Red
        }
    }
    
    Write-Host ""
    Write-Host "Press any key to continue..."
    $null = $Host.UI.RawUI.ReadKey("NoEcho,IncludeKeyDown")
    Clear-Host
}
