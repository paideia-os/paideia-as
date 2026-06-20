# ddc: Diverse Double Compilation

## Overview

DDC (Diverse Double Compilation) is a build verification harness that strengthens confidence in paideia-as determinism and resists trusting-trust attacks. It builds paideia-as twice using two different host toolchains and compares the resulting binaries for byte-for-byte identity.

## How It Works

The orchestrator script (`run.sh`) executes two release builds:

- **Toolchain A**: Default cargo/rustc (stable)
- **Toolchain B**: Nightly cargo/rustc (if available); falls back to a second stable build in phase-2-m10-001

Both stage-1 artifacts are saved to `tools/ddc/out/{a,b}/` for comparison and audit.

## Usage

```bash
tools/ddc/run.sh
```

**Exit codes:**
- `0` — both builds succeeded.
- `1` — a build failed.
- `2` — toolchain not available (phase-2-m10-005 activates proper handling).

## Phase-2-m10-001 Scope

This issue delivers the core shell orchestrator with:

- Dual-build orchestration (stable vs. nightly/fallback).
- Version logging for both toolchains.
- Artifact collection to `tools/ddc/out/{a,b}/`.
- Rust helpers placeholder (`src/lib.rs`).

## Future Enhancements (m10-002 through m10-006)

- **m10-002**: Byte-level differ — compare stage-1 artifacts and report divergences.
- **m10-003**: Attestation + signature verification — cryptographic binding.
- **m10-004**: Real toolchain diversity — GCC-built rustc, distro rustc, etc.
- **m10-005**: CI integration — wire into GitHub Actions workflow.
- **m10-006**: Reporting dashboard — track divergence rates over time.

## Notes

- `tools/ddc/out/` is a runtime artifact directory and is gitignored.
- The script is designed to be safe for CI environments and workstations alike.
- Real diverse-toolchain configurations activate in m10-005 when CI orchestration is complete.
