# Minimal x86_64 Rust OS

A minimal, modular operating system kernel written in Rust for x86_64 architecture, featuring hardware-safe I/O, VGA color support, and proper panic handling.

## 🏗️ Architecture

The kernel is organized into well-defined modules for maintainability:

- **`main.rs`**: Kernel entry point and panic handler
- **`constants.rs`**: Centralized configuration values
- **`display.rs`**: Output formatting and presentation logic
- **`init.rs`**: Hardware initialization routines
- **`serial.rs`**: COM1 serial port driver with hardware detection
- **`vga_buffer.rs`**: VGA text mode driver with color support

## 🎯 Features

### 🔧 Real Hardware Support

- **Robust Serial Port Handling**:
  - Automatic hardware presence detection via scratch register test
  - Timeout protection prevents infinite loops (100ms timeout)
  - Graceful degradation to VGA-only on systems without COM1
  - Safe operation on modern motherboards without physical serial ports

- **Fail-Safe Design**:
  - Panic messages always displayed on VGA (even without serial)
  - No CPU hangs from writing to non-existent ports
  - Kernel boots successfully on varied hardware configurations

- **BIOS Compatibility**:
  - Optimized for legacy BIOS text mode at 0xB8000
  - Works with CSM (Compatibility Support Module) in UEFI
  - Clear documentation of platform requirements

## 🎯 Core Features

### ✅ Hardware-Safe Serial I/O

- **UART Initialization**: Full 16550 UART setup (38400 baud, 8N1)
- **FIFO Transmit Check**: Waits for transmit buffer before writing (hardware-compatible)
- **Serial Port**: COM1 (0x3F8) with proper configuration
- **Benefits**: Stable communication on real hardware, no character corruption

### ✅ VGA Text Mode with Color Support

- **Type-Safe Colors**: `ColorCode` struct with predefined color schemes
- **8-Color Palette**: Full VGA 16-color support (foreground/background)
- **Color Functions**:
  - `ColorCode::normal()`: Light gray on black (default)
  - `ColorCode::info()`: Cyan for information
  - `ColorCode::success()`: Green for success messages
  - `ColorCode::warning()`: Yellow for warnings
  - `ColorCode::error()`: Red for errors
  - `ColorCode::panic()`: White on red background for panics
- **Auto-Scroll**: Automatic scrolling when reaching bottom of screen
- **Position Tracking**: Type-safe position management

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

## 🛠️ Code Quality Improvements

### Type Safety

- **ColorCode struct**: Replaces raw `u8` values
- **Position struct**: Type-safe VGA buffer position tracking
- **Explicit types**: All pointer arithmetic uses explicit type annotations

### Constants and Configuration

**Serial Driver (`serial.rs`):**

```rust
// Register offsets organized in module
mod register_offset {
    pub const DATA: u16 = 0;
    pub const INTERRUPT_ENABLE: u16 = 1;
    // ...
}

// Bit masks organized by register
mod line_control {
    pub const DLAB_ENABLE: u8 = 0x80;
    pub const CONFIG_8N1: u8 = 0x03;
}
```

**VGA Driver (`vga_buffer.rs`):**

```rust
const VGA_WIDTH: usize = 80;
const VGA_HEIGHT: usize = 25;
const BYTES_PER_CHAR: usize = 2;
const PRINTABLE_ASCII_START: u8 = 0x20;
```

### Function Decomposition

**Main Kernel (`main.rs`):**

- Separated initialization, display, and loop logic
- Each function has a single, clear responsibility
- Easier to test and maintain

**Example:**

```rust
fn kernel_main(_boot_info: &'static BootInfo) -> ! {
    initialize_system();
    display_boot_information();
    display_feature_list();
    display_usage_note();
    enter_idle_loop()
}
```

### Error Handling

- Explicit error types (e.g., `InitError`)
- Comprehensive documentation of safety requirements
- Clear panic messages with location information

### Documentation

- Module-level documentation with `//!`
- Function-level documentation with examples
- Safety documentation for `unsafe` blocks
- Inline comments for complex logic

## 🚀 Building and Running

### Prerequisites

```bash
# Install toolchain + components (rust-toolchain.toml will auto-prompt if omitted)
rustup toolchain install nightly --component rust-src --component llvm-tools-preview

# Install bootimage
cargo install bootimage

# Install QEMU (Ubuntu/Debian)
sudo apt install qemu-system-x86
```

### Build

