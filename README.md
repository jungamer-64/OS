# Tiny OS - Kernel and Userland Separation Project

A minimal OS kernel written in Rust with strict kernel/userland separation.

## Project Structure

```
.
├── kernel/                 # OS Kernel (Ring 0)
│   ├── src/
│   │   ├── main.rs        # Kernel entry point
│   │   ├── kernel/        # Kernel modules
│   │   │   ├── syscall/   # System call implementation
│   │   │   ├── process/   # Process management
│   │   │   └── mm/        # Memory management
│   │   └── arch/          # Architecture-specific code
│   └── build.rs           # Kernel build script
│
├── userland/              # Userland Programs (Ring 3)
│   ├── libuser/           # User standard library
│   │   └── src/
│   │       ├── syscall.rs # System call wrappers
│   │       ├── io.rs      # I/O functions
│   │       ├── process.rs # Process management
│   │       ├── mem.rs     # Memory management
│   │       ├── alloc.rs   # Global allocator
│   │       └── lib.rs     # Library root
│   └── programs/          # User programs
│       └── shell/         # Interactive shell
│
├── docs/                  # Documentation
│   └── syscall_interface.md  # System call specification
│
├── build_userland.ps1     # Userland build script
└── Cargo.toml             # Workspace configuration
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

- [System Call Interface](docs/syscall_interface.md) - Complete syscall specification
- [Implementation Plan](docs/implementation_plan.md) - Development roadmap (if exists)
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
