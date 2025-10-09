# tiny_os

Minimal x86_64 no_std Rust kernel scaffold.

Prerequisites:
- Rust toolchain (nightly recommended)
- cargo-binutils, bootimage (optional), qemu

Quick build (if bootimage is installed):

```bash
rustup default stable
cargo install bootimage --version 0.10.6 || true
cargo bootimage
qemu-system-x86_64 -drive format=raw,file=target/x86_64-blog_os/debug/bootimage-tiny_os.bin
```

If not using bootimage, you can still link and run the kernel with a custom linker and qemu, but setup is more involved.
