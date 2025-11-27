#!/bin/bash
#
# Unified build and run script for tiny_os (Linux/macOS Version)
# Based on run_qemu.ps1 v2.5
#
# Usage:
#   ./run_qemu.sh -menu                                 # Interactive Mode
#   ./run_qemu.sh                                       # Quick Build (Kernel) -> QEMU
#   ./run_qemu.sh -full-build -accel -network           # Full Build with KVM & Network
#   ./run_qemu.sh -check -build-only                    # Run Clippy & Build only
#   ./run_qemu.sh -skip-build                           # Run QEMU only

# --- Configuration ---
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TARGET_ARCH="x86_64-rany_os"
QEMU_EXEC="qemu-system-x86_64"

# Detect OS
OS_TYPE="$(uname -s)"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
MAGENTA='\033[0;35m'
NC='\033[0m' # No Color

# Defaults
MODE_MENU=false
SKIP_BUILD=false
FULL_BUILD=false
RELEASE_MODE=false
DEBUG_MODE=false
NO_GRAPHIC=false
CLEAN_MODE=false
BUILD_ONLY=false
INLINE_QEMU=false # Linux uses tee, so it's always inline+logged
KEEP_ALIVE=false
TIMEOUT_SEC=0
ENABLE_ACCEL=false
ENABLE_NET=false
RUN_CHECK=false

MEMORY="128M"
CORES=1
EXTRA_QEMU_ARGS=()
OVERRIDE_OVMF=""

# --- Helper Functions ---

log_info() { echo -e "${GREEN}[INFO] $1${NC}"; }
log_warn() { echo -e "${YELLOW}[WARN] $1${NC}"; }
log_err()  { echo -e "${RED}[ERR]  $1${NC}"; }
log_cmd()  { echo -e "${CYAN}> $1${NC}"; }

cleanup() {
    # Kill QEMU if script is interrupted
    if [ -n "$QEMU_PID" ]; then
        if ps -p $QEMU_PID > /dev/null; then
            log_warn "Stopping QEMU process ($QEMU_PID)..."
            kill $QEMU_PID 2>/dev/null
        fi
    fi
}
trap cleanup EXIT INT TERM

check_prereqs() {
    if ! command -v rustup &> /dev/null; then
        log_err "rustup not found in PATH."
        exit 1
    fi
    if ! command -v $QEMU_EXEC &> /dev/null; then
        log_err "$QEMU_EXEC not found in PATH."
        exit 1
    fi
}

find_ovmf() {
    if [ -n "$OVERRIDE_OVMF" ]; then
        echo "$OVERRIDE_OVMF"
        return
    fi

    # 1. Local project path
    local local_path="$(dirname "$SCRIPT_DIR")/assets/firmware/ovmf-x64/OVMF.fd"
    if [ -f "$local_path" ]; then
        echo "$local_path"
        return
    fi

    # 2. Common system paths (Linux and macOS)
    local sys_paths=(
        # Linux paths
        "/usr/share/OVMF/OVMF.fd"
        "/usr/share/ovmf/OVMF.fd"
        "/usr/share/qemu/OVMF.fd"
        "/usr/share/uefi-ovmf/OVMF.fd"
        "/usr/share/edk2-ovmf/x64/OVMF.fd"
        # macOS Homebrew paths
        "/opt/homebrew/share/qemu/edk2-x86_64-code.fd"
        "/usr/local/share/qemu/edk2-x86_64-code.fd"
        "/opt/homebrew/Cellar/qemu/*/share/qemu/edk2-x86_64-code.fd"
    )

    for path in "${sys_paths[@]}"; do
        # Handle glob patterns
        for resolved in $path; do
            if [ -f "$resolved" ]; then
                echo "$resolved"
                return
            fi
        done
    done
    done

    echo ""
}

clean_artifacts() {
    log_info "Cleaning build artifacts..."
    
    # Cargo clean in root, builder, kernel
    local dirs=("$SCRIPT_DIR" "$SCRIPT_DIR/builder" "$SCRIPT_DIR/kernel")
    for d in "${dirs[@]}"; do
        if [ -d "$d" ]; then
            log_cmd "Cleaning $(basename "$d")..."
            (cd "$d" && cargo clean)
        fi
    done

    # Clean userland targets
    local userland_dir="$SCRIPT_DIR/userland"
    if [ -d "$userland_dir" ]; then
        find "$userland_dir" -type d -name "target" -exec rm -rf {} + 2>/dev/null
    fi

    # Clean specific artifacts
    rm -f "$SCRIPT_DIR/target/initrd.cpio"
    rm -rf "$SCRIPT_DIR/target/initrd_root"

    log_info "Clean complete!"
}

