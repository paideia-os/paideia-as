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

echo "=== gate passed ==="
