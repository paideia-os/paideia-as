#!/bin/bash
# Smoke test driver: builds and runs a .pdx file in QEMU
# Usage: run-smoke.sh <pdx_path> [expected_string]
# Exit codes: 0 = success, 1 = failure, 77 = skip (QEMU not found)

set -e

PDX_PATH="${1:?Usage: run-smoke.sh <pdx_path> [expected_string]}"
EXPECTED="${2:-x}"

# Check if QEMU is available
if ! command -v qemu-system-x86_64 &> /dev/null; then
    exit 77
fi

# Build: compile .pdx to elf64 object
./target/release/paideia-as build --emit elf64 "$PDX_PATH" -o /tmp/smoke.o || exit 1

# Link: link object file with linker script
ld -T tests/build-emit/link.ld /tmp/smoke.o -o /tmp/smoke.elf || exit 1

# Clean any previous QEMU log
rm -f /tmp/qemu_serial.log

# Run in QEMU with timeout
timeout 5 qemu-system-x86_64 \
    -kernel /tmp/smoke.elf \
    -serial file:/tmp/qemu_serial.log \
    -display none \
    -no-reboot \
    -no-shutdown \
    -m 32M \
    >/dev/null 2>&1 || true

# Check for expected output
if grep -q "$EXPECTED" /tmp/qemu_serial.log; then
    exit 0
else
    exit 1
fi
