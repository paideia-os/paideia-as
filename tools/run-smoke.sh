#!/bin/bash
# Smoke test driver: builds and runs a .pdx file in QEMU
# Usage: run-smoke.sh <pdx_path> [expected_string]
# Exit codes: 0 = success, 1 = failure, 77 = skip (QEMU not found)
#
# #1267: on failure, prints the QEMU command and the last serial-log
# lines so the user can reproduce interactively without hunting.

set -e

PDX_PATH="${1:?Usage: run-smoke.sh <pdx_path> [expected_string]}"
EXPECTED="${2:-x}"
QEMU_LOG="/tmp/qemu_serial.log"
ELF="/tmp/smoke.elf"

# Check if QEMU is available
if ! command -v qemu-system-x86_64 &> /dev/null; then
    exit 77
fi

# Build: compile .pdx to elf64 object
./target/release/paideia-as build --emit elf64 "$PDX_PATH" -o /tmp/smoke.o || exit 1

# Link: link object file with linker script
ld -T tests/build-emit/link.ld /tmp/smoke.o -o "$ELF" || exit 1

# Clean any previous QEMU log
rm -f "$QEMU_LOG"

# QEMU command (kept in one variable so failure path can echo it verbatim)
QEMU_CMD=(timeout 5 qemu-system-x86_64
    -kernel "$ELF"
    -serial "file:$QEMU_LOG"
    -display none
    -no-reboot
    -no-shutdown
    -m 32M
)

# Run in QEMU
"${QEMU_CMD[@]}" >/dev/null 2>&1 || true

# Check for expected output
if grep -q "$EXPECTED" "$QEMU_LOG"; then
    exit 0
fi

# #1267: failure branch — dump reproducer so the human isn't stuck guessing.
{
    echo "smoke: expected marker '$EXPECTED' not found in $QEMU_LOG"
    echo ""
    echo "reproducer:"
    printf '  '
    printf '%q ' "${QEMU_CMD[@]}"
    echo
    echo ""
    if [[ -s "$QEMU_LOG" ]]; then
        echo "last 40 lines of $QEMU_LOG:"
        tail -40 "$QEMU_LOG" | sed 's/^/  /'
    else
        echo "$QEMU_LOG is empty (QEMU produced no serial output — likely early boot crash)"
    fi
} >&2
exit 1
