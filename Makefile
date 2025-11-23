# Makefile for Rust OS Kernel
#
# Provides convenient commands for building, running, and testing the kernel.
# Requires: cargo, QEMU, rust nightly toolchain

# ============================================================================
# Configuration
# ============================================================================

KERNEL_NAME = tiny_os

# Architecture selection (can be overridden for multi-architecture support)
# Currently fully implemented: x86_64
# Future support planned: aarch64, riscv64
ARCH ?= x86_64

# Target architecture specification
# Can be overridden via environment variable for future multi-arch support
TARGET ?= kernel/x86_64-rany_os.json
BUILD_MODE ?= debug

# Directories
BUILD_DIR = target/$(shell basename $(TARGET) .json)/$(BUILD_MODE)
KERNEL_BIN = $(BUILD_DIR)/$(KERNEL_NAME)

# QEMU configuration (architecture-specific)
# Automatically selects the correct QEMU system based on ARCH variable
QEMU ?= qemu-system-$(ARCH)

# Architecture-specific QEMU flags
ifeq ($(ARCH),x86_64)
    QEMU_MACHINE ?=
    QEMU_CPU ?=
    QEMU_ARCH_FLAGS =
else ifeq ($(ARCH),aarch64)
    QEMU_MACHINE ?= -machine virt
    QEMU_CPU ?= -cpu cortex-a57
    QEMU_ARCH_FLAGS = $(QEMU_MACHINE) $(QEMU_CPU)
else ifeq ($(ARCH),riscv64)
    QEMU_MACHINE ?= -machine virt
    QEMU_CPU ?=
    QEMU_ARCH_FLAGS = $(QEMU_MACHINE)
endif

QEMU_FLAGS = -drive format=raw,file=$(KERNEL_BIN) \
             -serial stdio \
             -display gtk \
             -m 128M \
             $(QEMU_ARCH_FLAGS)

# QEMU flags for different modes
QEMU_FLAGS_DEBUG = $(QEMU_FLAGS) -s -S
QEMU_FLAGS_TEST = $(QEMU_FLAGS) -device isa-debug-exit,iobase=0xf4,iosize=0x04 -display none

