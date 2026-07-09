#!/bin/bash
# verify-byte-order.sh: Regression probe for SysV function call byte ordering (issue #1099).
#
# Builds a minimal 2-arg SysV fixture with `paideia-as build`, then uses objdump
# to verify the physical byte layout of the `caller` function is:
#
#     movabs $imm, %rdi      # first arg
#     movabs $imm, %rsi      # second arg
#     call   target
#     ret                    # final byte
#
# and NOT the pre-#1099 broken order (`call; ret; mov; mov` with MOVs as dead
# code past RET).
#
# Usage: ./verify-byte-order.sh [--verbose]
# Exit: 0 if byte order is correct; 1 if broken, build fails, or objdump missing.

set -euo pipefail

VERBOSE="${1:-}"

WORKDIR=$(mktemp -d)
trap "rm -rf $WORKDIR" EXIT

PAIDEIA_AS_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

command -v objdump >/dev/null 2>&1 || {
    echo "[verify-byte-order] FAILED: objdump not found in PATH"
    exit 1
}

cat > "$WORKDIR/byte_order_probe.pdx" << 'EOF'
module ByteOrderProbe = structure {
  pub let add_two : (u64, u64) -> u64 = fn(x: u64, y: u64) -> x + y
  pub let caller  : (u64) -> u64      = fn(_z: u64) -> add_two(3, 4)
}
EOF

cd "$PAIDEIA_AS_ROOT"

echo "[verify-byte-order] Building paideia-as..."
if ! cargo build --release -p paideia-as --quiet 2>&1; then
    echo "[verify-byte-order] FAILED: cargo build failed"
    exit 1
fi

echo "[verify-byte-order] Building fixture to ELF..."
if ! cargo run --release -q -p paideia-as -- build "$WORKDIR/byte_order_probe.pdx" \
        -o "$WORKDIR/fixture.o" --emit elf64 2>"$WORKDIR/build.stderr"; then
    echo "[verify-byte-order] FAILED: paideia-as build failed:"
    cat "$WORKDIR/build.stderr"
    exit 1
fi

if [ ! -s "$WORKDIR/fixture.o" ]; then
    echo "[verify-byte-order] FAILED: fixture.o not produced or empty"
    exit 1
fi

echo "[verify-byte-order] Disassembling caller function..."
DUMP=$(objdump -d "$WORKDIR/fixture.o")
if [ -n "$VERBOSE" ]; then
    echo "--- objdump -d output ---"
    echo "$DUMP"
    echo "--- end objdump ---"
fi

CALLER_BLOCK=$(echo "$DUMP" | awk '/<caller>:/{f=1; next} f && /^[[:space:]]*$/{exit} f{print}')
if [ -z "$CALLER_BLOCK" ]; then
    echo "[verify-byte-order] FAILED: caller symbol not found in .text"
    echo "$DUMP"
    exit 1
fi

RDI_LINE=$(echo "$CALLER_BLOCK" | grep -nE 'movabs.*%rdi' | head -1 | cut -d: -f1 || true)
RSI_LINE=$(echo "$CALLER_BLOCK" | grep -nE 'movabs.*%rsi' | head -1 | cut -d: -f1 || true)
CALL_LINE=$(echo "$CALLER_BLOCK" | grep -nE $'\tcall' | head -1 | cut -d: -f1 || true)
RET_LINE=$(echo "$CALLER_BLOCK" | grep -nE $'\tret([^[:alnum:]]|$)' | head -1 | cut -d: -f1 || true)

if [ -z "$RDI_LINE" ] || [ -z "$RSI_LINE" ] || [ -z "$CALL_LINE" ] || [ -z "$RET_LINE" ]; then
    echo "[verify-byte-order] FAILED: expected instructions missing from caller block:"
    echo "  rdi MOV line: '${RDI_LINE:-MISSING}'"
    echo "  rsi MOV line: '${RSI_LINE:-MISSING}'"
    echo "  call line: '${CALL_LINE:-MISSING}'"
    echo "  ret line: '${RET_LINE:-MISSING}'"
    echo "$CALLER_BLOCK"
    exit 1
fi

if [ "$RDI_LINE" -ge "$CALL_LINE" ] || [ "$RSI_LINE" -ge "$CALL_LINE" ]; then
    echo "[verify-byte-order] FAILED: BROKEN BYTE ORDER — arg MOVs do not precede CALL"
    echo "  This is issue #1099 reappearing."
    echo "$CALLER_BLOCK"
    exit 1
fi

if [ "$CALL_LINE" -ge "$RET_LINE" ]; then
    echo "[verify-byte-order] FAILED: CALL does not precede RET"
    echo "$CALLER_BLOCK"
    exit 1
fi

TAIL_AFTER_RET=$(echo "$CALLER_BLOCK" | awk 'found{if(NF)print} /ret/{found=1}')
if [ -n "$TAIL_AFTER_RET" ]; then
    echo "[verify-byte-order] FAILED: unreachable instructions after RET:"
    echo "$TAIL_AFTER_RET"
    exit 1
fi

echo "[verify-byte-order] PASS — byte order is correct:"
echo "  mov %rdi before call: line $RDI_LINE < $CALL_LINE"
echo "  mov %rsi before call: line $RSI_LINE < $CALL_LINE"
echo "  ret is final instruction (line $RET_LINE)"
exit 0
