# V2 System Call Interface Specification

> [!NOTE]
> This document describes the **V2 System Call Interface** (Capability-based io_uring).
> For the deprecated V1 interface, see [syscall_interface.md](syscall_interface.md).

## Overview

The V2 system call interface represents a fundamental shift from the traditional file-descriptor-based model to a **Capability-based asynchronous I/O model**.

### Key Features

1. **No Integer File Descriptors**: Resources are accessed via `CapabilityID` (u64 handles).
2. **No `errno`**: Errors are returned as typed `SyscallResult<T>` (Result<T, SyscallError>).
3. **Asynchronous by Default**: All I/O operations are submitted via `io_uring` queues.
4. **Zero-Syscall Operations**: "Doorbell" mechanism allows submitting operations without a syscall in many cases.

## Core System Calls

The V2 interface is minimal, consisting of only 4 core system calls. All other operations (read, write, open, close, etc.) are performed via the `io_uring` submission queue.

### 1. `sys_io_uring_setup` (2002)

Sets up a new `io_uring` context and maps it to user space.

**Arguments:**

* `entries` (u64): Number of entries in the ring (must be power of 2, e.g., 256).
* `flags` (u64): Setup flags (e.g., `SQPOLL` to enable kernel-side polling).

**Returns:**

* `SyscallResult<u64>`: User-space base address of the mapped ring context.

**Usage:**

```rust
let entries = 256;
let flags = 0;
let base_addr = syscall::io_uring_setup(entries, flags)?;
// Initialize Ring struct from base_addr
```

### 2. `sys_io_uring_enter` (2003)

Notifies the kernel that new entries have been added to the submission queue.

**Arguments:**

* `sqe_addr` (u64): Address of the Submission Queue Entry (SQE) - *Note: In V2, this is often unused or used for direct submission optimization*.
* `cqe_addr` (u64): Address to write the Completion Queue Entry (CQE) - *Note: In V2, completions go to the CQ ring*.

**Returns:**

* `SyscallResult<()>`: Success or error.

**Usage:**

```rust
// After writing to SQ ring...
syscall::io_uring_enter(0, 0)?;
```

### 3. `sys_capability_dup` (2004)

Duplicates a capability, potentially with reduced rights.

**Arguments:**

* `capability_id` (u64): The ID of the capability to duplicate.
* `rights` (u64): The bitmask of rights for the new capability (must be a subset of existing rights).

**Returns:**

* `SyscallResult<u64>`: The new `CapabilityID`.

**Usage:**

```rust
let new_cap = syscall::capability_dup(old_cap, Rights::READ | Rights::WRITE)?;
```

### 4. `sys_capability_revoke` (2005)

Revokes a capability, invalidating it and freeing associated resources.

**Arguments:**

* `handle` (u64): The `CapabilityID` to revoke.

**Returns:**

* `SyscallResult<()>`: Success or error.

**Usage:**

```rust
syscall::capability_revoke(cap_id)?;
```

## Data Structures

### SubmissionEntryV2 (64 bytes)

```rust
#[repr(C)]
pub struct SubmissionEntryV2 {
    pub opcode: u8,
    pub flags: u8,
    pub reserved1: u16,
    pub buf_index: u32,
    pub capability_id: u64,
    pub addr: u64,
    pub len: u32,
    pub reserved2: u32,
    pub user_data: u64,
    pub auxiliary: [u64; 2],
}
```

### CompletionEntryV2 (32 bytes)

```rust
#[repr(C)]
pub struct CompletionEntryV2 {
    pub user_data: u64,
    pub result: i32,
    pub flags: u32,
    pub reserved: u64,
}
```

## Error Handling

Errors are returned as `SyscallError` enum variants, not raw integers.

Common errors:

* `SyscallError::InvalidArgument`: Invalid parameters.
* `SyscallError::InvalidHandle`: Invalid capability ID.
* `SyscallError::PermissionDenied`: Insufficient rights.
* `SyscallError::WouldBlock`: Ring buffer full or resource busy.

## Migration Guide

| Feature | V1 (Deprecated) | V2 (Current) |
| :--- | :--- | :--- |
| **Resource Handle** | `fd: u64` (Integer) | `cap: u64` (Capability ID) |
| **I/O Model** | Synchronous (`sys_read`, `sys_write`) | Asynchronous (`io_uring`) |
| **Error Codes** | `errno` (i32, e.g., -22) | `SyscallError` (Enum) |
| **Buffer Passing** | Raw Pointers | Registered Buffers (Preferred) |
