# RanY OS ABI

Shared Application Binary Interface (ABI) definitions for RanY OS kernel and userspace.

This crate provides type-safe definitions that are shared between the kernel and userspace programs:

- **SyscallError**: Type-safe error codes replacing traditional errno
- **SyscallNumber**: Type-safe syscall numbers
- **Handle types**: Zero-cost typed handles for resources (files, sockets, etc.)
- **io_uring types**: Submission/completion queue entry definitions
- **AbiResult**: Safe Result type for crossing the ABI boundary

## Usage

In kernel:
```toml
[dependencies]
rany_os_abi = { path = "../rany_os_abi", features = ["kernel"] }
```

In userspace:
```toml
[dependencies]
rany_os_abi = { path = "../rany_os_abi", features = ["userspace"] }
```

## Design Philosophy

- **No C compatibility**: Uses Rust's native representations for efficiency
- **Type safety first**: All types are strongly typed with compile-time checks
- **Zero-cost abstractions**: No runtime overhead for type safety
- **Move semantics**: Resources are automatically managed