run_build() {
    local profile_flag=""
    [ "$RELEASE_MODE" = true ] && profile_flag="--release"
    
    # Clippy Check
    if [ "$RUN_CHECK" = true ]; then
        log_info "Running Cargo Clippy..."
        (
            cd "$SCRIPT_DIR/kernel" || exit 1
            if [ -f "x86_64-rany_os.json" ]; then
                cargo clippy --target "x86_64-rany_os.json"
            else
                cargo clippy
            fi
        ) || log_warn "Clippy issues found."
    fi

    if [ "$FULL_BUILD" = true ]; then
        log_info "Running full build pipeline..."
        (
            cd "$SCRIPT_DIR/builder" || exit 1
            rustup run nightly cargo run $profile_flag
        ) || return 1
    else
        log_info "Building kernel (Quick Mode)..."
        (
            cd "$SCRIPT_DIR/kernel" || exit 1
            if [ ! -f "x86_64-rany_os.json" ]; then
                log_err "x86_64-rany_os.json not found."
                exit 1
            fi
            rustup run nightly cargo build --target "x86_64-rany_os.json" $profile_flag
        ) || return 1

        log_info "Creating EFI disk image..."
        (
            cd "$SCRIPT_DIR/builder" || exit 1
            
            # Construct builder args
            local builder_args=()
            [ "$RELEASE_MODE" = true ] && builder_args+=("--release")
            builder_args+=("--")
            builder_args+=("--kernel-path" "$KERNEL_PATH")
            builder_args+=("--output-path" "$DISK_IMAGE")

            if [ -f "$INITRD_PATH" ]; then
                log_info "  Including initrd: $INITRD_PATH"
                builder_args+=("--ramdisk" "$INITRD_PATH")
            else
                log_warn "  No initrd found."
            fi

            # -Zbuild-std= disables workspace's build-std setting for builder
            rustup run nightly cargo -Zbuild-std= run "${builder_args[@]}"
        ) || return 1
    fi
}