```bash
# Debug build
cargo build

# Release build (optimized for size)
cargo build --release

# Create bootable image
cargo bootimage
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

# Run release build
qemu-system-x86_64 \
  -drive format=raw,file=target/x86_64-blog_os/release/bootimage-tiny_os.bin
```

Exit QEMU: Press `Ctrl+A`, then `X`

## 📊 Code Quality

### Safety Improvements

- **Type Safety**: `ColorCode` and `Position` structs prevent type errors
- **FIFO Check**: Prevents serial buffer overflow
- **Const Functions**: Compile-time guarantees for color encoding
- **Interrupt Safety**: `with_writer()` helper prevents deadlocks

### Performance

- **CPU Halt**: Low power consumption in idle
- **Efficient I/O**: FIFO-based serial transmission
- **Minimal Overhead**: Direct hardware access
- **LTO Optimization**: Link-time optimization in release builds
- **Optimized Scrolling**: Uses `copy()` for fast memory operations

### Maintainability

- **Modular Functions**: Each feature in separate, focused function
- **Comprehensive Documentation**: Module and function-level docs
- **Color Abstractions**: Easy-to-use color methods
- **Constants**: All magic numbers replaced with named constants
- **Organized Code**: Related constants grouped in modules

## 🔧 Project Structure

```text
OS/
├── .cargo/
│   └── config.toml              # Build configuration (build-std)
├── src/
│   ├── main.rs                  # Kernel entry point (refactored)
│   ├── serial.rs                # Serial port driver (refactored)
│   └── vga_buffer.rs            # VGA text mode driver (refactored)
├── x86_64-blog_os.json          # Custom target specification
├── Cargo.toml                   # Dependencies and build config
├── build.rs                     # Build script
└── README.md                    # This file
```

## 📝 Key Implementation Details

### Type-Safe Color System

```rust
pub struct ColorCode(u8);

impl ColorCode {
    pub const fn new(fg: VgaColor, bg: VgaColor) -> Self {
        Self((bg as u8) << 4 | (fg as u8))
    }

    pub const fn normal() -> Self { /* ... */ }
    pub const fn error() -> Self { /* ... */ }
    // etc.
}
```

### Position Management

```rust
struct Position {
    row: usize,
    col: usize,
}

impl Position {
    const fn byte_offset(&self) -> usize {
        (self.row * VGA_WIDTH + self.col) * BYTES_PER_CHAR
    }

    fn is_at_screen_bottom(&self) -> bool {
        self.row >= VGA_HEIGHT
    }
}
```

### Interrupt-Safe Writer Access

```rust
fn with_writer<F, R>(f: F) -> R
where
    F: FnOnce(&mut VgaWriter) -> R,
{
    interrupts::without_interrupts(|| f(&mut VGA_WRITER.lock()))
}
```

### Organized Constants

```rust
// Serial port configuration
mod register_offset {
    pub const DATA: u16 = 0;
    pub const LINE_STATUS: u16 = 5;
}

mod line_status {
    pub const TRANSMIT_EMPTY: u8 = 0x20;
}
```

## 🎓 Refactoring Highlights

### Before → After

**Magic Numbers:**

```rust
// Before
port.write(0x80);  // What does this do?

// After
port.write(line_control::DLAB_ENABLE);  // Clear and self-documenting
```

**Type Safety:**

```rust
// Before
fn print_colored(s: &str, color: u8);

// After
fn print_colored(s: &str, color: ColorCode);
```

**Function Size:**

```rust
// Before: kernel_main() with 60+ lines

// After: kernel_main() with 6 clear steps
fn kernel_main(_boot_info: &'static BootInfo) -> ! {
    initialize_system();
    display_boot_information();
    display_feature_list();
    display_usage_note();
    enter_idle_loop()
}
```

## 🔜 Next Steps

Potential improvements:

- [ ] Keyboard input handling
- [ ] Timer/RTC support
- [ ] Interrupt handling (IDT)
- [ ] Memory management (paging, heap allocator)
- [ ] Simple shell/command interpreter
- [ ] File system support
- [ ] Multi-tasking
- [ ] Unit tests with custom test framework
- [ ] Integration tests

## 📄 License

This project is created for educational purposes.

## 🙏 Acknowledgments

- [Writing an OS in Rust](https://os.phil-opp.com/) by Philipp Oppermann
- Rust OS Dev community
- bootloader crate maintainers

---

**Status**: ✅ All features implemented, refactored, and documented
**Platform**: x86_64
**Language**: Rust (nightly)
**Bootloader**: bootloader 0.9.33
**Code Quality**: Type-safe, well-documented, maintainable
