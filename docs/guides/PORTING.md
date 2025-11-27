# Porting Guide

This document provides guidance for porting the kernel to new architectures beyond x86_64.

## Current Architecture Support

### Fully Supported

- **x86_64**: Complete implementation with VGA text mode and serial I/O

### Planned Support

- **AArch64** (ARM 64-bit): Foundation laid in build system
- **RISC-V 64-bit**: Foundation laid in build system
- **x86** (32-bit): Partially supported through conditional compilation

## Platform-Specific Code Locations

### 1. Hardware Access Layer

**Location**: `src/arch/{architecture}/`

Each architecture must implement:

- `cpu.rs`: CPU-specific operations (halt, interrupts, timestamp)
- `serial.rs`: Serial port/UART backend
- `vga.rs` or equivalent display backend
- `keyboard.rs`: Keyboard input handling
- `qemu.rs`: QEMU debug port utilities (optional)

**Example structure**:

```
src/arch/
├── mod.rs              # Architecture selection and trait definitions
├── x86_64/            # x86_64 implementation
│   ├── mod.rs
│   ├── cpu.rs
│   ├── serial.rs
│   ├── vga.rs
│   ├── keyboard.rs
│   └── qemu.rs
└── aarch64/           # Future: ARM 64-bit implementation
    └── ...
```

### 2. Build Configuration

**Files**:

- `build.rs`: Build-time validation and architecture detection
- `Makefile`: Build commands and QEMU configuration
- `{arch}-*.json`: Target specification files

### 3. Hardware Constants

**Files**:

- `src/constants.rs`: Platform-specific I/O addresses and configuration
- `src/vga_buffer/constants.rs`: Display buffer addresses
- `src/serial/constants.rs`: UART register offsets (mostly portable)

## Required Implementations

### CPU Trait (`src/arch/mod.rs`)

All architectures must implement the `Cpu` trait:

```rust
pub trait Cpu {
    /// Halt the CPU until the next interrupt
    fn halt();
    
    /// Disable interrupts
    fn disable_interrupts();
    
    /// Enable interrupts
    fn enable_interrupts();
    
    /// Check if interrupts are enabled
    fn are_interrupts_enabled() -> bool;
}
```

### Serial Backend

Implement either:

- **I/O Port-based** (x86/x86_64): `src/arch/{arch}/serial.rs` using port I/O
- **MMIO-based** (ARM, RISC-V): Memory-mapped UART access

Required interface (from `serial::backend::SerialBackend`):

```rust
pub trait SerialBackend {
    unsafe fn init(&mut self);
    unsafe fn write_byte(&mut self, byte: u8);
    fn is_initialized(&self) -> bool;
}
```

### Display Backend

Implement one of:

- **VGA Text Mode** (x86/x86_64 only): Direct framebuffer access at 0xB8000
- **Framebuffer**: Modern pixel-based graphics (implemented in `src/framebuffer/`)
- **Serial Console**: Use serial port as primary display

**Framebuffer Module (Available):**

The kernel includes a complete framebuffer implementation ready for integration:

- **Location**: `src/framebuffer/`
- **Features**:
  - RGB/BGR/U8 pixel format support
  - 8x16 bitmap font rendering (ASCII 32-126)
  - Text writer with auto-scrolling
  - VGA 16-color palette mapping
- **Status**: Ready for bootloader integration
- **Integration**: Requires framebuffer info from bootloader (UEFI/Multiboot2)

**Display Abstraction Layer:**

The `DisplayHardware` trait (`src/display/backend.rs`) provides unified interface:

```rust
trait DisplayHardware {
    fn is_available(&self) -> bool;
    fn write_colored(&mut self, text: &str, color: ColorCode) -> Result<()>;
    fn clear(&mut self) -> Result<()>;
    fn set_color(&mut self, color: ColorCode) -> Result<()>;
}
```

Current implementations:

- `VgaDisplay`: VGA text mode (x86/x86_64)
- `FramebufferDisplay`: Pixel-based rendering (ready for activation)
- `StubDisplay`: Fallback no-op implementation

