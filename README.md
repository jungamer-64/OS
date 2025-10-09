# Minimal x86_64 Rust OS

A minimal operating system kernel written in Rust for x86_64 architecture, featuring hardware-safe I/O, VGA color support, and proper panic handling.

## 🎯 Features

### ✅ Hardware-Safe Serial I/O

- **UART Initialization**: Full 16550 UART setup (38400 baud, 8N1)
- **FIFO Transmit Check**: Waits for transmit buffer before writing (hardware-compatible)
- **Serial Port**: COM1 (0x3F8) with proper configuration
- **Benefits**: Stable communication on real hardware, no character corruption

### ✅ VGA Text Mode with Color Support

- **8-Color Palette**: Full VGA 16-color support (foreground/background)
- **Color Functions**:
  - `COLOR_NORMAL`: Light gray on black (default)
  - `COLOR_INFO`: Cyan for information
  - `COLOR_SUCCESS`: Green for success messages
  - `COLOR_WARNING`: Yellow for warnings
  - `COLOR_ERROR`: Red for errors
  - `COLOR_PANIC`: White on red background for panics
- **Auto-Scroll**: Automatic scrolling when reaching bottom of screen
- **Cursor Management**: Tracks position (VGA_ROW, VGA_COL)

### ✅ Power Management

- **CPU Halt**: Uses `hlt` instruction in main loop
- **Low Power Mode**: CPU sleeps until next interrupt
- **Efficiency**: No busy-waiting, minimal power consumption

### ✅ Advanced Panic Handler

- **Detailed Information**:
  - Panic message
  - File name, line number, column
  - Dual output (serial + VGA)
- **Visual Indicators**:
  - Prominent color-coded display
  - Box-drawing characters for serial output
  - Easy-to-spot red background on VGA
- **Safe Halt**: CPU halted with `hlt` after panic

## 🛠️ Technical Implementation

### Serial Port Initialization

```rust
fn serial_init() {
    // Disable interrupts
    // Enable DLAB (set baud rate divisor)
    // Set divisor to 3 (38400 baud)
    // 8 bits, no parity, one stop bit (8N1)
    // Enable FIFO, clear them
    // IRQs enabled, RTS/DSR set
}
```

### FIFO Transmit Check

```rust
fn serial_wait_transmit_empty() {
    // Wait until bit 5 (transmit buffer empty) is set
    while (line_status_port.read() & 0x20) == 0 {
        core::hint::spin_loop();
    }
}
```

### VGA Color System

```rust
enum VgaColor {
    Black, Blue, Green, Cyan, Red, Magenta, Brown, LightGray,
    DarkGray, LightBlue, LightGreen, LightCyan,
    LightRed, Pink, Yellow, White
}

const fn vga_color_code(fg: VgaColor, bg: VgaColor) -> u8 {
    (bg as u8) << 4 | (fg as u8)
}
```

## 🚀 Building and Running

### Prerequisites

```bash
# Install Rust nightly
rustup default nightly

# Install required components
rustup component add rust-src llvm-tools-preview

# Install bootimage
cargo install bootimage

# Install QEMU (Ubuntu/Debian)
sudo apt install qemu-system-x86
```

### Build

```bash
# Build kernel
cargo +nightly build

# Create bootable image
cargo +nightly bootimage
```

### Run

```bash
# Run with serial output only (headless)
qemu-system-x86_64 \
  -drive format=raw,file=target/x86_64-blog_os/debug/bootimage-tiny_os.bin \
  -serial stdio \
  -display none

# Run with VGA display
qemu-system-x86_64 \
  -drive format=raw,file=target/x86_64-blog_os/debug/bootimage-tiny_os.bin
```

Exit QEMU: Press `Ctrl+A`, then `X`

## 📊 Code Quality

### Safety Improvements

- **FIFO Check**: Prevents serial buffer overflow
- **Type Safety**: Explicit type annotations for pointer arithmetic
- **Color Encapsulation**: Type-safe color system with enums

### Performance

- **CPU Halt**: Low power consumption in idle
- **Efficient I/O**: FIFO-based serial transmission
- **Minimal Overhead**: Direct hardware access

### Maintainability

- **Modular Functions**: Each feature in separate function
- **Documentation**: Comprehensive inline comments
- **Color Abstractions**: Easy-to-use color functions

## 🔧 Project Structure

```
OS/
├── .cargo/
│   └── config.toml          # Build configuration (build-std)
├── src/
│   └── main.rs              # Kernel source code (227 lines)
├── x86_64-blog_os.json      # Custom target specification
├── Cargo.toml               # Dependencies (bootloader 0.9, x86_64)
└── README.md                # This file
```

## 📝 Key Implementation Details

### Memory Layout

- **VGA Buffer**: `0xb8000` (text mode, 80x25 characters)
- **Serial Port**: `0x3F8` (COM1)
- **Character Format**: 2 bytes per character (ASCII + color attribute)

### Boot Process

1. Bootloader (bootloader 0.9.33) loads kernel
2. Kernel entry via `entry_point!` macro
3. Serial port initialization
4. VGA screen clear and setup
5. Display welcome messages
6. Enter low-power `hlt` loop

### Error Handling

- **Compile-time**: `#![no_std]` ensures no standard library dependencies
- **Runtime**: Panic handler catches all panics
- **Hardware**: FIFO checks prevent I/O errors

## 🎓 Learning Resources

This project demonstrates:

- **Bare-metal Programming**: No OS beneath
- **Hardware Control**: Direct I/O port access
- **Memory Management**: Raw pointer manipulation
- **Rust Safety**: `unsafe` blocks with safe abstractions
- **OS Development**: Bootloader integration, VGA text mode

## 🔜 Next Steps

Potential improvements:

- [ ] Keyboard input handling
- [ ] Timer/RTC support
- [ ] Interrupt handling (IDT)
- [ ] Memory management (paging, heap allocator)
- [ ] Simple shell/command interpreter
- [ ] File system support
- [ ] Multi-tasking

## 📄 License

This project is created for educational purposes.

## 🙏 Acknowledgments

- [Writing an OS in Rust](https://os.phil-opp.com/) by Philipp Oppermann
- Rust OS Dev community
- bootloader crate maintainers

---

**Status**: ✅ All features implemented and tested
**Platform**: x86_64
**Language**: Rust (nightly)
**Bootloader**: bootloader 0.9.33
