#!/usr/bin/env bash
#
# test_bootloader.sh — smoke-test the asm-reference bootloader on QEMU.
#
# Builds boot.asm with NASM, boots it under qemu-system-x86_64 with the
# isa-debug-exit device wired so the bootloader can terminate the VM
# cleanly, captures serial output to a temp file, and asserts the
# expected banner appears.
#
# Exit codes:
#   0 — banner found, QEMU exited cleanly.
#   1 — banner missing or QEMU exited unexpectedly.
#   2 — toolchain missing (nasm / qemu-system-x86_64).
#
# Requirements: nasm, qemu-system-x86_64, GNU coreutils (timeout, stat).

set -euo pipefail

readonly EXPECTED="Hello, paideia-os boot!"
readonly TIMEOUT_SECS="${TIMEOUT_SECS:-10}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly ASM_REF_DIR="$(dirname "$SCRIPT_DIR")"
readonly BOOT_ASM="$ASM_REF_DIR/bootloader/boot.asm"

# ── Tooling guard ─────────────────────────────────────────────────────
for tool in nasm qemu-system-x86_64; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        printf 'error: required tool not on PATH: %s\n' "$tool" >&2
        exit 2
    fi
done

# ── Temp file setup ───────────────────────────────────────────────────
BOOT_BIN="$(mktemp --suffix=.bin)"
LOG_FILE="$(mktemp --suffix=.log)"
cleanup() { rm -f "$BOOT_BIN" "$LOG_FILE"; }
trap cleanup EXIT

# ── Build ─────────────────────────────────────────────────────────────
printf 'assembling %s\n' "$BOOT_ASM"
nasm -f bin "$BOOT_ASM" -o "$BOOT_BIN"

size=$(stat -c %s "$BOOT_BIN")
if [ "$size" -ne 512 ]; then
    printf 'error: boot.bin is %s bytes, expected exactly 512 (MBR size)\n' "$size" >&2
    exit 1
fi
printf '  -> %s (%s bytes)\n' "$BOOT_BIN" "$size"

# ── Run on QEMU ───────────────────────────────────────────────────────
printf 'booting under qemu-system-x86_64 (timeout %ss)\n' "$TIMEOUT_SECS"
qemu_exit=0
timeout "$TIMEOUT_SECS" qemu-system-x86_64 \
    -drive format=raw,file="$BOOT_BIN",if=floppy \
    -display none \
    -serial file:"$LOG_FILE" \
    -device isa-debug-exit,iobase=0xf4,iosize=0x04 \
    -no-reboot \
    -monitor none \
    >/dev/null 2>&1 || qemu_exit=$?

# isa-debug-exit produces (N << 1) | 1 — writing 0 yields exit 1.
# Map QEMU exit codes back into pass/fail categories.
case "$qemu_exit" in
    1)
        printf '  qemu exited cleanly via isa-debug-exit\n'
        ;;
    124)
        printf 'error: qemu timed out after %ss\n' "$TIMEOUT_SECS" >&2
        printf -- '--- captured serial output ---\n' >&2
        cat "$LOG_FILE" >&2 || true
        printf -- '--- end ---\n' >&2
        exit 1
        ;;
    *)
        printf 'error: qemu exited with unexpected status %s\n' "$qemu_exit" >&2
        exit 1
        ;;
esac

# ── Assert ────────────────────────────────────────────────────────────
printf 'checking serial output for %q\n' "$EXPECTED"
if grep -q -- "$EXPECTED" "$LOG_FILE"; then
    printf 'PASS\n'
    exit 0
fi

printf 'FAIL: expected banner not found on serial console\n' >&2
printf -- '--- captured serial output ---\n' >&2
cat "$LOG_FILE" >&2 || true
printf -- '--- end ---\n' >&2
exit 1