## Adding a New Architecture

### Step 1: Create Target Specification

Create `{arch}-blog_os.json` in the project root:

```json
{
    "llvm-target": "{arch}-unknown-none",
    "data-layout": "...",
    "arch": "{arch}",
    "os": "none",
    "target-pointer-width": 64,
    "disable-redzone": true,
    "panic-strategy": "abort",
    ...
}
```

### Step 2: Implement Architecture Module

Create `src/arch/{arch}/`:

```bash
mkdir src/arch/{arch}
touch src/arch/{arch}/mod.rs
touch src/arch/{arch}/cpu.rs
touch src/arch/{arch}/serial.rs
```

Implement required components in each file.

### Step 3: Update Build System

1. **Add to supported architectures** in `build.rs`:

```rust
const SUPPORTED_ARCHITECTURES: &[&str] = &[
    "x86_64", "aarch64", "riscv64", /* new arch */
];
```

2. **Add architecture-specific validation** in `build.rs`:

```rust
fn validate_architecture_compatibility(arch: &str, pointer_width: u16) -> bool {
    match (arch, pointer_width) {
        // ... existing cases ...
        ("{arch}", {width}) => true,
        _ => false,
    }
}
```

3. **Configure QEMU** in `Makefile`:

```makefile
else ifeq ($(ARCH),{arch})
    QEMU_MACHINE ?= -machine {machine}
    QEMU_CPU ?= -cpu {cpu_model}
    QEMU_ARCH_FLAGS = $(QEMU_MACHINE) $(QEMU_CPU)
endif
```

### Step 4: Add Platform Constants

In `src/constants.rs`, add architecture-specific constants:

```rust
#[cfg(target_arch = "{arch}")]
pub const SERIAL_MMIO_BASE: usize = 0x...; // UART base address

#[cfg(target_arch = "{arch}")]
pub const CONSOLE_BASE: usize = 0x...; // Framebuffer or console
```

### Step 5: Build and Test

```bash
# Set architecture
export ARCH={arch}

# Clean previous builds
cargo clean

# Build for new architecture
cargo build --target {arch}-blog_os.json

# Run in QEMU
make run ARCH={arch}
```

## Architecture-Specific Notes

### AArch64 (ARM 64-bit)

**Hardware**:

- UART: PL011 UART (MMIO-based)
- QEMU virt machine: UART at 0x09000000
- No VGA text mode - use framebuffer or serial console

**Required changes**:

- Implement MMIO-based UART driver
- Replace VGA with framebuffer or serial-only output
- Implement ARM-specific interrupt handling

### RISC-V 64-bit

**Hardware**:

- UART: 16550-compatible (MMIO-based)
- QEMU virt machine: UART at 0x10000000
- No VGA - use serial console

**Required changes**:

- Implement MMIO UART (similar to 16550 on x86)
- Serial-only console output
- RISC-V interrupt and exception handling

### x86 (32-bit)

**Status**: Partially supported through conditional compilation

**Required changes**:

- 32-bit pointer handling
- Adjust memory layout for 32-bit address space
- Test on 32-bit QEMU

## Testing Checklist

For each new architecture:

- [ ] Kernel builds without errors
- [ ] Target specification validates in `build.rs`
- [ ] QEMU launches kernel image  
- [ ] Serial output appears
- [ ] Display output works (if applicable)
- [ ] Panic handler produces output
- [ ] Interrupt handling works
- [ ] Tests pass (if applicable to architecture)

## Resources

- [LLVM Target Triples](https://clang.llvm.org/docs/CrossCompilation.html)
- [Rust Target Specifications](https://rust-lang.github.io/rfcs/0131-target-specification.html)
- [QEMU System Emulation](https://www.qemu.org/docs/master/system/targets.html)
- [OSDev Wiki](https://wiki.osdev.org/)

## Support

For questions or assistance with porting:

1. Review existing x86_64 implementation in `src/arch/x86_64/`
2. Check build system validation in `build.rs`
3. Examine platform-specific constants in `src/constants.rs`
