# DDC operational guide

paideia-as uses **Diverse Double Compilation** (DDC) per Wheeler 2005 as its trusting-trust mitigation. This document is the operational guide.

## 1. What DDC verifies

The DDC harness builds the paideia-as binary **twice** under two different host toolchain configurations, then byte-compares the two stage-1 outputs. If they match (modulo a documented allowlist), the build is deterministic enough to defeat the Thompson trusting-trust attack: a malicious stage-0 would have to be present in BOTH toolchains AND produce identical output, which is a much higher bar than a single-stage-0 bootstrap.

For the bootstrap shape (single-stage-0 vs dual-stage-0) see `design/toolchain/bootstrap.md` — the m10-007 decision commits to dual stage-0.

## 2. Running DDC locally

```sh
bash tools/ddc/run.sh
./target/release/ddc-diff tools/ddc/out/a/paideia-as tools/ddc/out/b/paideia-as tools/ddc/allowlist.toml
```

Exit codes:
- 0: match modulo allowlist.
- 1: divergences not covered by allowlist.
- 2: usage / load / IO error.

## 3. Format-gate corpus

`tools/ddc/fixtures/` contains 10 small `.pdx` modules. Each must build to byte-identical output across two successive invocations. The test harness at `tools/ddc/tests/format_gates.rs` exercises this.

Run with `cargo test -p ddc --release -- --ignored` to activate the per-emit tests (PE/COFF / ELF64 / PAX).

## 4. Allowlist policy

`tools/ddc/allowlist.toml` documents every byte-offset range where non-determinism is intentional and accepted. Entries take this form:

```toml
[[rules]]
name = "build-timestamp-elf-note"
start = 0
end = 0
reason = "ELF .note.gnu.build-id can embed a timestamp; m10-003 eliminates this."
```

**Policy**:
1. Every allowlist entry must have a `reason` that names the determinism source.
2. Adding an entry requires a PR review citing why the source can't be eliminated.
3. Allowlist entries are revisited at every release: any entry not still needed should be removed.
4. The allowlist is small by construction. A growing allowlist is a signal that determinism is regressing.

## 5. CI integration

Two workflows compose the operational picture:

- `.github/workflows/ddc.yml` (m10-005): nightly schedule + workflow_dispatch. Advisory — does not gate `main` PRs. Failures land in the workflow summary + as uploaded artifacts with 30-day retention.
- `.github/workflows/release.yml` (m10-006): tag-triggered (`v*`). HARD-FAIL on DDC divergence, blocking the release. An audited bypass via `workflow_dispatch.ddc_bypass_justification` covers emergency cases — the bypass is logged for audit.

Both workflows live behind the org's billing toggle (currently disabled); they activate without further code changes once billing is restored.

## 6. Incident response

When DDC fails on `main` (nightly):

1. **Download** the diff report from the workflow's artifacts.
2. **Inspect** the divergent offsets. If all divergences fall in a known category (timestamps in .note, paths in DWARF, etc.), propose an allowlist update.
3. **If the divergence is unexpected**: this is a real determinism regression. Open a bug, identify the offending PR, revert if necessary.

When DDC fails on a release:

1. The pipeline blocks. **Default response**: investigate the root cause; fix; re-tag.
2. **Emergency bypass**: re-run the workflow via `workflow_dispatch` with a non-empty `ddc_bypass_justification`. The justification is preserved in the audit trail. Use sparingly.

## 7. Env-var contract

See `docs/build-determinism.md` for the canonical contract:
- `SOURCE_DATE_EPOCH` — fixes embedded timestamps.
- `PDX_PATH_PREFIX_MAP="OLD=NEW"` — rewrites build paths.

DDC always runs with `SOURCE_DATE_EPOCH=0` and `PDX_PATH_PREFIX_MAP=/=/build/`.

## 8. References

- `design/toolchain/bootstrap.md` — the dual-stage-0 decision (m10-007).
- `docs/build-determinism.md` — env-var contract (m10-003).
- `tools/ddc/` — orchestrator + differ + allowlist + format-gate corpus.
- `.github/workflows/ddc.yml` + `release.yml` — CI hooks.
- Wheeler 2005, Thompson 1984 — the canonical citations.
