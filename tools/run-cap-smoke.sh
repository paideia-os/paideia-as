#!/usr/bin/env bash
# Phase 6 m6-002: cap_smoke userspace ELF builder + runner.
#
# Builds tests/build-emit/cap_smoke.pdx into an executable, links via
# cap_smoke.link.ld, runs under timeout 5, asserts exit code.
#
# Usage: tools/run-cap-smoke.sh [expected_exit_code]
#   expected_exit_code defaults to 1 (the cap_verify happy-path return).
#
# Exit codes:
#   0   — built + linked + ran + got expected exit code
#   1   — ran but unexpected exit code
#   2   — build or link failed
#  77   — Linux-only; skipped on other OS

set -uo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
EXPECTED="${1:-1}"

case "$(uname -s)" in
    Linux) ;;
    *) echo "cap_smoke: Linux-only; skipping on $(uname -s)" >&2; exit 77 ;;
esac

PAIDEIA_AS="${REPO_ROOT}/target/release/paideia-as"
if [[ ! -f "${PAIDEIA_AS}" ]]; then
    echo "cap_smoke: paideia-as not built; cargo build --release -p paideia-as first" >&2
    exit 2
fi

PDX="${REPO_ROOT}/tests/build-emit/cap_smoke.pdx"
LD_SCRIPT="${REPO_ROOT}/tests/build-emit/cap_smoke.link.ld"
OBJ="/tmp/cap_smoke.o"
ELF="/tmp/cap_smoke.elf"

rm -f "${OBJ}" "${ELF}"

if ! "${PAIDEIA_AS}" build --emit elf64 "${PDX}" -o "${OBJ}" 2>&1; then
    echo "cap_smoke: paideia-as build failed" >&2
    exit 2
fi

if ! ld -static -T "${LD_SCRIPT}" "${OBJ}" -o "${ELF}" 2>&1; then
    echo "cap_smoke: ld link failed" >&2
    exit 2
fi

timeout 5 "${ELF}"
RC=$?

if [[ "${RC}" -eq "${EXPECTED}" ]]; then
    echo "cap_smoke: exit ${RC} matches expected ${EXPECTED}"
    exit 0
else
    echo "cap_smoke: exit ${RC} does NOT match expected ${EXPECTED}" >&2
    exit 1
fi
