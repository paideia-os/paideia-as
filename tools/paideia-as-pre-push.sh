#!/bin/bash
# paideia-as pre-push gate — verification recipe before pushing to main.
#
# This script runs the checks required by the v0.20 SELF-HOST milestones.
# Failures exit with status 1 and print diagnostics to stderr.
#
# Usage: tools/paideia-as-pre-push.sh

set -e

echo "=== paideia-as pre-push gate ==="
echo ""

# 1. Check that the example compiles
echo "[1/6] cargo check -p paideia-as-emit --examples"
cargo check -p paideia-as-emit --examples
echo ""

# 2. Build the paideia-satellite-runtime staticlib (nested cargo
#    workspace — PAS-DEBT-B6-002 / #1528). It is not covered by
#    `cargo build --workspace` at the repo root.
echo "[2/6] paideia-satellite-runtime (nested workspace) release build"
bash "$(dirname "$0")/build-satellite-runtime.sh"
echo ""

echo "=== gate passed ==="
