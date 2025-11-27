#!/bin/bash
# run_qemu.sh - Wrapper script for relocated QEMU runner
# This script forwards all arguments to the actual run_qemu.sh in tools/scripts/

"$(dirname "$0")/tools/scripts/run_qemu.sh" "$@"
