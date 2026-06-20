# ddc: Diverse Double Compilation

## Overview

DDC (Diverse Double Compilation) is a build verification harness that strengthens confidence in paideia-as determinism and resists trusting-trust attacks. It builds paideia-as twice using two different host toolchains and compares the resulting binaries for byte-for-byte identity.

## How It Works

### Phase-2-m10-001: Orchestration

The orchestrator script (`run.sh`) executes two release builds:

- **Toolchain A**: Default cargo/rustc (stable)
- **Toolchain B**: Nightly cargo/rustc (if available); falls back to a second stable build in phase-2-m10-001

Both stage-1 artifacts are saved to `tools/ddc/out/{a,b}/` for comparison and audit.

**Exit codes:**
- `0` — both builds succeeded.
- `1` — a build failed.
- `2` — toolchain not available (phase-2-m10-005 activates proper handling).

### Phase-2-m10-002: Byte-Level Differ

This phase introduces:

- **ddc-diff CLI**: Compare two binaries byte-by-byte and produce a structured JSON report.
- **Allowlist (.toml)**: Document known sources of non-determinism (e.g., timestamps) so they don't fail the comparison.
- **Rust library**: Core differ logic for reuse in test harnesses and CI pipelines.

#### ddc-diff Usage

```bash
ddc-diff <path-a> <path-b> <allowlist-toml>
```

**Output**: Structured JSON report to stdout containing:
- Byte offsets of all divergences
- Values at each divergence (byte_a vs byte_b)
- Allowlist coverage (which rule, if any, applies)
- Summary: allowlisted count, unallowlisted count, match status

**Exit codes:**
- `0` — binaries match modulo allowlist (all divergences are allowlisted).
- `1` — divergences found that are NOT allowlisted (determinism issue).
- `2` — error (missing file, bad allowlist TOML, I/O error).

#### Allowlist Format

`allowlist.toml` uses TOML format:

```toml
[[rules]]
name = "rule-name"
start = 0x1000      # byte offset (decimal or hex)
end = 0x1fff        # byte offset (inclusive)
reason = "Why this range can differ (e.g., build timestamp)"
```

Each rule documents a known non-deterministic range. The differ reports divergences in these ranges but marks them as allowlisted, so they don't cause exit code 1.

#### Library API

```rust
use ddc::allowlist::Allowlist;
use ddc::diff_files;

let allowlist = Allowlist::load("allowlist.toml")?;
let report = diff_files(Path::new("a.bin"), Path::new("b.bin"), &allowlist)?;

println!("Unallowlisted divergences: {}", report.unallowlisted_count);
if report.match_modulo_allowlist {
    println!("Builds are deterministic!");
}
```

## Phase-2-m10-001 Scope

- Dual-build orchestration (stable vs. nightly/fallback).
- Version logging for both toolchains.
- Artifact collection to `tools/ddc/out/{a,b}/`.
- Rust helpers placeholder (`src/lib.rs`).

## Phase-2-m10-002 Scope

- Byte-level differ library + CLI (`ddc-diff`).
- Allowlist parser (.toml format).
- Documented non-determinism rules (sentinel at m10-002; real ranges from m10-003).
- Comprehensive tests (lib + CLI).

## Future Enhancements

- **m10-003**: Determinism fixes + expanded allowlist.
- **m10-004**: Attestation + signature verification — cryptographic binding.
- **m10-005**: Real toolchain diversity — GCC-built rustc, distro rustc, etc.
- **m10-006**: CI integration — wire into GitHub Actions workflow.
- **m10-007**: Reporting dashboard — track divergence rates over time.

## Notes

- `tools/ddc/out/` is a runtime artifact directory and is gitignored.
- The script is designed to be safe for CI environments and workstations alike.
- Real diverse-toolchain configurations activate in m10-005 when CI orchestration is complete.
- The allowlist design is "positive" (allow known divergences) so that unknown divergences fail fast.
