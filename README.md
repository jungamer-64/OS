# Tiny OS - Minimal Rust Kernel

![Rust Version](https://img.shields.io/badge/rust-nightly-orange.svg)
![License](https://img.shields.io/badge/license-MIT-blue.svg)
![Platform](https://img.shields.io/badge/platform-x86__64-lightgrey.svg)

A minimal, educational operating system kernel written in Rust. This project demonstrates bare-metal programming, hardware interaction, and safe systems programming using Rust.

## 🏛️ Architecture Support

**Currently Supported:**

- ✅ **x86_64** (PC/AT compatible systems) - Fully implemented and tested

**Planned Support** (infrastructure in place):

- 🔨 **AArch64** (ARM 64-bit) - Build system and constants configured
- 🔨 **RISC-V 64-bit** - Build system and constants configured  
- 🔨 **x86** (32-bit) - Partial conditional compilation support

The kernel is designed with an architecture abstraction layer (`src/arch/`) that separates platform-specific code from core logic. The build system (`build.rs`, `Makefile`) has been generalized to support multiple target architectures, and hardware constants are conditionally compiled based on the target platform.

**For Porting Information:** See [docs/PORTING.md](docs/PORTING.md) for a comprehensive guide on adding support for new architectures.

**Architecture-Specific Components:**

- CPU operations (halt, interrupt control, timestamp counter)
- Serial port I/O (x86/x86_64: I/O mapped UART 16550, other: MMIO-based)
- VGA text buffer (x86/x86_64: 0xB8000 legacy standard, other: framebuffer/serial)
- QEMU debug interfaces

## ✨ Features

### Core Functionality

- **VGA Text Mode Output** - 80x25 character display with 16-color support
- **Serial Port (COM1)** - Debug output via UART 16550 at 38400 baud
- **Interrupt-Safe I/O** - Mutex-protected output prevents race conditions
- **Hardware Detection** - Robust detection of VGA and serial hardware
- **Panic Handler** - Detailed error reporting with source location

### Safety & Robustness

- ✅ **Memory Safety** - All buffer accesses bounds-checked
- ✅ **Error Handling** - Comprehensive Result types throughout
- ✅ **Hardware Validation** - Multi-stage hardware presence checks
- ✅ **Timeout Protection** - All blocking operations have timeouts
- ✅ **Deadlock Prevention** - Documented lock ordering, interrupt disabling
- ✅ **Idempotent Init** - Safe to call initialization multiple times
- ✅ **Zero Unsafe** in application code (all unsafe centralized and documented)

### Code Quality

- 📝 **Fully Documented** - Every public API has documentation
- 🧪 **Unit Tests** - Core functionality covered by tests
- 🔍 **Zero Warnings** - Passes clippy and rustfmt checks
- 📊 **Type Safe** - Compile-time validation where possible

## 🚀 Quick Start

### Prerequisites

```bash
# Install Rust nightly toolchain
rustup toolchain install nightly

# Set nightly as default for this project
rustup override set nightly

# Install required components
rustup component add rust-src llvm-tools-preview
rustup component add clippy rustfmt

# Install QEMU for x86_64 emulation
# On Ubuntu/Debian:
sudo apt install qemu-system-x86

# On macOS:
brew install qemu

# On Windows:
# Download from https://www.qemu.org/download/

# For other architectures (future):
# sudo apt install qemu-system-arm qemu-system-misc  # AArch64, RISC-V
```

### Building

```bash
# Build the kernel (debug mode)
make build

# Or using cargo directly
cargo build

# Build release version (optimized)
make build-release
cargo build --release
```

### Running

```bash
# Run in QEMU (x86_64)
make run

# Run release version
make run-release

# Run with GDB debugger (waits for connection)
make debug

# For future: specify architecture
# make run ARCH=aarch64
```

The kernel will boot and display:

- Boot environment information
- System status
- Feature list
- Usage instructions

**To exit QEMU:** Press `Ctrl+A`, then `X`

## 📁 Project Structure

```
tiny_os/
├── src/
│   ├── main.rs              # Kernel entry point
│   ├── constants.rs         # Hardware constants and config
│   ├── init.rs              # Initialization routines
│   ├── serial.rs            # UART serial port driver
│   ├── vga_buffer.rs        # VGA text mode driver
│   └── display/
│       ├── mod.rs           # Display module exports
│       ├── core.rs          # Output abstraction
│       ├── boot.rs          # Boot information display
│       └── panic.rs         # Panic handler display
├── .cargo/
│   └── config.toml          # Cargo configuration
├── x86_64-blog_os.json      # Custom target specification
├── Cargo.toml               # Dependencies and build config
├── rust-toolchain.toml      # Rust toolchain specification
├── Makefile                 # Build automation
└── README.md                # This file
```

## 🏗️ Architecture

### Boot Process

```
Bootloader (bootloader 0.9)
    ↓
kernel_main() in main.rs
    ↓
initialize_all() in init.rs
    ├── initialize_vga() - Clear screen, test buffer
    └── initialize_serial() - Detect and configure COM1
    ↓
Display boot information
    ├── Boot environment
    ├── System info
    ├── Feature list
    └── Usage notes
    ↓
halt_forever() - Enter low-power idle loop
```

### Module Dependencies

```
main.rs
    ├─→ init.rs
    │   ├─→ vga_buffer.rs
    │   └─→ serial.rs
    ├─→ display/
    │   ├─→ core.rs
    │   ├─→ boot.rs
    │   └─→ panic.rs
    └─→ constants.rs
```

### Memory Map

```
0x00000000 - 0x000FFFFF  : Real mode area (1 MB)
0x00100000 - ...         : Kernel code (loaded by bootloader)
0x000B8000 - 0x000B8FA0  : VGA text buffer (80x25x2 = 4000 bytes, PC/AT legacy)
0x000003F8 - 0x000003FF  : COM1 serial port (8 I/O ports, PC/AT standard)
```

## 🔧 Development

### Testing

```bash
# Run unit tests
make test
cargo test --lib

# Run all CI checks (format, clippy, test, build)
make ci
```

### Code Quality

```bash
# Check code (fast, no binary)
make check
cargo check

# Run clippy linter
make clippy
cargo clippy

# Format code
make fmt
cargo fmt

# Check formatting
make fmt-check
cargo fmt -- --check
```

### Documentation

```bash
# Generate documentation
make doc

# Generate and open in browser
make doc-open
cargo doc --open --no-deps --document-private-items
```

### Binary Analysis

```bash
# Show binary size
make size

# Analyze binary bloat (requires cargo-bloat)
cargo install cargo-bloat
make bloat
```

## 🐛 Debugging

### QEMU Monitor

When running in QEMU, press `Ctrl+Alt+2` to access the QEMU monitor:

```
(qemu) info registers    # Show CPU registers
(qemu) info mem          # Show memory mappings
(qemu) info pic          # Show interrupt controller
```

Press `Ctrl+Alt+1` to return to the guest display.

### GDB Debugging

```bash
# Terminal 1: Start kernel with debugger
make debug

# Terminal 2: Connect GDB
gdb target/x86_64-blog_os/debug/tiny_os
(gdb) target remote localhost:1234
(gdb) break kernel_main
(gdb) continue
```

### Serial Output

All kernel messages are sent to both VGA and serial output. Serial output is particularly useful for:

- Detailed logs not shown on VGA
- Panic information with full details
- System state at error time

To capture serial output to a file:

```bash
qemu-system-x86_64 -drive format=raw,file=<kernel> -serial file:serial.log
```

## 📚 Learning Resources

### Rust OS Development

- [Writing an OS in Rust](https://os.phil-opp.com/) - Excellent tutorial series
- [OSDev Wiki](https://wiki.osdev.org/) - Comprehensive OS development reference
- [The Rust Book](https://doc.rust-lang.org/book/) - Learn Rust fundamentals

### x86_64 Architecture

- [Intel® 64 and IA-32 Architectures Software Developer's Manual](https://www.intel.com/content/www/us/en/developer/articles/technical/intel-sdm.html)
- [AMD64 Architecture Programmer's Manual](https://www.amd.com/en/support/tech-docs)

### Hardware Programming

- [OSDev: Serial Ports](https://wiki.osdev.org/Serial_Ports) - UART programming
- [OSDev: VGA Hardware](https://wiki.osdev.org/VGA_Hardware) - VGA text mode

## 🤝 Contributing

Contributions are welcome! Please:

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Make your changes
4. Run tests and checks (`make ci`)
5. Commit your changes (`git commit -m 'Add amazing feature'`)
6. Push to the branch (`git push origin feature/amazing-feature`)
7. Open a Pull Request

### Code Style

- Follow Rust standard style (enforced by `rustfmt`)
- Add documentation for all public APIs
- Write tests for new functionality
- Keep unsafe code minimal and well-documented
- Use meaningful names (no single-letter variables except loop counters)

## 📋 Future Roadmap

### Short Term

- [ ] Interrupt handling (IDT, ISR)
- [ ] Keyboard input (PS/2)
- [ ] Timer (PIT/APIC)

### Medium Term

- [ ] Memory management (paging, heap)
- [ ] Process/task structure
- [ ] Context switching

### Long Term

- [ ] File system support
- [ ] Network stack
- [ ] User mode and system calls

## 📄 License

This project is licensed under the MIT License - see below for details:

```
MIT License

Copyright (c) 2025 [Your Name]

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## 🙏 Acknowledgments

- [Philipp Oppermann](https://os.phil-opp.com/) - For the excellent "Writing an OS in Rust" tutorial
- [Rust Community](https://www.rust-lang.org/community) - For the amazing language and ecosystem
- [OSDev Community](https://wiki.osdev.org/) - For comprehensive OS development documentation

## 📞 Support

- **Issues**: [GitHub Issues](https://github.com/yourusername/tiny_os/issues)
- **Discussions**: [GitHub Discussions](https://github.com/yourusername/tiny_os/discussions)
- **Email**: <your.email@example.com>

---

**Note**: This is an educational project. It is not intended for production use.