start_qemu() {
    log_info "Starting QEMU..."

    # Setup Logging
    local log_dir="$SCRIPT_DIR/logs"
    local history_dir="$log_dir/history"
    mkdir -p "$history_dir"

    local qemu_log="$log_dir/qemu.debug.log"
    local stdout_log="$log_dir/qemu.stdout.log"
    local stderr_log="$log_dir/qemu.stderr.log"

    # Log Rotation
    local timestamp=$(date +"%Y%m%d-%H%M%S")
    for log in "$qemu_log" "$stdout_log" "$stderr_log"; do
        if [ -f "$log" ]; then
            cp "$log" "$history_dir/$(basename "$log").$timestamp.bak"
        fi
    done
    
    # Cleanup old logs (keep last 20)
    ls -t "$history_dir" | tail -n +61 | xargs -I {} rm "$history_dir/{}" 2>/dev/null

    # Build QEMU Args
    local qargs=()
    qargs+=("-drive" "format=raw,file=$DISK_IMAGE")
    qargs+=("-bios" "$OVMF_PATH")
    qargs+=("-m" "$MEMORY")
    qargs+=("-smp" "$CORES")
    qargs+=("-no-reboot")
    qargs+=("-d" "int,cpu_reset")
    qargs+=("-D" "$qemu_log")

    # Acceleration (KVM on Linux, HVF on macOS)
    if [ "$ENABLE_ACCEL" = true ]; then
        if [ "$OS_TYPE" = "Darwin" ]; then
            # macOS: Use Hypervisor.framework
            log_info "  Acceleration: Enabled (HVF)"
            qargs+=("-accel" "hvf" "-cpu" "host")
        elif [ -e /dev/kvm ]; then
            # Linux: Use KVM
            log_info "  Acceleration: Enabled (KVM)"
            qargs+=("-enable-kvm" "-cpu" "host")
        else
            log_warn "  Acceleration requested but no accelerator found. Falling back to TCG."
        fi
    fi

    # Network
    if [ "$ENABLE_NET" = true ]; then
        log_info "  Network: Enabled (User/NAT, e1000)"
        qargs+=("-netdev" "user,id=net0")
        qargs+=("-device" "e1000,netdev=net0")
    fi

    # Serial / Graphics
    if [ "$NO_GRAPHIC" = true ]; then
        qargs+=("-serial" "mon:stdio")
        qargs+=("-nographic")
    else
        qargs+=("-serial" "stdio")
    fi

    [ "$KEEP_ALIVE" = true ] && qargs+=("-no-shutdown")

    if [ "$DEBUG_MODE" = true ]; then
        log_info "  GDB Stub: localhost:1234"
        qargs+=("-s" "-S")
    fi

    # Append Extra Args
    qargs+=("${EXTRA_QEMU_ARGS[@]}")

    log_cmd "$QEMU_EXEC ${qargs[*]}"
    echo "  (Output mirrored to $stdout_log)"

    # Run QEMU
    # Run in foreground with tee for logging. Ctrl+C will work properly.
    echo "  (Press Ctrl+C to stop)"
    
    if [ "$TIMEOUT_SEC" -gt 0 ]; then
        echo "  (Timeout: ${TIMEOUT_SEC}s)"
        
        # Find timeout command (gtimeout on macOS with coreutils)
        local timeout_cmd=""
        if command -v timeout &> /dev/null; then
            timeout_cmd="timeout"
        elif command -v gtimeout &> /dev/null; then
            timeout_cmd="gtimeout"
        fi
        
        if [ -n "$timeout_cmd" ]; then
            # Run with timeout command
            "$timeout_cmd" --signal=KILL "$TIMEOUT_SEC" "$QEMU_EXEC" "${qargs[@]}" 2> "$stderr_log" | tee "$stdout_log"
            local exit_code=${PIPESTATUS[0]}
            if [ $exit_code -eq 137 ]; then
                log_warn "Timeout reached (${TIMEOUT_SEC}s). QEMU was killed."
            fi
        else
            # Fallback: manual timeout with background process
            log_warn "timeout command not found. Using manual timeout."
            "$QEMU_EXEC" "${qargs[@]}" 2> "$stderr_log" | tee "$stdout_log" &
            QEMU_PID=$!
            local waited=0
            while kill -0 $QEMU_PID 2>/dev/null; do
                sleep 1
                waited=$((waited + 1))
                if [ $waited -ge $TIMEOUT_SEC ]; then
                    log_warn "Timeout reached (${TIMEOUT_SEC}s). Killing QEMU..."
                    kill -9 $QEMU_PID 2>/dev/null
                    break
                fi
            done
            wait $QEMU_PID 2>/dev/null
            local exit_code=$?
        fi
    else
        # Run normally - Ctrl+C works
        "$QEMU_EXEC" "${qargs[@]}" 2> "$stderr_log" | tee "$stdout_log"
        local exit_code=${PIPESTATUS[0]}
    fi
    
    QEMU_PID="" # Clear PID so trap doesn't try to kill it again

    # Show stderr if not empty
    if [ -s "$stderr_log" ]; then
        echo -e "${YELLOW}--- QEMU Stderr ---${NC}"
        cat "$stderr_log"
        echo -e "${YELLOW}-------------------${NC}"
    fi

    return $exit_code
}

show_menu() {
    while true; do
        clear
        echo -e "${CYAN}=======================================${NC}"
        echo -e "${CYAN}      Tiny OS Build System (Linux)     ${NC}"
        echo -e "${CYAN}=======================================${NC}"
        echo ""
        echo -e "${GREEN}  1. Quick build & run (kernel only)${NC}"
        echo -e "${GREEN}  2. Full build & run (userland + kernel)${NC}"
        echo -e "${YELLOW}  3. Build only (no QEMU)${NC}"
        echo -e "${YELLOW}  4. Run only (skip build)${NC}"
        echo -e "${MAGENTA}  5. Debug mode (GDB)${NC}"
        echo -e "${BLUE}  6. Release build & run${NC}"
        echo -e "${RED}  7. Clean build artifacts${NC}"
        echo -e "${NC}  8. Exit${NC}"
        echo ""
        echo -e "${CYAN}=======================================${NC}"
        
        read -p "Select option (1-8): " choice
        
        # Reset per-loop flags
        FULL_BUILD=false; BUILD_ONLY=false; SKIP_BUILD=false
        DEBUG_MODE=false; RELEASE_MODE=false
        
        case $choice in
            1) ;;
            2) FULL_BUILD=true ;;
            3) BUILD_ONLY=true ;;
            4) SKIP_BUILD=true ;;
            5) DEBUG_MODE=true ;;
            6) RELEASE_MODE=true ;;
            7) clean_artifacts; read -p "Press Enter..." ; continue ;;
            8) exit 0 ;;
            *) continue ;;
        esac

        # Execute
        main_logic
        read -p "Press Enter to return to menu..."
    done
}

