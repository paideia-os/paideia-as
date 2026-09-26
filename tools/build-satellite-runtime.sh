#!/bin/bash
# Build the `paideia-satellite-runtime` staticlib.
#
# The crate lives in its own nested cargo workspace at
# `crates/paideia-satellite-runtime/Cargo.toml` (PAS-DEBT-B6-002 /
# #1528) so parent-workspace feature unification cannot leak
# `std` into its `#![no_std]` dependency graph. It is therefore
# NOT built by `cargo build --workspace` at the repo root and must
# be built by this script.
#
# Output: `crates/paideia-satellite-runtime/target/release/libpaideia_satellite_runtime.a`
# (satellite host tools — mkfs.pdxfs / mount.pdxfs / umount.pdxfs —
# link against this archive on the final `ld -nostdlib` line).
#
# Usage: `bash tools/build-satellite-runtime.sh` from the repo root.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MANIFEST="${REPO_ROOT}/crates/paideia-satellite-runtime/Cargo.toml"

if [[ ! -f "${MANIFEST}" ]]; then
    echo "error: satellite runtime manifest not found at ${MANIFEST}" >&2
    exit 2
fi

echo "=== building paideia-satellite-runtime (nested workspace) ==="
cargo build --release --manifest-path "${MANIFEST}"

ARCHIVE="${REPO_ROOT}/crates/paideia-satellite-runtime/target/release/libpaideia_satellite_runtime.a"
if [[ ! -f "${ARCHIVE}" ]]; then
    echo "error: expected staticlib not produced at ${ARCHIVE}" >&2
    exit 3
fi

echo "=== built: ${ARCHIVE} ==="
