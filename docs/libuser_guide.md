# libuser API Guide

## Overview

`libuser` is the standard userland library for programs running in user mode (Ring 3) on Tiny OS.

## Module Reference

### `syscall` - System Call Interface

Low-level system call wrappers.

```rust
use libuser::syscall;

let result = syscall::write(1, b"Hello\n")?;
```

### `io` - I/O Functions

High-level I/O operations.

```rust
use libuser::{println, io};

println!("Hello, World!");
let mut buf = [0u8; 64];
let n = io::read(0, &mut buf)?;
```

### `process` - Process Management

```rust
use libuser::process;

// Get PID
let pid = process::getpid();

// Fork
match process::fork()? {
    0 => { /* child */ },
    pid => { /* parent */ },
}

// Spawn
let child = process::spawn("/bin/program")?;
process::wait(-1, None)?;
```

### `mem` - Memory Management

```rust
use libuser::mem;

// Allocate memory
let addr = mem::alloc(4096)?;

// RAII wrapper
let region = mem::MemoryRegion::new(8192)?;
```

## Examples

See `userland/programs/test` for comprehensive examples.