# --- Argument Parsing ---

while [[ $# -gt 0 ]]; do
    case $1 in
        -menu) MODE_MENU=true; shift ;;
        -skip-build) SKIP_BUILD=true; shift ;;
        -full-build) FULL_BUILD=true; shift ;;
        -release) RELEASE_MODE=true; shift ;;
        -debug) DEBUG_MODE=true; shift ;;
        -nographic) NO_GRAPHIC=true; shift ;;
        -clean) CLEAN_MODE=true; shift ;;
        -build-only) BUILD_ONLY=true; shift ;;
        -accel) ENABLE_ACCEL=true; shift ;;
        -network) ENABLE_NET=true; shift ;;
        -check) RUN_CHECK=true; shift ;;
        -keep-alive) KEEP_ALIVE=true; shift ;;
        -memory) MEMORY="$2"; shift 2 ;;
        -cores) CORES="$2"; shift 2 ;;
        -timeout) TIMEOUT_SEC="$2"; shift 2 ;;
        -qemu-path) QEMU_EXEC="$2"; shift 2 ;;
        -ovmf-path) OVERRIDE_OVMF="$2"; shift 2 ;;
        *) EXTRA_QEMU_ARGS+=("$1"); shift ;;
    esac
done

# --- Main Logic ---

main_logic() {
    # Set Paths based on profile
    local profile="debug"
    [ "$RELEASE_MODE" = true ] && profile="release"

    KERNEL_PATH="$SCRIPT_DIR/target/x86_64-rany_os/$profile/tiny_os"
    DISK_IMAGE="$SCRIPT_DIR/target/x86_64-rany_os/$profile/boot-uefi-tiny_os.img"
    INITRD_PATH="$SCRIPT_DIR/target/initrd.cpio"

    # Pre-flight
    check_prereqs
    OVMF_PATH=$(find_ovmf)
    if [ -z "$OVMF_PATH" ]; then
        log_err "OVMF firmware not found. Please install 'ovmf' package or place OVMF.fd in ./assets/firmware/ovmf-x64/"
        exit 1
    fi

    # Display Config
    if [ "$CLEAN_MODE" = false ] && [ "$MODE_MENU" = false ]; then
        echo -e "${NC}--- Configuration ---"
        echo -e "Profile: $profile"
        echo -e "Hardware: $MEMORY RAM, $CORES Core(s)"
        echo -e "Features: Accel=$( [ "$ENABLE_ACCEL" = true ] && echo "ON" || echo "OFF" ) Net=$( [ "$ENABLE_NET" = true ] && echo "ON" || echo "OFF" ) Check=$( [ "$RUN_CHECK" = true ] && echo "ON" || echo "OFF" )"
        echo -e "OVMF: $OVMF_PATH"
        echo -e "---------------------${NC}\n"
    fi

    if [ "$CLEAN_MODE" = true ]; then
        clean_artifacts
        exit 0
    fi

    if [ "$SKIP_BUILD" = false ]; then
        run_build || { log_err "Build failed."; return 1; }
    fi

    if [ "$BUILD_ONLY" = true ]; then
        log_info "Build complete."
        return 0
    fi

    if [ ! -f "$DISK_IMAGE" ]; then
        log_err "Disk image not found: $DISK_IMAGE"
        return 1
    fi

    start_qemu || { log_err "QEMU exited with error."; return 1; }
}

# --- Entry Point ---

if [ "$MODE_MENU" = true ]; then
    show_menu
else
    main_logic
fi