#!/bin/bash
# Cross-build orchestration: build the same module via NASM and paideia-as,
# extract mnemonics, and diff against expected output.
#
# Takes a fixture directory as $1 (e.g., tools/cross-build/fixtures/add_one/).
# The fixture must contain:
#   - module.asm: NASM source
#   - module.pdx: paideia-as source
#   - module.expect-mnemonics.txt: expected instruction sequence (one per line)
#
# Returns 0 if both build paths produce the expected output; nonzero on failure.
#
# Usage:
#   cross-build.sh fixtures/add_one/

set -euo pipefail

# Script directory (where this script lives)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TOOLS_DIR="${SCRIPT_DIR}"
ROOT_DIR="$(cd "${TOOLS_DIR}/../.." && pwd)"

if [[ $# -ne 1 ]]; then
    echo "Usage: $(basename "$0") <fixture-dir>" >&2
    exit 1
fi

FIXTURE_DIR="$1"

# Resolve to absolute path
if [[ "$FIXTURE_DIR" != /* ]]; then
    # If relative, first try from current working directory, then from ROOT_DIR
    if [[ -d "$FIXTURE_DIR" ]]; then
        FIXTURE_DIR="$(cd "$FIXTURE_DIR" && pwd)"
    else
        FIXTURE_DIR="$(cd "${ROOT_DIR}/${FIXTURE_DIR}" && pwd)"
    fi
fi

if [[ ! -d "$FIXTURE_DIR" ]]; then
    echo "Error: fixture directory $FIXTURE_DIR not found" >&2
    exit 1
fi

FIXTURE_NAME="$(basename "$FIXTURE_DIR")"

# Required files
ASM_SRC="${FIXTURE_DIR}/module.asm"
PDX_SRC="${FIXTURE_DIR}/module.pdx"
EXPECT_FILE="${FIXTURE_DIR}/module.expect-mnemonics.txt"

# Check required files exist
for file in "$ASM_SRC" "$PDX_SRC" "$EXPECT_FILE"; do
    if [[ ! -f "$file" ]]; then
        echo "Error: $file not found" >&2
        exit 1
    fi
done

# Create temporary build directory
TEMP_DIR=$(mktemp -d)
trap "rm -rf $TEMP_DIR" EXIT

echo "[cross-build] Fixture: $FIXTURE_NAME"
echo "[cross-build] Working in: $TEMP_DIR"

# ──────────────────────────────────────────────────────────────────────
# Step 1: Build via NASM
# ──────────────────────────────────────────────────────────────────────

NASM_OBJ="${TEMP_DIR}/module-nasm.o"

echo "[cross-build] Assembling $ASM_SRC via NASM..."
if ! nasm -f elf64 "$ASM_SRC" -o "$NASM_OBJ"; then
    echo "Error: NASM assembly failed" >&2
    exit 1
fi

# Extract NASM mnemonics
NASM_MNEMONICS="${TEMP_DIR}/nasm-mnemonics.txt"
"${TOOLS_DIR}/extract-mnemonics.sh" "$NASM_OBJ" > "$NASM_MNEMONICS"

# ──────────────────────────────────────────────────────────────────────
# Step 2: Build via paideia-as
# ──────────────────────────────────────────────────────────────────────

PDX_OBJ="${TEMP_DIR}/module-paideia-as.o"

echo "[cross-build] Building $PDX_SRC via paideia-as..."
if ! cargo run -p paideia-as --quiet -- build --emit elf64 "$PDX_SRC" -o "$PDX_OBJ" >/dev/null 2>&1; then
    echo "Error: paideia-as build failed" >&2
    exit 1
fi

# Extract paideia-as mnemonics
PDX_MNEMONICS="${TEMP_DIR}/paideia-as-mnemonics.txt"
"${TOOLS_DIR}/extract-mnemonics.sh" "$PDX_OBJ" > "$PDX_MNEMONICS"

# ──────────────────────────────────────────────────────────────────────
# Step 3: Compare against expected
# ──────────────────────────────────────────────────────────────────────

echo "[cross-build] Comparing against expected..."

# Count mismatches
NASM_DIFF_EXIT=0
PDX_DIFF_EXIT=0

diff -u "$EXPECT_FILE" "$NASM_MNEMONICS" >/dev/null 2>&1 || NASM_DIFF_EXIT=$?
diff -u "$EXPECT_FILE" "$PDX_MNEMONICS" >/dev/null 2>&1 || PDX_DIFF_EXIT=$?

# ──────────────────────────────────────────────────────────────────────
# Report
# ──────────────────────────────────────────────────────────────────────

if [[ $NASM_DIFF_EXIT -eq 0 ]] && [[ $PDX_DIFF_EXIT -eq 0 ]]; then
    echo "[cross-build] ✓ PASS: $FIXTURE_NAME"
    echo "[cross-build]   NASM mnemonics match expected"
    echo "[cross-build]   paideia-as mnemonics match expected"
    exit 0
else
    echo "[cross-build] ✗ FAIL: $FIXTURE_NAME"
    echo ""
    echo "═══════════════════════════════════════════════════════════════"
    echo "Expected mnemonics:"
    echo "═══════════════════════════════════════════════════════════════"
    cat "$EXPECT_FILE"
    echo ""

    if [[ $NASM_DIFF_EXIT -ne 0 ]]; then
        echo "═══════════════════════════════════════════════════════════════"
        echo "NASM assembly divergence (diff -u):"
        echo "═══════════════════════════════════════════════════════════════"
        diff -u "$EXPECT_FILE" "$NASM_MNEMONICS" || true
        echo ""
    fi

    if [[ $PDX_DIFF_EXIT -ne 0 ]]; then
        echo "═══════════════════════════════════════════════════════════════"
        echo "paideia-as build divergence (diff -u):"
        echo "═══════════════════════════════════════════════════════════════"
        diff -u "$EXPECT_FILE" "$PDX_MNEMONICS" || true
        echo ""
    fi

    exit 1
fi
