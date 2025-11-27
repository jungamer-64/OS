---
trigger: always_on
---

# run_qemu.ps1 Command Reference for AI Agents

This document provides a comprehensive reference for the `run_qemu.ps1` build and run script used in the tiny_os project. This guide is intended for AI agents to understand and correctly invoke the script.

## Overview

`run_qemu.ps1` is a unified PowerShell build and run script (v2.5) that handles:

- Kernel and userland compilation
- EFI disk image creation
- QEMU virtual machine execution
- Logging and diagnostics

## Basic Usage Patterns

### Quick Build and Run (Most Common)

```powershell
.\run_qemu.ps1
```

Builds the kernel in debug mode and launches QEMU.

### Full Build with All Features

```powershell
.\run_qemu.ps1 -FullBuild -Release -Accel -Network
```

Builds kernel + userland in release mode with hardware acceleration and networking.

### Build Only (No QEMU)

```powershell
.\run_qemu.ps1 -BuildOnly
```

### Run Only (Skip Build)

```powershell
.\run_qemu.ps1 -SkipBuild
```

## Parameters Reference

### Build Control

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `-SkipBuild` | Switch | `$false` | Skip the build phase, use existing artifacts |
| `-FullBuild` | Switch | `$false` | Build userland programs + kernel (not just kernel) |
| `-Release` | Switch | `$false` | Build in release mode (optimized) |
| `-BuildOnly` | Switch | `$false` | Build only, do not launch QEMU |
| `-Clean` | Switch | `$false` | Remove all build artifacts and exit |
| `-Check` | Switch | `$false` | Run `cargo clippy` before building |

### QEMU Execution

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `-Debug` | Switch | `$false` | Enable GDB stub (localhost:1234, waits for debugger) |
| `-NoGraphic` | Switch | `$false` | Run QEMU without GUI (serial to stdio) |
| `-InlineQemu` | Switch | `$false` | Run QEMU in current console with stdout mirroring |
| `-KeepAlive` | Switch | `$false` | Keep QEMU open after crash/shutdown (for debugging) |
| `-Timeout` | Int | `0` | Timeout in seconds (0 = no timeout, wait indefinitely) |

### Hardware Configuration

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `-Memory` | String | `"128M"` | RAM size (e.g., "128M", "256M", "1G", "2G") |
| `-Cores` | Int | `1` | Number of CPU cores |
| `-Accel` | Switch | `$false` | Enable hardware acceleration (WHPX on Windows) |
| `-Network` | Switch | `$false` | Enable user networking (e1000 NIC) |

### Path Overrides

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `-QemuPath` | String | `"qemu-system-x86_64"` | Path to QEMU executable |
| `-OverrideOvmfPath` | String | `""` | Override OVMF firmware path |

### Extra QEMU Arguments

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `-ExtraQemuArgs` | String[] | `@()` | Additional QEMU arguments as array |
| `-ExtraQemuArgStr` | String | `""` | Additional QEMU arguments as string |

### Interactive Mode

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `-Menu` | Switch | `$false` | Launch interactive menu |

## Common Use Cases

### 1. Development Iteration (Fast)

```powershell
# Quick kernel-only build and run
.\run_qemu.ps1
```

### 2. Full System Build

```powershell
# Build everything including userland programs
.\run_qemu.ps1 -FullBuild
```

### 3. Release Build for Testing

```powershell
# Optimized build with acceleration
.\run_qemu.ps1 -Release -Accel
```

### 4. Debugging with GDB

```powershell
# Start with GDB stub enabled
.\run_qemu.ps1 -Debug
# Then connect with: gdb -ex "target remote localhost:1234"
```

### 5. Headless/CI Testing

```powershell
# No GUI, with timeout
.\run_qemu.ps1 -NoGraphic -Timeout 60
```

### 6. Full Featured Build

```powershell
# Everything enabled
.\run_qemu.ps1 -FullBuild -Release -Accel -Network
```

### 7. Code Quality Check

```powershell
# Run clippy + build
.\run_qemu.ps1 -Check -BuildOnly
```

### 8. Clean Build

```powershell
# Clean all artifacts
.\run_qemu.ps1 -Clean

# Then fresh build
.\run_qemu.ps1 -FullBuild
```

### 9. Custom Memory/CPU Configuration

```powershell
# More resources
.\run_qemu.ps1 -Memory "256M" -Cores 2
```

### 10. Extra QEMU Options

```powershell
# Pass additional QEMU flags
.\run_qemu.ps1 -ExtraQemuArgStr "-device virtio-rng-pci"
```

## Output and Logs

The script generates logs in the `logs/` directory:

| File | Description |
|------|-------------|
| `logs/qemu.debug.log` | QEMU debug output (interrupts, CPU resets) |
| `logs/qemu.stdout.log` | QEMU standard output (serial console) |
| `logs/qemu.stderr.log` | QEMU standard error |
| `logs/history/` | Historical log backups (last 20 runs) |

## Exit Codes

| Code | Meaning |
|------|---------|
| `0` | Success |
| `1` | Build or execution error |

## Important Notes for AI Agents

1. **Working Directory**: The script must be run from the repository root (`d:\Rust\OS`).

2. **Prerequisites**:
   - `rustup` must be in PATH
   - QEMU must be installed and accessible
   - OVMF firmware must exist at `ovmf-x64/OVMF.fd`

3. **Build Artifacts**:
   - Debug kernel: `target/x86_64-rany_os/debug/tiny_os`
   - Release kernel: `target/x86_64-rany_os/release/tiny_os`
   - Disk image: `target/x86_64-rany_os/{debug|release}/boot-uefi-tiny_os.img`
   - Initrd: `target/initrd.cpio`

4. **Acceleration Note**: `-Accel` uses Windows WHPX. If WHPX is unavailable, QEMU will fail. Remove this flag if acceleration is not supported.

5. **Timeout Behavior**: When `-Timeout` is set, the script will kill QEMU after the specified seconds. Use `0` (default) for indefinite execution.

6. **Ctrl+C Handling**: The script handles Ctrl+C gracefully and cleans up QEMU processes.

## Example Command Sequences

### Fresh Development Setup

```powershell
.\run_qemu.ps1 -Clean
.\run_qemu.ps1 -FullBuild -Check
```

### CI Pipeline Simulation

```powershell
.\run_qemu.ps1 -FullBuild -Release -NoGraphic -Timeout 120
```

### Interactive Development

```powershell
.\run_qemu.ps1 -Menu
```
