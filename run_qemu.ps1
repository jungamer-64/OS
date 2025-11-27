# run_qemu.ps1 - Wrapper script for relocated QEMU runner
# This script forwards all arguments to the actual run_qemu.ps1 in tools/scripts/

& "$PSScriptRoot\tools\scripts\run_qemu.ps1" @args
