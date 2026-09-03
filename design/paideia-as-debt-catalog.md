# paideia-as debt catalog

**Status:** Design (2026-09-03). Categorisation + prioritisation pass; **no
issue-filing this batch.**
**Umbrella issue:** [paideia-as#1396](https://github.com/paideia-os/paideia-as/issues/1396)
**Batch:** Wave 0 Batch 4 (L-sized techdoc)
**Source of assignment:** MASTER_PLAN.md Phase A + manifest v2 (Q-A-3
resolution per challenger §5.6).

---

## 0. Preamble

### 0.1 Purpose

`paideia-as` has accumulated three qualitatively different bodies of debt
between v0.20 and v0.32: (a) a stable, non-growing set of pre-existing
`build_emit` failures observed at every v0.22 / v0.23 / v0.24 landing;
(b) parser gaps repeatedly surfaced by driver and substrate work; and
(c) `TODO` / `unimplemented!()` residues in the intrinsic tables, the
IR optimiser, and the `stdlib_lowering` recipes. The umbrella
[paideia-as#1396](https://github.com/paideia-os/paideia-as/issues/1396)
names three sub-groups explicitly; this catalog additionally surfaces
four adjacent categories — `stdlib_lowering` placeholders, IR-opt stubs,
the test-runner vaporware entry (paideia-as#1349), and the satellite
runtime-shim gap (paideia-as#1348) — so that per-item sub-issues can be
filed once, coherently, in Phase 2.

### 0.2 Scope of this document

- **In scope:** every actionable item currently visible in the tree
  under `crates/paideia-as*/**` and `tests/**`, categorised into 7
  buckets, sized, and assigned a landing wave.
- **Out of scope:** the actual filing of per-item sub-issues. Sub-issues
  are drafted in `.plans/scratch/file-pas-debt-issues.sh` (this batch)
  but **not filed**; the parent agent runs the filing step in Phase 2.
- **Also out of scope:** planned intrinsics that are already tracked
  under their own release-track issue (`#1361` u128 arith, `#1381` f16,
  `#1379` SPIR-V embed, `#1380` WGSL embed, `#1346` ECDSA-P256). These
  are forward work, not debt; the catalog cross-references them but
  does not restate their scope.

### 0.3 Sizing convention

| Size | Wall-clock estimate | Typical shape                                              |
|------|--------------------|------------------------------------------------------------|
| S    | ≤ 1 day             | Mechanical fill-in: one table row, one recipe, one guard.  |
| M    | 1–3 days            | Parser production, elaborator walker gap, encoder arm.     |
| L    | ≥ 3 days            | Cross-crate design pass, sret marshalling, IR-opt rewrite. |

Bucket-level sizes are the sum of their entries, floored at the
weakest link (a bucket containing one L-item is at least L).

### 0.4 Filing plan (Phase 2)

Per umbrella policy the sub-issues this catalog produces **do not**
appear in `.plans/next-wave-issues.tsv`. They are drafted below (with
IDs `PAS-DEBT-B<n>-<mmm>`) and filed by the parent agent via the
draft script at `.plans/scratch/file-pas-debt-issues.sh` — which
mirrors the shape of the existing `.plans/scratch/file-issues.sh`
but never runs in this batch (`chmod +x` deliberately withheld).

---

## 1. Bucket summary

| # | Bucket                                               | Items | Size | Next-wave landing target                                    |
|---|------------------------------------------------------|-------|------|-------------------------------------------------------------|
| 1 | `build_emit` pre-existing failures (v0.22 baseline)  |  ≥12  | L    | Wave 1 encoder + elaborator (spread across v0.22.x .. v0.25.x) |
| 2 | Parser gaps surfaced by driver / substrate work      |   4   | M    | Wave 2 parser refactor (parallel to `#1360` v0.26 helpers)  |
| 3 | Intrinsic-table TODOs + IR-opt stubs                 |   9   | L    | Wave 2 IR-opt round + Wave 3 intrinsic completion           |
| 4 | `stdlib_lowering` placeholders (hash / sret / imm64) |   4   | M    | Wave 1 encoder round (`#1392` BLAKE3) + Wave 2 sret design  |
| 5 | Test-runner vaporware (`paideia-as test` is a no-op) |   1   | M    | Wave 1 delivery via `#1393` (v0.33-M1-007)                  |
| 6 | Satellite runtime shim (`crypto_shim.rs`)            |   1   | S    | Wave 1 delivery via `#1391` (v0.33-M1-005)                  |
| 7 | Ignored corpus tests (test-discipline hygiene)       |   7   | S    | Wave 3 corpora reactivation (post Waves 1 + 2)              |

**Aggregate:** 38 individually filable items across 7 buckets, weighted
2 L + 2 M + 3 S when collapsed to bucket granularity. The Group-1
count is a lower bound: the v0.22.0 CHANGELOG confirmed "426 passed
/ 12 failed (12 pre-existing, identical set confirmed via `git stash`
against `39b3e93`)" (`CHANGELOG.md:264`), but only three of the twelve
are surfaced as `#[ignore]` in tree; the remaining nine require a
fresh `cargo test -p paideia-as --test build_emit` run to enumerate.
See §2.2 for the observed gap and the recommended enumeration
procedure.

Formally, if `F` is the set of pre-existing `build_emit` failures
(|F| = 12 per the v0.22.0 baseline) and `I ⊂ F` the subset marked
`#[ignore]` in-tree, then `|I| = 3` and `|F \ I| = 9` failures remain
un-marked in source. The catalog enumerates `I` concretely; the nine
in `F \ I` are named in §2 by shape only until the enumeration run
completes.

---

## 2. Bucket 1 — `build_emit` pre-existing failures

### 2.1 Rationale

The `paideia-as` `build_emit` integration test binary
(`crates/paideia-as/tests/build_emit/`, 151 files as of v0.32.0) is
the primary byte-exact end-to-end regression net for the elaborator +
encoder path. The v0.22.0 landing recorded a stable "12 pre-existing"
failure count against commit `39b3e93` and confirmed the set was
identical to the pre-#1326 baseline. That count has held across
v0.23.0 (`mldsa65_sign`) and v0.24.0 (scalar-float SSE encoder) — the
CHANGELOG for both versions notes the same 12-count. No landing has
addressed this baseline.

**Fix-category taxonomy** (per umbrella):

| Category            | Signature                                                        |
|---------------------|------------------------------------------------------------------|
| `encoder-table-gap` | A mnemonic is defined but a `(shape, opcode)` row is missing.    |
| `lowering-rule`     | The elaborator recipe for an AST → IR shape is absent or wrong.  |
| `walker-gap`        | A walker (unsafe / effect / cap / linearity) skips a construct.  |
| `size-threading`    | Byte-width plumbing (`W8/W16/W32/W64`) drops through a call.     |

### 2.2 Observed gap between in-tree evidence and the umbrella count

The umbrella cites "12 known failures". A grep over `tests/build_emit/`
finds **three** `#[ignore]`'d tests and one `#[ignore = "..."]` in
`bridge_thunk.rs`:

| Entry id            | Site                                                                                                       | Symptom                                                                                                                   | Fix category        | Size | Landing wave |
|---------------------|------------------------------------------------------------------------------------------------------------|---------------------------------------------------------------------------------------------------------------------------|---------------------|------|--------------|
| `PAS-DEBT-B1-001`   | `/home/snunez/Development/PaideiaOS/tools/paideia-as/crates/paideia-as/tests/build_emit/field_read.rs:39`  | `field_access_cap_set_rights_deferred_pending_parser_support` — `struct` type-definition syntax not accepted by parser.   | `walker-gap` (via B2 parser gap)   | M    | Wave 2 parser + Wave 1 walker      |
| `PAS-DEBT-B1-002`   | `crates/paideia-as/tests/build_emit/pa10_007_data_symbol_names.rs:89`                                      | Requires `readelf` + `ld` in `$PATH`; integration test hoisted out of unit-test lane rather than a real failure.          | (infra)             | S    | Wave 3 CI wiring                    |
| `PAS-DEBT-B1-003`   | `crates/paideia-as/tests/build_emit/pa10_007_data_symbol_names.rs:122`                                     | Same as B1-002; separate `#[test]` guarded on `readelf`.                                                                  | (infra)             | S    | Wave 3 CI wiring                    |
| `PAS-DEBT-B1-004`   | `crates/paideia-as/tests/build_emit/bridge_thunk.rs:314`                                                   | `U1620` narrowing: MS x64 lambda bodies containing function calls not in current MVP set (identity / add-imm / literal).  | `walker-gap`        | M    | Wave 2 elaborator MS-x64 widening   |

The remaining **nine failures cited by the v0.22.0 CHANGELOG are not
marked `#[ignore]` in source** — they pass compile but fail at
runtime. Their identities cannot be recovered from grep alone; a
fresh test run (`cargo test -p paideia-as --test build_emit 2>&1 |
grep -E "^test .* FAILED"`) is the fastest enumeration.

For each of the nine unmarked-runtime failures a `PAS-DEBT-B1-005`
… `PAS-DEBT-B1-013` id is reserved; the parent's Phase-2 filing pass
runs the enumeration then flushes those ids to sub-issues. The draft
script at `.plans/scratch/file-pas-debt-issues.sh` reads the
enumeration output as its input file.

### 2.3 Reserved id block

```
PAS-DEBT-B1-001 .. B1-004  — enumerated above (3 #[ignore] + 1 #[ignore="..."])
PAS-DEBT-B1-005 .. B1-013  — reserved for the nine un-marked runtime failures
                             (enumerated by cargo test -p paideia-as --test build_emit)
```

Total 13 reserved ids; the umbrella's "12 known" is the count of
runtime failures — the four `#[ignore]` items are a proper subset of
the same failure surface (the ignored ones no longer report FAILED
because they are skipped, but they still describe the debt).

### 2.4 Sizing rationale

Enumerating the nine unmarked failures is fast (single `cargo test`
run). Triaging each into a fix category and drafting a minimal
reproducer is the L component: the failures include at least one
elaborator pattern-lowering regression, one encoder byte-form gap,
and one walker gap per historical CHANGELOG mentions. Estimated total
across the bucket: ~9 M-sized fixes + ~3 S-sized (`#[ignore]` clears)
= ≈ 20 engineer-days; hence L at the bucket level.

---

## 3. Bucket 2 — Parser gaps

### 3.1 Rationale

Each entry is a construct the parser rejects (or silently drops) that a
downstream consumer of `paideia-as` (`paideia-os` driver code,
`paideia-stdlib` recipes, or a `build_emit` fixture) needs. The fix
category taxonomy:

| Category         | Signature                                                              |
|------------------|------------------------------------------------------------------------|
| `production-add` | Add a new grammar production for a shape today's parser skips.        |
| `terminal-add`   | Extend the lexer with a new terminal (keyword, punctuation).           |
| `recovery-rule`  | Change error-recovery behaviour so a downstream pass can still walk.  |

### 3.2 Entries

| Entry id          | Site                                                                                        | Missing shape                                                                                                    | Fix category      | Size | Landing wave |
|-------------------|---------------------------------------------------------------------------------------------|------------------------------------------------------------------------------------------------------------------|-------------------|------|--------------|
| `PAS-DEBT-B2-001` | `crates/paideia-as-parser/src/parse_control.rs:162`                                         | `for pat in iter { body }` — pattern (`pat`) is parsed then dropped; only the header is stored.                  | `production-add`  | S    | Wave 2       |
| `PAS-DEBT-B2-002` | `crates/paideia-as-parser/src/parse_item/generics.rs:104`                                   | Associated-type projections (`T::AssocTy`) are lexed but not validated against the trait's associated-type set.   | `production-add`  | M    | Wave 2       |
| `PAS-DEBT-B2-003` | `crates/paideia-as-parser/src/parse_item/trait_impl.rs:588`                                 | `trait_args = Vec::new();` — trait-impl argument extraction stubbed with an unconditional empty vector.           | `production-add`  | M    | Wave 2       |
| `PAS-DEBT-B2-004` | `crates/paideia-as/tests/build_emit/field_read.rs:37-42` (parser side of B1-001)            | `struct` type-definition syntax needed by `cap_set_rights.pdx` fixture.                                          | `production-add`  | M    | Wave 2       |

### 3.3 Note on shape

Every entry is `production-add`; the parser today does not use
`recovery-rule`-shaped defects (Pratt-style precedence recovery works
correctly), and the lexer terminal set is complete for every `.pdx`
fixture on disk. The bucket may grow if paideia-os R52+ driver work
surfaces new shapes; the id space `PAS-DEBT-B2-005` and up is
reserved.

---

## 4. Bucket 3 — Intrinsic-table TODOs and IR-opt stubs

### 4.1 Rationale

Two adjacent bodies of debt: (a) intrinsic descriptor / recipe TODOs
that block runtime functionality, and (b) IR-optimiser passes that
compile but never actually rewrite because a supporting mnemonic or
label-tracking machinery is not yet in `paideia_as_ir`. Both surface
as `TODO` comments in the same crates and are triaged together to
avoid double-filing.

| Category               | Signature                                                          |
|------------------------|--------------------------------------------------------------------|
| `native-lowering`      | Add a native encoder path for an intrinsic (no runtime call).      |
| `satellite-runtime`    | Wire the intrinsic to a satellite `.a` extern-C thunk.             |
| `encoder-fallback`     | Fall back to an encoded sequence when a mnemonic is not in enum.   |
| `ir-mnemonic-add`      | Extend `Mnemonic` enum + `encode_instruction.rs` dispatch table.   |
| `walker-threading`     | Thread `LocalBindingTable` (or peer) through a walker pass.        |

### 4.2 Entries

| Entry id          | Site                                                                                                       | Symptom                                                                                                          | Fix category         | Size | Landing wave |
|-------------------|------------------------------------------------------------------------------------------------------------|------------------------------------------------------------------------------------------------------------------|----------------------|------|--------------|
| `PAS-DEBT-B3-001` | `crates/paideia-as-ir/src/opt/tailcall.rs:33`                                                              | Recursion detection stubbed — tail-call pass never fires on self-recursion.                                     | `walker-threading`   | M    | Wave 2       |
| `PAS-DEBT-B3-002` | `crates/paideia-as-ir/src/opt/tailcall.rs:43`                                                              | Capability boundary / handler-install / effect-row extraction TODO in tail-call rewrite check.                   | `walker-threading`   | M    | Wave 2       |
| `PAS-DEBT-B3-003` | `crates/paideia-as-ir/src/opt/unroll.rs:156,169`                                                           | Body-duplication + remainder-loop emission TODO in unroll pass — pass identifies loops but rewrites none.        | `ir-mnemonic-add`    | L    | Wave 3       |
| `PAS-DEBT-B3-004` | `crates/paideia-as-ir/src/opt/peephole.rs:165,175,245,352,361,384,391`                                    | Strength-reduce (mul→shl, div→shr), collapse-jump-to-next, combine-push-pop all blocked on missing mnemonics.    | `ir-mnemonic-add`    | M    | Wave 2       |
| `PAS-DEBT-B3-005` | `crates/paideia-as-ir/src/opt/schedule.rs:256`                                                             | Actual block reordering via arena TODO — pass computes schedule then discards it.                                | `walker-threading`   | M    | Wave 3       |
| `PAS-DEBT-B3-006` | `crates/paideia-as-elaborator/src/emit_walker_tests/scratch_and_ops.rs:754`                                | `unimplemented!("deferred: requires LocalBindingTable threading")` — field-access with non-`rdi` base rejected.  | `walker-threading`   | M    | Wave 2       |
| `PAS-DEBT-B3-007` | `crates/paideia-as-ir/src/abi.rs:38-40`                                                                    | Three abi.rs TODOs: aggregate-type classification (#1009), MS hidden-pointer sret (#1011), SysV `RDX:RAX` (#1012). | `native-lowering`    | L    | Wave 2       |
| `PAS-DEBT-B3-008` | `crates/paideia-as-elaborator/src/effect_walker.rs:312,321`                                                | `phase-4-m1-003` handler-clause effect-row save/record TODOs — effect walker does not push `HandlerSideTable`.   | `walker-threading`   | M    | Wave 3       |
| `PAS-DEBT-B3-009` | `crates/paideia-as-elaborator/src/lower/kind_map.rs:59`                                                    | Dedicated `IrKind::HandlerValue` deferred to phase-2 — handler values ride on `Placeholder` today.               | `native-lowering`    | S    | Wave 3       |

### 4.3 Cross-references to forward-tracked intrinsics

The following are **not** debt; they are release-track work already
issued. Cross-listed so a Phase-2 filer does not duplicate:

- `#1361` — u128 arithmetic intrinsics (`@mulu64`, `@divu64`) — v0.26 M1.
- `#1381` — `f16` intrinsic — v0.30 M1.
- `#1379`, `#1380` — SPIR-V / WGSL embed intrinsics — v0.30 M1.
- `#1346` — ECDSA-P256 sign + verify — future crypto wave.
- `#1392` — encoder-conservative BLAKE3 hash intrinsic — v0.33 M1 (relates to B4-001).

---

## 5. Bucket 4 — `stdlib_lowering` placeholders

### 5.1 Rationale

Recipes in `crates/paideia-as-elaborator/src/stdlib_lowering/` that
compile and dispatch but return a semantically weaker result than the
trait promises. Distinct from B3 in that these are *elaborator*
recipes, not `paideia_as_ir` opt passes; and distinct from B1 in that
the failure is silent (recipe returns success) rather than a `build_emit`
assertion break.

| Category            | Signature                                                                     |
|---------------------|-------------------------------------------------------------------------------|
| `hash-placeholder`  | Weak (non-cryptographic) hash used where a cryptographic hash was promised.   |
| `sret-deferred`     | Caller-allocated-buffer convention chosen because sret marshalling absent.    |
| `encoder-arm-gap`   | Encoder is missing an operand-shape arm; recipe hard-codes byte width.        |

### 5.2 Entries

| Entry id          | Site                                                                                       | Symptom                                                                                                                            | Fix category        | Size | Landing wave |
|-------------------|--------------------------------------------------------------------------------------------|------------------------------------------------------------------------------------------------------------------------------------|---------------------|------|--------------|
| `PAS-DEBT-B4-001` | `crates/paideia-as-elaborator/src/string_intern.rs:13-27`                                  | FNV-1a-64 hash used for the symbol dedup table. Adequate for interning; **not** adequate for `libpdx-schema-registry`'s content-addressed keying. `#1392` tracks the BLAKE3 replacement. | `hash-placeholder`  | M    | Wave 1 (`#1392`) |
| `PAS-DEBT-B4-002` | `crates/paideia-as-elaborator/src/stdlib_lowering/cpuidops.rs:13-26,217`                   | `cpuid_leaf` full record-return deferred to a separate design pass. The two shipped SysVRegs recipes are pure-scalar workarounds. | `sret-deferred`     | L    | Wave 2       |
| `PAS-DEBT-B4-003` | `crates/paideia-as-elaborator/src/stdlib_lowering/mldsaops.rs:15-27`                       | `mldsa65_sign` uses (A) caller-allocated 3309-byte buffer over (B) sret record return, for the same reason as B4-002.             | `sret-deferred`     | M    | Wave 2       |
| `PAS-DEBT-B4-004` | `crates/paideia-as-elaborator/src/emit_store_record.rs:554`                                | `encode_mov` does not accept `[MemSib, Imm64]`; record-store recipes hard-code an 8-byte literal to work around it.               | `encoder-arm-gap`   | S    | Wave 1       |

### 5.3 Note

`emit_store_record.rs:554` is a self-contained encoder gap: the fix is
one arm in `encode_mov` and a delete of the hard-coded 8-byte
literal. Sized S. Escalating B4 to L would be wrong.

---

## 6. Bucket 5 — Test-runner vaporware

### 6.1 Rationale

`paideia-as test <fixture.pdx>` currently succeeds against fixture
files that neither exist nor parse — the runner does no I/O beyond a
`std::fs::read_to_string(...).ok()` and a substring scan for the
literal string `#[test]`. Any build.sh Phase that expects
`paideia-as test` to gate a fixture regression is running a
false-positive gate.

### 6.2 Entry

| Entry id          | Site                                                                                | Symptom                                                                                                              | Fix category     | Size | Landing wave |
|-------------------|-------------------------------------------------------------------------------------|----------------------------------------------------------------------------------------------------------------------|------------------|------|--------------|
| `PAS-DEBT-B5-001` | `crates/paideia-as-test/src/lib.rs:71-111` + `crates/paideia-as/src/cmd_test.rs:65-69` | `TestRunner::discover` is a plain-text substring scan; `run_human_format` reports all discovered tests as passed.    | `native-lowering` | M    | Wave 1 (`#1393`) |

### 6.3 Cross-reference (per umbrella §"Known catalog entry")

- **paideia-as#1349** — original OPEN bug report.
- **paideia-as#1393** — v0.33-M1-007 release-track companion.
- **ls v1.0.1 adversarial verification** — discovery context; see body
  of `#1349` for the three concrete symptoms observed against
  `tests/human_size_fixtures.pdx`, a nonexistent path, and outright
  garbage input.

The catalog entry `PAS-DEBT-B5-001` **is** the sub-issue for `#1349`
in this catalog's numbering scheme; the Phase-2 filer should attach
the id to the existing `#1349` rather than opening a duplicate.

---

## 7. Bucket 6 — Satellite runtime shim

### 7.1 Rationale

Satellite host tools (`mkfs.pdxfs`, `mount.pdxfs`, `umount.pdxfs`)
compiled by `paideia-as` emit `call` relocations against the crypto
FFI intrinsics declared by `stdlib_lowering::cryptoops` +
`stdlib_lowering::mldsaops`. On kernel builds those resolve via the
`paideia-as-crypto` + `paideia-pq-sign` rlibs on the link line; on
satellite `ld -nostdlib` link lines neither rlib is present and the
final link fails with unresolved symbols.

`paideia-satellite-runtime` (`crates/paideia-satellite-runtime/src/lib.rs`,
0.29.1) already carries the re-exports needed to close this gap; the
outstanding sub-issue is the mechanical split into a dedicated
`crypto_shim.rs` module per manifest v0.33-M1-005.

### 7.2 Entry

| Entry id          | Site                                                                | Symptom                                                                                                       | Fix category         | Size | Landing wave |
|-------------------|---------------------------------------------------------------------|---------------------------------------------------------------------------------------------------------------|----------------------|------|--------------|
| `PAS-DEBT-B6-001` | `crates/paideia-satellite-runtime/src/lib.rs` (target `crypto_shim.rs`) | Symbol re-exports currently live in `lib.rs`; sub-issue is the split into `src/crypto_shim.rs` per `#1391`.   | `satellite-runtime`  | S    | Wave 1 (`#1391`) |

### 7.3 Cross-reference

- **paideia-as#1348** — original OPEN bug report on the missing shim.
- **paideia-as#1391** — v0.33-M1-005 release-track companion.

Entry `PAS-DEBT-B6-001` supersedes `#1348` under the same collapse
policy as B5-001 vs `#1349`.

---

## 8. Bucket 7 — Ignored corpus tests

### 8.1 Rationale

Test hygiene: seven `#[ignore]`'d test binaries or fixtures across the
non-`build_emit` corpora carry explicit "deferred pending X" reasons.
None represents active runtime failure; all represent debt against a
future driver / harness landing. Filing them as sub-issues gives the
Phase-2 pass a single dashboard for corpora reactivation.

### 8.2 Entries

| Entry id          | Site                                                                                                | Symptom (from source comment)                                                                                          | Size | Landing wave |
|-------------------|-----------------------------------------------------------------------------------------------------|------------------------------------------------------------------------------------------------------------------------|------|--------------|
| `PAS-DEBT-B7-001` | `tests/end-to-end/tests/examples_compile.rs` + `codes/m2_macro_*.pdx`                               | Corpus test `#[ignore]`'d pending structured IR payloads + macro driver (`#217`).                                     | S    | Wave 3       |
| `PAS-DEBT-B7-002` | `tests/reflection-corpus/tests/runner.rs`                                                           | Runner `#[ignore]`'d; comparator active but corpus fixtures deferred.                                                 | S    | Wave 3       |
| `PAS-DEBT-B7-003` | `tests/effects-corpus/tests/runner.rs:57` + `corpus/reject/index_u64_outside_rawmem_row.pdx`        | Reject fixture `#[ignore]`'d until call-resolution path lands.                                                        | S    | Wave 3       |
| `PAS-DEBT-B7-004` | `tests/linearity-regression` — `reject_corpus_emits_expected_s_codes`                               | `#[ignore]`'d; awaiting borrow-checker phase-4 driver hookup.                                                         | S    | Wave 3       |
| `PAS-DEBT-B7-005` | `tests/opt-regression/tests/encode_tight_regression.rs`                                             | `#[ignore]`'d pending encode-tight diagnostic wiring; documented in `design/toolchain/optimization-passes.md:49`.     | S    | Wave 3       |
| `PAS-DEBT-B7-006` | `tests/uefi-smoke/tests/smoke.rs`                                                                   | Boot smoke `#[ignore]`'d until m6-009+ ships a meaningful hello.efi.                                                  | S    | Wave 3       |
| `PAS-DEBT-B7-007` | `tests/lsp-harness/tests/harness.rs`                                                                | Latency probe `#[ignore]`'d for release-build profiling only.                                                         | S    | Wave 3       |

All seven are `size = S`. This bucket exists purely so the debt is
observable; the fixes are landing-dependent and cannot proceed until
the driver they gate on ships.

---

## 9. Cross-reference appendix

### 9.1 GitHub issues consumed or superseded by this catalog

| Issue            | Status | Catalog entry that supersedes it                          |
|------------------|--------|-----------------------------------------------------------|
| paideia-as#1349  | OPEN   | `PAS-DEBT-B5-001` (test-runner vaporware)                 |
| paideia-as#1348  | OPEN   | `PAS-DEBT-B6-001` (satellite runtime shim)                |
| paideia-as#1396  | OPEN   | This document as a whole (umbrella)                       |

### 9.2 GitHub issues cross-referenced (not superseded)

| Issue            | Status | Relation                                                            |
|------------------|--------|---------------------------------------------------------------------|
| paideia-as#1392  | OPEN   | Release-track for `PAS-DEBT-B4-001` (BLAKE3 replaces FNV).          |
| paideia-as#1393  | OPEN   | Release-track for `PAS-DEBT-B5-001` (v0.33-M1-007).                 |
| paideia-as#1391  | OPEN   | Release-track for `PAS-DEBT-B6-001` (v0.33-M1-005).                 |
| paideia-as#1361  | OPEN   | Forward u128 arith; adjacent to B3.                                 |
| paideia-as#1381  | OPEN   | Forward f16; adjacent to B3.                                        |
| paideia-as#1379  | OPEN   | Forward SPIR-V embed; adjacent to B3.                               |
| paideia-as#1380  | OPEN   | Forward WGSL embed; adjacent to B3.                                 |
| paideia-as#1346  | OPEN   | Forward ECDSA-P256; adjacent to B3.                                 |
| paideia-as#1009  | (cited in `abi.rs:38`) | Aggregate type classification (B3-007).                 |
| paideia-as#1011  | (cited in `abi.rs:39`) | MS hidden-pointer aggregate return (B3-007).            |
| paideia-as#1012  | (cited in `abi.rs:40`) | SysV `RDX:RAX` 128-bit return pair (B3-007).            |
| paideia-as#983   | (cited in `scratch_and_ops.rs`) | LocalBindingTable threading (B3-006).          |
| paideia-as#217   | (cited in `codes/m2_macro_*.pdx`) | Macro driver (B7-001).                       |

### 9.3 Source of "12 known" figure

`CHANGELOG.md` v0.22.0 entry:

> `paideia-as` `build_emit` suite 426 passed / 12 failed (12
> pre-existing, identical set confirmed via `git stash` against
> `39b3e93`)

The identical figure is re-cited in the v0.23.0 and v0.24.0 landing
notes; no landing between v0.22.0 and v0.32.0 has reduced or added to
the count. The catalog's Bucket 1 sizing of "≥ 12" is therefore a
stable lower bound rather than an aspirational target.

---

## 10. Filing plan (Phase 2)

### 10.1 Policy

Per umbrella §"Q-A-3 resolution": the sub-issues this catalog
produces **do not** appear in `.plans/next-wave-issues.tsv`. They are
filed only against `paideia-os/paideia-as` and cross-referenced from
this document.

### 10.2 Draft script

A draft script lives at `.plans/scratch/file-pas-debt-issues.sh`. It
mirrors the shape of `.plans/scratch/file-issues.sh` — including the
`flock`-based mutex, the resume-on-slug support, and the
per-slug body template — but reads its input from
`.plans/scratch/pas-debt-catalog.tsv` (a per-entry TSV materialised
from this document in Phase 2, one row per `PAS-DEBT-B<n>-<mmm>` id).
It is **not** marked executable this batch; Phase 2's filer chmods and
runs it after the B1-005..013 enumeration lands.

### 10.3 Filing order (dependency-first)

1. **B5** — file `PAS-DEBT-B5-001` and link it to `#1349` (`Closes
   #1349` in the body if the sub-issue is authored as a fix; else a
   plain reference). This is the highest-severity item because a
   silent test-runner gates every downstream discipline.
2. **B6** — file `PAS-DEBT-B6-001` and link to `#1348`.
3. **B4-001** — file and link to `#1392`.
4. **B4-002..004, B3-001..009, B2-001..004** — file in bucket order.
5. **B1-001..004** — file the four `#[ignore]`-visible items.
6. **B1-005..013** — file after `cargo test -p paideia-as --test
   build_emit` enumerates the nine unmarked runtime failures.
7. **B7-001..007** — file last; low priority, landing-gated.

### 10.4 Commit message for Phase-1 catalog landing

The parent agent will use the following exact string:

```
paideia-as debt catalog: categorise pre-existing build_emit failures, parser gaps, intrinsic TODOs

Closes #1396.
```

No body bullets per the "compact commit messages" repo convention; the
catalog document itself carries the detail.

---

## Provenance

- **Umbrella:** [paideia-as#1396](https://github.com/paideia-os/paideia-as/issues/1396) — filed per MASTER_PLAN.md Phase A + manifest v2 (Q-A-3 resolution, challenger §5.6).
- **Source figures:** `CHANGELOG.md` v0.22.0 entry ("426 passed / 12 failed (12 pre-existing)") + `.plans/scratch/CHANGELOG-v0.32-M1-003.md`.
- **Companion design docs:** `design/roadmap/paideia-as-tactical-issues.md` §2.1 (encoder gap forecast), `design/compiler/lambda-arity-stack-spill.md` (register discipline exemplar).
- **Sizing convention:** stated in §0.3; matches Wave 0 batch conventions.
- **Filing plan:** §10 mirrors `.plans/scratch/file-issues.sh` shape.
