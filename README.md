# Tiny OS - Kernel and Userland Separation Project

A minimal OS kernel written in Rust with strict kernel/userland separation.

## Project Structure

```
.
├── crates/                 # All Rust crates
│   ├── kernel/            # OS Kernel (Ring 0)
│   │   ├── src/
│   │   │   ├── main.rs    # Kernel entry point
│   │   │   ├── kernel/    # Kernel modules
│   │   │   │   ├── syscall/   # System call implementation
│   │   │   │   ├── process/   # Process management
│   │   │   │   └── mm/        # Memory management
│   │   │   └── arch/      # Architecture-specific code
│   │   └── build.rs       # Kernel build script
│   │
│   ├── libuser/           # User standard library (Ring 3)
│   │   └── src/
│   │       ├── syscall.rs # System call wrappers
│   │       ├── io.rs      # I/O functions
│   │       ├── process.rs # Process management
│   │       ├── mem.rs     # Memory management
│   │       ├── alloc.rs   # Global allocator
│   │       └── lib.rs     # Library root
│   │
│   └── programs/          # User programs (Ring 3)
│       ├── shell/         # Interactive shell
│       ├── init/          # Init process
│       ├── test/          # Test program
│       ├── syscall_test/  # Syscall tests
│       ├── io_uring_test/ # io_uring tests
│       └── hello/         # Hello world program
│
├── docs/                  # Documentation
│   ├── design/           # Design documents
│   │   ├── syscall_interface.md
│   │   ├── RING_SEPARATION_DESIGN.md
│   │   └── ...
│   ├── guides/           # Development guides
│   │   ├── SAFETY_GUIDELINES.md
│   │   ├── RUN_QEMU_COMMAND_REFERENCE.md
│   │   └── ...
│   ├── implementation/   # Implementation details
│   └── changelogs/       # Change logs
│
├── tools/                 # Development tools
│   ├── build/            # Build tools
│   │   ├── builder/
│   │   └── mkcpio/
│   ├── scripts/          # Build and test scripts
│   │   ├── run_qemu.ps1
│   │   ├── run_qemu.sh
│   │   └── generate_font.py
│   ├── mockbin/
│   └── tests/
│
├── tests/                 # Integration tests
│   ├── integration/      # Integration test suite
│   │   ├── basic_boot.rs
│   │   ├── vga_buffer.rs
│   │   └── ...
│   └── README.md         # Test documentation
│
├── assets/               # Project assets
│   ├── firmware/        # UEFI firmware
│   │   └── ovmf-x64/
│   └── fonts/           # Font resources
│
├── run_qemu.ps1          # Wrapper script (forwards to tools/scripts/)
└── Cargo.toml            # Workspace configuration
```

## System Requirements

- **Rust**: Nightly toolchain
- **Components**: `rust-src`, `llvm-tools-preview`
- **Tools**: `rust-objcopy` or `llvm-objcopy`
- **QEMU**: For testing (optional)

### Installation

```powershell
# Install Rust nightly
rustup default nightly

# Add required components
rustup component add rust-src
rustup component add llvm-tools-preview
```

## Building

### Quick Build

```powershell
# Build everything (userland + kernel)
cargo build -p shell --release
cargo build -p tiny_os
```

### Using the Build Script

```powershell
# Build userland programs
.\build_userland.ps1

# Build kernel
cargo build -p tiny_os
```

### Step-by-Step Build

```powershell
# 1. Build libuser (user standard library)
cargo build -p libuser

# 2. Build shell (requires libuser)
cargo build --target x86_64-rany_os -p shell --release

# 3. Build kernel (embeds shell.bin)
cargo build -p tiny_os
```

## Running

```powershell
# Run in QEMU
.\run_qemu.ps1

# Or manually
qemu-system-x86_64 -drive format=raw,file=target/x86_64-rany_os/debug/bootimage-tiny_os.bin
```

## Architecture

### Kernel-Userland Separation

- **Strict Separation**: Kernel and userland are independent crates
- **System Calls Only**: Communication only through syscalls
- **No Shared Code**: Each side has its own implementation

### System Call Interface

12 system calls defined in `docs/syscall_interface.md`:

- `sys_write`, `sys_read` - I/O operations
- `sys_fork`, `sys_exec`, `sys_wait` - Process management  
- `sys_mmap`, `sys_munmap` - Memory management
- `sys_pipe` - IPC
- `sys_exit`, `sys_getpid` - Process control

### Memory Layout

```
User Space:   0x0000_0000_0000_0000 ~ 0x0000_7FFF_FFFF_FFFF
Kernel Space: 0xFFFF_8000_0000_0000 ~ 0xFFFF_FFFF_FFFF_FFFF
```

## Development Phases

- [x] **Phase 1**: Interface Definition
  - System call specification
  - libuser implementation
  - Type-safe wrappers

- [x] **Phase 2**: Build System Separation
  - Independent builds
  - Binary embedding
  - Build automation

- [ ] **Phase 3**: Security Enhancement
  - User pointer validation
  - Security module
  - Access control

- [ ] **Phase 4**: Process Management
  - Complete fork/exec
  - ELF loader
  - Resource management

- [ ] **Phase 5**: Userland Libraries
  - Init process
  - Shell improvements
  - Additional programs

- [ ] **Phase 6**: Testing & Documentation
  - Integration tests
  - Architecture docs
  - Performance benchmarks

## Key Features

✅ **Type-Safe System Calls**: Rust's type system prevents errors  
✅ **Result-Based Errors**: Proper error handling with `Result<T, SyscallError>`  
✅ **RAII Memory Management**: Automatic resource cleanup with `MemoryRegion`  
✅ **Global Allocator**: Heap allocation support via `MmapAllocator`  
✅ **No External Dependencies**: Pure `no_std` implementation

## Documentation

- [System Call Interface](docs/design/syscall_interface.md) - Complete syscall specification
- [Safety Guidelines](docs/guides/SAFETY_GUIDELINES.md) - Safety and security guidelines
- [Run QEMU Reference](docs/guides/RUN_QEMU_COMMAND_REFERENCE.md) - QEMU execution guide
- Code Documentation - Run `cargo doc --open`

## Testing

```powershell
# Build and test
cargo build -p tiny_os
.\run_qemu.ps1

# Expected output:
# - Kernel boots
# - Shell starts
# - Fork/exec/pipe tests run
# - Processes communicate via IPC
```

## Contributing

This is an educational project demonstrating kernel/userland separation in Rust.

### Code Style

- Follow Rust conventions
- Document all public APIs
- Add safety comments for `unsafe` blocks
- Keep kernel and userland strictly separated

## License

[Specify your license]

## Resources

- [OSDev Wiki](https://wiki.osdev.org/)
- [System V ABI](https://refspecs.linuxbase.org/elf/x86_64-abi-0.99.pdf)