# Colors for output
COLOR_RESET = \033[0m
COLOR_BOLD = \033[1m
COLOR_GREEN = \033[32m
COLOR_YELLOW = \033[33m
COLOR_BLUE = \033[34m

# ============================================================================
# Phony targets
# ============================================================================

.PHONY: all build run run-release clean test check doc help
.PHONY: clippy fmt fmt-check install-deps

# ============================================================================
# Default target
# ============================================================================

all: build

# ============================================================================
# Build targets
# ============================================================================

## build: Build the kernel (debug mode)
build:
	@echo "$(COLOR_BOLD)$(COLOR_BLUE)Building kernel (debug)...$(COLOR_RESET)"
	@cargo build -p tiny_os --target $(TARGET)
	@echo "$(COLOR_GREEN)✓ Build complete$(COLOR_RESET)"
	@echo "Binary: $(KERNEL_BIN)"

## build-release: Build the kernel (release mode)
build-release:
	@echo "$(COLOR_BOLD)$(COLOR_BLUE)Building kernel (release)...$(COLOR_RESET)"
	@cargo build --release -p tiny_os --target $(TARGET)
	@echo "$(COLOR_GREEN)✓ Build complete$(COLOR_RESET)"
	@echo "Binary: $(KERNEL_BIN)"

## clean: Remove build artifacts
clean:
	@echo "$(COLOR_YELLOW)Cleaning build artifacts...$(COLOR_RESET)"
	@cargo clean
	@echo "$(COLOR_GREEN)✓ Clean complete$(COLOR_RESET)"

# ============================================================================
# Run targets
# ============================================================================

## run: Build and run the kernel in QEMU (debug mode)
run:
	@echo "$(COLOR_BOLD)$(COLOR_GREEN)Starting kernel in QEMU...$(COLOR_RESET)"
	@echo "$(COLOR_YELLOW)Press Ctrl+A, X to exit QEMU$(COLOR_RESET)"
	@cargo run -p builder

## run-release: Build and run the kernel in QEMU (release mode)
run-release: build-release
	@echo "$(COLOR_BOLD)$(COLOR_GREEN)Starting kernel in QEMU (release)...$(COLOR_RESET)"
	@echo "$(COLOR_YELLOW)Press Ctrl+A, X to exit QEMU$(COLOR_RESET)"
	@# Note: builder currently defaults to debug, manual QEMU invocation for release
	@$(QEMU) $(QEMU_FLAGS)

## debug: Build and run the kernel with QEMU debugger (waits for GDB)
debug: build
	@echo "$(COLOR_BOLD)$(COLOR_BLUE)Starting kernel in debug mode...$(COLOR_RESET)"
	@echo "$(COLOR_YELLOW)Waiting for GDB connection on localhost:1234$(COLOR_RESET)"
	@echo "$(COLOR_YELLOW)In another terminal, run: gdb $(KERNEL_BIN) -ex 'target remote localhost:1234'$(COLOR_RESET)"
	@$(QEMU) $(QEMU_FLAGS_DEBUG)

# ============================================================================
# Test targets
# ============================================================================

## test: Run all tests
test:
	@echo "$(COLOR_BOLD)$(COLOR_BLUE)Running tests...$(COLOR_RESET)"
	@cargo test --lib
	@echo "$(COLOR_GREEN)✓ All tests passed$(COLOR_RESET)"

## check: Run cargo check (fast validation)
check:
	@echo "$(COLOR_BOLD)$(COLOR_BLUE)Checking code...$(COLOR_RESET)"
	@cargo check
	@echo "$(COLOR_GREEN)✓ Check complete$(COLOR_RESET)"

## clippy: Run clippy lints
clippy:
	@echo "$(COLOR_BOLD)$(COLOR_BLUE)Running clippy...$(COLOR_RESET)"
	@cargo clippy -- -D warnings
	@echo "$(COLOR_GREEN)✓ Clippy checks passed$(COLOR_RESET)"

# ============================================================================
# Format targets
# ============================================================================

## fmt: Format all source code
fmt:
	@echo "$(COLOR_BOLD)$(COLOR_BLUE)Formatting code...$(COLOR_RESET)"
	@cargo fmt
	@echo "$(COLOR_GREEN)✓ Format complete$(COLOR_RESET)"

## fmt-check: Check if code is properly formatted
fmt-check:
	@echo "$(COLOR_BOLD)$(COLOR_BLUE)Checking formatting...$(COLOR_RESET)"
	@cargo fmt -- --check
	@echo "$(COLOR_GREEN)✓ Format check passed$(COLOR_RESET)"

# ============================================================================
# Documentation targets
# ============================================================================

## doc: Generate documentation
doc:
	@echo "$(COLOR_BOLD)$(COLOR_BLUE)Generating documentation...$(COLOR_RESET)"
	@cargo doc --no-deps --document-private-items
	@echo "$(COLOR_GREEN)✓ Documentation generated$(COLOR_RESET)"
	@echo "Open: target/doc/$(KERNEL_NAME)/index.html"

## doc-open: Generate and open documentation
doc-open:
	@echo "$(COLOR_BOLD)$(COLOR_BLUE)Generating and opening documentation...$(COLOR_RESET)"
	@cargo doc --no-deps --document-private-items --open

# ============================================================================
# Dependency management
# ============================================================================

## install-deps: Install required dependencies
install-deps:
	@echo "$(COLOR_BOLD)$(COLOR_BLUE)Installing dependencies...$(COLOR_RESET)"
	@rustup component add rust-src llvm-tools-preview
	@rustup component add clippy rustfmt
	@echo "$(COLOR_GREEN)✓ Dependencies installed$(COLOR_RESET)"

## update-deps: Update dependencies
update-deps:
	@echo "$(COLOR_BOLD)$(COLOR_BLUE)Updating dependencies...$(COLOR_RESET)"
	@cargo update
	@echo "$(COLOR_GREEN)✓ Dependencies updated$(COLOR_RESET)"

# ============================================================================
# Analysis targets
# ============================================================================

## size: Show binary size information
size: build
	@echo "$(COLOR_BOLD)$(COLOR_BLUE)Binary size analysis:$(COLOR_RESET)"
	@ls -lh $(KERNEL_BIN)
	@file $(KERNEL_BIN)

## bloat: Analyze binary bloat (requires cargo-bloat)
bloat: build
	@echo "$(COLOR_BOLD)$(COLOR_BLUE)Analyzing binary bloat...$(COLOR_RESET)"
	@cargo bloat --release || echo "$(COLOR_YELLOW)Install cargo-bloat: cargo install cargo-bloat$(COLOR_RESET)"

# ============================================================================
# Continuous Integration targets
# ============================================================================

## ci: Run all CI checks
ci: fmt-check clippy test build
	@echo "$(COLOR_BOLD)$(COLOR_GREEN)✓ All CI checks passed$(COLOR_RESET)"

# ============================================================================
# Help target
# ============================================================================

## help: Show this help message
help:
	@echo "$(COLOR_BOLD)Available targets:$(COLOR_RESET)"
	@echo ""
	@grep -E '^## ' $(MAKEFILE_LIST) | sed 's/^## /  /' | column -t -s ':'
	@echo ""
	@echo "$(COLOR_BOLD)Examples:$(COLOR_RESET)"
	@echo "  make build         # Build the kernel"
	@echo "  make run           # Build and run in QEMU"
	@echo "  make test          # Run tests"
	@echo "  make ci            # Run all CI checks"
	@echo ""
	@echo "$(COLOR_BOLD)Configuration:$(COLOR_RESET)"
	@echo "  BUILD_MODE         # Set to 'debug' or 'release' (default: debug)"
	@echo ""
