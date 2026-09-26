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
| 1 | `build_emit` pre-existing failures (v0.22 baseline)  |  13   | M    | Wave 1 encoder + elaborator (spread across v0.22.x .. v0.25.x) |
| 2 | Parser gaps surfaced by driver / substrate work      |  21   | L    | Wave 2 parser refactor (parallel to `#1360` v0.26 helpers)  |
| 3 | Intrinsic-table TODOs + IR-opt stubs                 |   9   | L    | Wave 2 IR-opt round + Wave 3 intrinsic completion           |
| 4 | `stdlib_lowering` placeholders (hash / sret / imm64) |   4   | M    | Wave 1 encoder round (`#1392` BLAKE3) + Wave 2 sret design  |
| 5 | Test-runner vaporware (`paideia-as test` is a no-op) |   0   | —    | **LANDED** in v0.33-M1-007 (`#1393`) — bucket now empty     |
| 6 | Satellite runtime shim (`crypto_shim.rs` + ml-dsa)   |   2   | M    | Wave 1 shim split (`#1391`) + Wave 2 `ml-dsa` `no_std` fix  |
| 7 | Ignored corpus tests (test-discipline hygiene)       |   7   | S    | Wave 3 corpora reactivation (post Waves 1 + 2)              |

**Aggregate:** 56 individually filable items across 6 active buckets
(B5 is done). The 2026-09-25 refresh added 17 parser gaps
(B2-005..021), 1 satellite-runtime blocker (B6-002), recovered the 6
remaining B1 runtime-failure names (B1-008..013, down from a reserved
9), retired B1-014..016 as silently-fixed since v0.22.0, and marked
B5-001 as landed. See §2.3 for the fresh Bucket-1 enumeration.

Formally, let `F` be the set of pre-existing `build_emit` runtime
failures (|F| = 12 per the v0.22.0 baseline) and `V` the set of
`#[ignore]`'d entries visible in tree (|V| = 7 per 2026-09-22 grep).
The two sets overlap but neither contains the other: three visible
entries (B1-002, B1-003, B1-005..007) are fixture / infra gates that
never reported FAILED, and nine runtime failures live in `F \ V` with
no `#[ignore]` marker. The catalog enumerates all of `V` concretely
(B1-001..007) and reserves `B1-008..B1-016` for `F \ V`, giving a
total of `|V| + |F \ V| = 16` actionable ids. See §2.2 for the
per-entry table and the material adjustment vs the umbrella's stated
"12".

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

The umbrella cites "12 known failures". A fresh grep over
`tests/build_emit/` (2026-09-22, `grep -rn '#\[ignore' …`) finds
**seven** `#[ignore]`'d tests. Four of them describe encoder / walker
debt (B1-001, B1-004..006) plus three infra / fixture gates (B1-002,
B1-003, B1-007). The remaining nine of the v0.22.0 "12 failed" count
compile clean but fail at runtime — their names are only recoverable
from a fresh test run.

| Entry id            | Site                                                                                                       | Symptom                                                                                                                   | Fix category        | Size | Landing wave |
|---------------------|------------------------------------------------------------------------------------------------------------|---------------------------------------------------------------------------------------------------------------------------|---------------------|------|--------------|
| `PAS-DEBT-B1-001`   | `crates/paideia-as/tests/build_emit/field_read.rs:39`                                                     | `field_access_cap_set_rights_deferred_pending_parser_support` — `struct` type-definition syntax not accepted by parser.   | `walker-gap` (via B2 parser gap)   | M    | Wave 2 parser + Wave 1 walker      |
| `PAS-DEBT-B1-002`   | `crates/paideia-as/tests/build_emit/pa10_007_data_symbol_names.rs:89`                                      | Requires `readelf` + `ld` in `$PATH`; integration test hoisted out of unit-test lane rather than a real failure.          | (infra)             | S    | Wave 3 CI wiring                    |
| `PAS-DEBT-B1-003`   | `crates/paideia-as/tests/build_emit/pa10_007_data_symbol_names.rs:122`                                     | Same as B1-002; separate `#[test]` guarded on `readelf`.                                                                  | (infra)             | S    | Wave 3 CI wiring                    |
| `PAS-DEBT-B1-004`   | `crates/paideia-as/tests/build_emit/bridge_thunk.rs:314`                                                   | `U1620` narrowing: MS x64 lambda bodies containing function calls not in current MVP set (identity / add-imm / literal).  | `walker-gap`        | M    | Wave 2 elaborator MS-x64 widening   |
| `PAS-DEBT-B1-005`   | `crates/paideia-as/tests/build_emit/typed_encoder_diagnostics.rs:140`                                     | `#[ignore = "TODO(#1105-followup): needs label-resolution fixture"]` — typed-encoder diagnostic requires missing fixture. | (fixture-add)       | S    | Wave 3 fixture backfill             |
| `PAS-DEBT-B1-006`   | `crates/paideia-as/tests/build_emit/typed_encoder_diagnostics.rs:148`                                     | `#[ignore = "TODO(#1105-followup): needs duplicate-symbol fixture"]` — typed-encoder diagnostic requires missing fixture. | (fixture-add)       | S    | Wave 3 fixture backfill             |
| `PAS-DEBT-B1-007`   | `crates/paideia-as/tests/build_emit/typed_encoder_diagnostics.rs:156`                                     | `#[ignore = "TODO(#1105-followup): needs lambda-no-offset fixture"]` — typed-encoder diagnostic requires missing fixture. | (fixture-add)       | S    | Wave 3 fixture backfill             |

The remaining **nine failures cited by the v0.22.0 CHANGELOG are not
marked `#[ignore]` in source** — they pass compile but fail at
runtime. Their identities cannot be recovered from grep alone; a
fresh test run (`cargo test -p paideia-as --test build_emit 2>&1 |
grep -E "^test .* FAILED"`) is the fastest enumeration.

For each of the nine unmarked-runtime failures a `PAS-DEBT-B1-008`
… `PAS-DEBT-B1-016` id is reserved; the parent's Phase-2 filing pass
runs the enumeration then flushes those ids to sub-issues. The draft
script at `.plans/scratch/file-pas-debt-issues.sh` reads the
enumeration output as its input file.

### 2.3 Reserved id block — RESOLVED 2026-09-25

The v0.22.0 baseline reported 12 pre-existing FAILED runtime tests.
The 2026-09-25 audit ran `cargo test -p paideia-as --test build_emit
--no-fail-fast` against v0.36.40 and observed the count has **drifted
downward from 12 to 6** (441 passed / 6 failed / 7 ignored). Six of
the original 12 failures were fixed in the intervening ~15 minor
releases without a CHANGELOG note pinning the individual regressions.
So the reserved block collapses from B1-008..B1-016 (9 slots) to
B1-008..B1-013 (6 slots):

| Entry id          | Test name                                                                             | Root-cause hint (from panic stderr)                                                                    | Fix category           |
|-------------------|---------------------------------------------------------------------------------------|--------------------------------------------------------------------------------------------------------|------------------------|
| `PAS-DEBT-B1-008` | `build_emit::bss_array_length_const::expression_array_length_is_rejected_with_t0577`  | Parser rejects const-expression array length `[u64; MAX_PIDS * SLOT_QWORDS]`; `T0577` expected but `P0100` "expected `]` found `*`" produced instead. | parser-gap (feeds B2-x) |
| `PAS-DEBT-B1-009` | `build_emit::label_patches::label_fixup_jz_backward_local_disp32_not_zero`            | `C1301` — `unsafe` port-I/O in lambda body not lifting `PortIo` effect into the lambda's `!{}` row.    | elaborator-effect-lift |
| `PAS-DEBT-B1-010` | `build_emit::pa10_006l_inout::pa10_006l_inout_instructions_emit`                      | Same `C1301` root cause as B1-009: `_start` port-I/O declaration synthesis gap.                        | elaborator-effect-lift |
| `PAS-DEBT-B1-011` | `build_emit::typed_encoder_diagnostics::encoder_failure_typed_diagnostic_in_sarif`    | `[m1-003] estimated_offset 0 != encoded text_bytes 5` — advisory tracker off-by-one vs `InstructionSideTable::byte_offset_in_text` (encoder owns truth). Exit-code mismatch (`Some(0)` where `Some(2)` expected). | encoder-diagnostic-align |
| `PAS-DEBT-B1-012` | `build_emit::typed_encoder_diagnostics::encoder_warn_diagnostic_appears_in_stderr_when_no_sarif` | Same m1-003 tracker root cause as B1-011.                                                             | encoder-diagnostic-align |
| `PAS-DEBT-B1-013` | `build_emit::typed_encoder_diagnostics::encoder_warn_typed_diagnostic_in_sarif`       | Same m1-003 tracker root cause as B1-011.                                                             | encoder-diagnostic-align |

So the total Bucket-1 id count is now **13** (B1-001..007 visible +
B1-008..013 recovered), not 16. IDs B1-014..016 are formally
**retired** — they were reserved for failures that had already been
silently fixed by the 2026-09-25 audit date.

### 2.4 Sizing rationale

The 6 recovered failures triage into 3 groups:
- **1 parser gap** (B1-008): feeds through B2 (const-expr in bracket
  position); estimated S once the parser grammar is extended.
- **2 elaborator regressions** (B1-009, B1-010): shared root cause
  (`unsafe`/`PortIo` lift through lambda), single fix likely closes
  both; estimated M.
- **3 encoder-diagnostic alignment failures** (B1-011..013): shared
  root cause (m1-003 tracker off-by-one), single fix likely closes
  all three; estimated M.

Total across the bucket: ~3 M-sized fixes + ~3 S-sized (`#[ignore]`
clears from §2.2) + 2 M for infra/fixture (B1-002, B1-003, B1-005..007
group) = ≈ 10 engineer-days; downgrade the bucket from L to M.

---

## 3. Bucket 2 — Parser gaps

### 3.1 Rationale

Each entry is a construct the parser rejects (or silently drops) that a
downstream consumer of `paideia-as` (`paideia-os` driver code,
`paideia-stdlib` recipes, or a `build_emit` fixture) needs. The fix
category taxonomy:

| Category             | Signature                                                              |
|----------------------|------------------------------------------------------------------------|
| `production-add`     | Add a new grammar production for a shape today's parser skips.        |
| `terminal-add`       | Extend the lexer with a new terminal (keyword, punctuation).           |
| `recovery-rule`      | Change error-recovery behaviour so a downstream pass can still walk.  |
| `lexer-context-add`  | Extend the lexer context state (newline handling, sigil dispatch).    |
| `lowering-deferred`  | Surface parses but lowering to primitives deferred to a named wave.   |

### 3.2 Entries

Refreshed 2026-09-25 from a full-tree grep (`unimplemented!()` / `TODO` /
deferred markers / `#[ignore]`'d fixtures). B2-001..B2-004 are the
original entries; B2-005..B2-021 are new evidence surfaced by the
2026-09-25 audit pass under this umbrella (#1396).

| Entry id          | Site                                                                                        | Missing shape                                                                                                    | Fix category           | Size | Landing wave |
|-------------------|---------------------------------------------------------------------------------------------|------------------------------------------------------------------------------------------------------------------|------------------------|------|--------------|
| `PAS-DEBT-B2-001` | `crates/paideia-as-parser/src/parse_control.rs:295`                                         | `for pat in iter { body }` — pattern (`pat`) is parsed then dropped; only `Ident` header stored.                 | `production-add`       | S    | Wave 2       |
| `PAS-DEBT-B2-002` | `crates/paideia-as-parser/src/parse_item/generics.rs:104`                                   | Associated-type projections (`Iterator<Item = u64>`) lexed but not validated against trait's assoc-type set.     | `production-add`       | M    | Wave 2       |
| `PAS-DEBT-B2-003` | `crates/paideia-as-parser/src/parse_item/trait_impl.rs:588`                                 | `trait_args = Vec::new();` — trait-impl argument extraction stubbed with an unconditional empty vector.           | `production-add`       | M    | Wave 2       |
| `PAS-DEBT-B2-004` | `crates/paideia-as/tests/build_emit/field_read.rs:37-42` (parser side of B1-001)            | `struct` type-definition syntax needed by `cap_set_rights.pdx` fixture.                                          | `production-add`       | M    | Wave 2       |
| `PAS-DEBT-B2-005` | `crates/paideia-as-parser/src/parse_expr.rs:58,138` (+ `paideia-as-lexer` no `DotDot`)      | Range operator `a..b` and chaining `a..b..c` unparseable — no `DotDot` terminal + no production.                 | `terminal-add`         | M    | Wave 2       |
| `PAS-DEBT-B2-006` | `crates/paideia-as-parser/src/parse_stmt.rs:6-13`                                           | §9.2 multi-line expression statements + §9.3 newline-as-separator — lexer emits newlines as trivia.              | `lexer-context-add`    | M    | Wave 2       |
| `PAS-DEBT-B2-007` | `crates/paideia-as-parser/src/parse_primary/collection.rs:120,160-168`                      | Tuple exprs `(a, b, c)` — elements parsed into `_elements`, discarded, returns bare `Placeholder`.               | `production-add`       | S    | Wave 2       |
| `PAS-DEBT-B2-008` | `crates/paideia-as-parser/src/parse_type.rs:38-48`                                          | `forall v. T` — bound variable consumed and discarded; whole `forall` wrapper unstored.                          | `production-add`       | S    | Wave 2       |
| `PAS-DEBT-B2-009` | `crates/paideia-as-parser/src/parse_type/type_shape.rs:350-367`                             | Function-type param names `(name: T, ...) -> R` — `name` consumed and dropped; only type survives.               | `production-add`       | S    | Wave 2       |
| `PAS-DEBT-B2-010` | `crates/paideia-as-parser/src/parse_macro.rs:6-9,205`                                       | Macro pattern/template stored as span-only `Placeholder`; no fragment kinds, repetition, hygiene.                | `production-add`       | L    | Wave 3       |
| `PAS-DEBT-B2-011` | `crates/paideia-as-parser/src/toolkit_attrs.rs:58-66` + `crates/paideia-as-ast/src/functor_attr.rs:19` | `FunctorDecl` has no `NodeId` — `@retain`/`@immediate` cannot key into `FunctorAttrTable`.                       | `production-add`       | M    | Wave 2       |
| `PAS-DEBT-B2-012` | `crates/paideia-as-parser/src/parse_handler.rs:202-206`                                     | Handler bodies do not synthesize unit literal on trailing `;` (unlike if/loop; P0158 carve-out).                 | `recovery-rule`        | S    | Wave 2       |
| `PAS-DEBT-B2-013` | `crates/paideia-as-parser/src/parse_pattern.rs:272-286`                                     | Range / or-pipe (`p1 \| p2`) / reference (`&p`) / slice (`[a, b, ..]`) patterns rejected with generic P0100.     | `production-add`       | M    | Wave 3       |
| `PAS-DEBT-B2-014` | `crates/paideia-as-parser/src/timeline.rs:1-9`                                              | `@timeline_wait` / `@timeline_signal` parses; lowering to timeline-fence primitives deferred to v0.27-M2.        | `lowering-deferred`    | M    | v0.27-M2     |
| `PAS-DEBT-B2-015` | `crates/paideia-as-parser/src/endian_attr.rs:13-15`                                         | `@endian(be\|le)` parses + validates scalar shape; byte-swap insertion deferred to elaborator.                    | `lowering-deferred`    | S    | Wave 2       |
| `PAS-DEBT-B2-016` | `crates/paideia-as-shell-datalog/src/parser.rs:66-73`                                       | Zero-arity Datalog atoms (`foo().`) rejected until R226.M2 schema registry attaches types.                       | `production-add`       | S    | R226         |
| `PAS-DEBT-B2-017` | `crates/paideia-as-shell-ast/src/ast.rs:37-40`                                              | Process-substitution `>(cmd)` — AST variant reserved but pipeline parser does not accept it (R222 deferred).     | `production-add`       | S    | R222         |
| `PAS-DEBT-B2-018` | `crates/paideia-as-shell-ast/src/parser/mod.rs:16-22`                                       | Shell parser halts at first error; no sync-point recovery (deferred to R229 REPL incremental parse).             | `recovery-rule`        | M    | R229         |
| `PAS-DEBT-B2-019` | `crates/paideia-as-shell-lex/src/lexer.rs:52-54`                                            | `UnexpectedChar` on `#` / `@` sigils — attribute-macro tokens not handled (R221.M4 gap).                          | `lexer-context-add`    | S    | R221         |
| `PAS-DEBT-B2-020` | `crates/paideia-as-lexer/src/scan_comment.rs:9-12`                                          | Block-comment scanner stops at first `*/`; nested `/* /* ... */ */` unsupported.                                 | `lexer-context-add`    | S    | Wave 2       |
| `PAS-DEBT-B2-021` | `crates/paideia-as-lexer/src/token.rs` (site: no `DotDot` variant)                          | Lexer terminal for range operator (`..`) — companion to B2-005; must land first for parser to reach it.          | `terminal-add`         | S    | Wave 2       |

### 3.3 Note on shape

The bucket now spans five categories (`production-add`, `terminal-add`,
`recovery-rule`, `lexer-context-add`, `lowering-deferred`). B2-005..021
were added by the 2026-09-25 audit; the original assertion that "every
entry is production-add" no longer holds. `PAS-DEBT-B2-005` and
`PAS-DEBT-B2-021` are companions — the lexer terminal must land before
the parser production can consume it. B2-014 and B2-015 are
"parses-but-lowering-deferred" cases where the surface is fine but
downstream lowering to primitives waits on named waves (v0.27-M2 for
timeline fences, elaborator work for endian byte-swap).

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
| `PAS-DEBT-B5-001` | `crates/paideia-as-test/src/lib.rs:161-263,388-461` (post-v0.33-M1-007)             | `TestRunner::discover` substring scan + `run_human_format` false-positive gate. **LANDED** in v0.33-M1-007 (#1393): discover now token-walks via `paideia_as_lexer::Lexer` for `Hash LBracket Ident("test") RBracket`; `run` invokes full parser+elaborator per file. | `native-lowering` | M    | **DONE** (v0.33-M1-007, #1393) |

### 6.3 Cross-reference (per umbrella §"Known catalog entry")

- **paideia-as#1349** — original OPEN bug report. **Closed by #1393
  landing.** The 2026-09-25 audit under this umbrella confirmed the
  substring-scan is gone; module header at `paideia-as-test/src/lib.rs`
  documents the Q-A-4 Option B scoped fix + 8 acceptance tests at
  lines 478-662.
- **paideia-as#1393** — v0.33-M1-007 release-track companion (landed).
- **Deferred to v0.34** (not this bucket, filed under the release
  track): per-test parallel exec, runtime fixture-invocation protocol,
  per-function isolation, recursive dir scan.

`PAS-DEBT-B5-001` is now **superseded** — no sub-issue to file.
Phase-2 filer must skip this row.

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

### 7.2 Entries

| Entry id          | Site                                                                | Symptom                                                                                                       | Fix category         | Size | Landing wave |
|-------------------|---------------------------------------------------------------------|---------------------------------------------------------------------------------------------------------------|----------------------|------|--------------|
| `PAS-DEBT-B6-001` | `crates/paideia-satellite-runtime/src/lib.rs` (target `crypto_shim.rs`) | Symbol re-exports currently live in `lib.rs`; sub-issue is the split into `src/crypto_shim.rs` per `#1391`.   | `satellite-runtime`  | S    | Wave 1 (`#1391`) |
| `PAS-DEBT-B6-002` | `crates/paideia-satellite-runtime/src/lib.rs:236-247`               | `pub use paideia_as_crypto::ffi::mldsa65_*` attempted but RustCrypto `ml-dsa` 0.1.1 pulls `std` via `crypto_common`, conflicting with `#![no_std] panic_impl`. Blocked `cargo build --workspace` with E0152 `panic_impl`. **RESOLVED in 0.36.54 (Wave 14, #1528)** by path (iii): satellite crate promoted to its own nested workspace root; see §7.3. | `satellite-runtime`  | M    | Wave 14 (RESOLVED) |

### 7.3 Cross-reference

- **paideia-as#1348** — original OPEN bug report on the missing shim.
- **paideia-as#1391** — v0.33-M1-005 release-track companion.

Entry `PAS-DEBT-B6-001` supersedes `#1348` under the same collapse
policy as B5-001 vs `#1349`.

`PAS-DEBT-B6-002` was added by the 2026-09-25 audit: it explained why
`cargo build --workspace` failed on a clean checkout (the `E0152
panic_impl` collision in `paideia-satellite-runtime`), which had been
silently gating every per-crate `cargo test` invocation as the pre-push
script's workaround.

**Resolution (0.36.54, Wave 14, #1528 close)** — path (iii) taken:
`paideia-satellite-runtime` is now its own nested cargo workspace root
at `crates/paideia-satellite-runtime/Cargo.toml`. The parent workspace
no longer lists it as a member (see the anchor comment in the parent
`Cargo.toml` `[workspace] members` block), which means parent-workspace
feature unification no longer applies to it — its
`paideia-as-crypto = { default-features = false }` is now honoured
because no other workspace consumer forces defaults on. `paideia-as-crypto`
is pulled by path across the workspace boundary; its own `.workspace`
inheritance resolves against the parent workspace it still belongs to.

`cargo build --workspace` at the repo root now succeeds without any
E0152 collision. The satellite runtime staticlib is built by a
dedicated script — `bash tools/build-satellite-runtime.sh` — which
runs `cargo build --release --manifest-path
crates/paideia-satellite-runtime/Cargo.toml`. The pre-push gate
(`tools/paideia-as-pre-push.sh`) runs both.

Trade-offs considered but not taken:

- **(i) `[workspace.exclude]`** — equivalent effect on parent feature
  unification but the crate remains a workspace non-member sharing the
  parent `Cargo.lock`. Rejected because (iii) is cleaner: nothing else
  in the workspace depends on this crate, so it does not need to share
  the parent lock; and a dedicated nested workspace lets `[profile]`
  differ (the satellite build wants `panic = "abort"` in both `release`
  and `dev`, independent of parent policy).
- **(ii) `[patch.crates-io]`** for `crypto-common` / `digest` — too
  deep; would require maintaining a fork or waiting on RustCrypto
  upstream to publish `no_std`-clean variants.

Cost of (iii): a second `Cargo.lock` at `crates/paideia-satellite-runtime/Cargo.lock`.
Dependency-version divergence between the two workspaces is possible
in principle but is bounded in practice: the sub-workspace pulls only
`paideia-as-crypto` (by path), which in turn pins `argon2`, `chacha20poly1305`,
`ml-kem`, `blake3`, `thiserror` to the same versions the parent workspace
uses. `cargo update` in the sub-workspace stays local and is a manual
step for a release check.

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
5. **B1-001..007** — file the seven `#[ignore]`-visible items.
6. **B1-008..016** — file after `cargo test -p paideia-as --test
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

## 11. Sub-issue filing plan

Per-item proposed title, priority, and dependency. Priorities: **P0**
gates every downstream build lane (silent success on `paideia-as
test`, missing satellite shim); **P1** blocks the next release-track
wave (`#1391`/`#1392`/`#1393` companions + Bucket-1 walker gaps);
**P2** planned work for Waves 2–3 (parser, IR-opt, elaborator threading);
**P3** landing-gated hygiene (ignored corpora, fixture backfill).

| Id                 | Proposed sub-issue title                                                            | Priority | Deps (catalog / issue)                    |
|--------------------|-------------------------------------------------------------------------------------|----------|-------------------------------------------|
| PAS-DEBT-B5-001    | `paideia-as test` runs zero fixtures + reports success — replace substring scan     | P0       | supersedes `#1349`; companion `#1393`     |
| PAS-DEBT-B6-001    | Split `crypto_shim.rs` out of `paideia-satellite-runtime/src/lib.rs`                | P0       | supersedes `#1348`; companion `#1391`     |
| PAS-DEBT-B4-001    | Replace FNV-1a-64 symbol-dedup with BLAKE3 for `libpdx-schema-registry`             | P1       | companion `#1392`                         |
| PAS-DEBT-B4-002    | `cpuid_leaf` full-record return: land sret marshalling design                       | P1       | depends on B3-007 (SysV/MS ABI classifier) |
| PAS-DEBT-B4-003    | `mldsa65_sign` sret return (drop 3309-byte caller-allocated buffer)                 | P1       | depends on B4-002 + B3-007                |
| PAS-DEBT-B4-004    | `encode_mov [MemSib, Imm64]` arm — retire hard-coded 8-byte literal                 | P1       | none                                       |
| PAS-DEBT-B3-001    | Tail-call pass self-recursion detection (currently stubbed)                          | P2       | depends on B3-006 (LocalBindingTable)     |
| PAS-DEBT-B3-002    | Tail-call rewrite: capability boundary / handler-install / effect-row guard         | P2       | depends on B3-008 (HandlerSideTable push) |
| PAS-DEBT-B3-003    | Unroll pass body-duplication + remainder-loop emission                              | P2       | depends on B3-004 (peephole mnemonics)    |
| PAS-DEBT-B3-004    | Peephole strength-reduce (mul→shl, div→shr) + jump-to-next + push/pop combine       | P2       | depends on `ir-mnemonic-add` (SHL/SHR)    |
| PAS-DEBT-B3-005    | Schedule pass block reordering via arena (currently computes then discards)         | P2       | depends on B3-003                          |
| PAS-DEBT-B3-006    | Thread `LocalBindingTable` through emit walker (field-access non-`rdi` base)        | P2       | cited by `#983`                            |
| PAS-DEBT-B3-007    | `abi.rs`: aggregate classifier (`#1009`) + MS hidden-ptr sret (`#1011`) + SysV `RDX:RAX` (`#1012`) | P1 | dep of B4-002, B4-003; cites `#1009`/`#1011`/`#1012` |
| PAS-DEBT-B3-008    | Effect walker: handler-clause save/record — push `HandlerSideTable`                 | P2       | dep of B3-002                              |
| PAS-DEBT-B3-009    | Introduce `IrKind::HandlerValue` (retire `Placeholder` ride-along)                  | P3       | dep of B3-008                              |
| PAS-DEBT-B2-001    | Parser: `for pat in iter { body }` — store pattern (currently dropped)              | P2       | dep of B1-004 walker widening              |
| PAS-DEBT-B2-002    | Parser: associated-type projection validation against trait's assoc-type set        | P2       | none                                       |
| PAS-DEBT-B2-003    | Parser: extract trait-impl `trait_args` from `TypeName` nodes (currently `vec![]`)  | P2       | none                                       |
| PAS-DEBT-B2-004    | Parser: `struct` type-definition syntax for `cap_set_rights.pdx`                    | P2       | blocks B1-001                              |
| PAS-DEBT-B1-001    | Un-`#[ignore]` `field_access_cap_set_rights_deferred_pending_parser_support`        | P1       | blocked-on B2-004                          |
| PAS-DEBT-B1-002    | `pa10_007_data_symbol_names` (line 89): wire `readelf`+`ld` gate in CI              | P3       | Wave 3 CI                                  |
| PAS-DEBT-B1-003    | `pa10_007_data_symbol_names` (line 122): same as B1-002 for the readelf-only test    | P3       | Wave 3 CI                                  |
| PAS-DEBT-B1-004    | Un-`#[ignore]` `bridge_thunk.rs:314` — MS x64 lambda body with call-body            | P1       | none                                       |
| PAS-DEBT-B1-005    | `typed_encoder_diagnostics:140` — supply label-resolution fixture (`#1105-followup`) | P3       | Wave 3 fixture backfill                    |
| PAS-DEBT-B1-006    | `typed_encoder_diagnostics:148` — supply duplicate-symbol fixture (`#1105-followup`) | P3       | Wave 3 fixture backfill                    |
| PAS-DEBT-B1-007    | `typed_encoder_diagnostics:156` — supply lambda-no-offset fixture (`#1105-followup`) | P3       | Wave 3 fixture backfill                    |
| PAS-DEBT-B1-008..016 | (reserved) nine runtime failures from v0.22.0 baseline — enumerate then file      | P1       | one issue per FAILED name from `cargo test`|
| PAS-DEBT-B7-001    | Reactivate `examples_compile.rs` + `codes/m2_macro_*.pdx` (macro driver `#217`)     | P3       | blocked-on `#217`                          |
| PAS-DEBT-B7-002    | Reactivate `reflection-corpus` runner                                               | P3       | Wave 3                                     |
| PAS-DEBT-B7-003    | Reactivate `effects-corpus` `index_u64_outside_rawmem_row.pdx` reject fixture       | P3       | Wave 3                                     |
| PAS-DEBT-B7-004    | Reactivate `linearity-regression reject_corpus_emits_expected_s_codes`              | P3       | Wave 3                                     |
| PAS-DEBT-B7-005    | Reactivate `opt-regression encode_tight_regression`                                 | P3       | dep on `optimization-passes.md:49`         |
| PAS-DEBT-B7-006    | Reactivate `uefi-smoke::smoke` once m6-009+ ships meaningful `hello.efi`            | P3       | blocked-on m6-009+                         |
| PAS-DEBT-B7-007    | Reactivate `lsp-harness::harness` latency probe under release-profile lane          | P3       | Wave 3                                     |

**Counts:** 2 P0 + 8 P1 + 12 P2 + 12 P3 + 9 reserved (P1) = 43 filable
ids (one line under B1-008..016 covers nine items).

### 11.1 Filing-script excerpt (bash + `gh` cli)

Draft only; not run this batch. Full driver at
`.plans/scratch/file-pas-debt-issues.sh`. The script reads a
per-entry TSV (`.plans/scratch/pas-debt-catalog.tsv`, columns:
`id\ttitle\tpriority\tdeps\tbucket\tsize`) and files one issue per
row. Bucket-1 rows B1-008..016 are appended to the TSV only after
the `cargo test` enumeration lands.

```bash
#!/usr/bin/env bash
# .plans/scratch/file-pas-debt-issues.sh (draft — chmod withheld)
#
# Requires: gh authenticated against paideia-os/paideia-as.
# Idempotent via slug-based grep of `gh issue list --state all`.
# Runs under flock so parallel invocations serialise.

set -euo pipefail
UMBRELLA=1396
TSV=".plans/scratch/pas-debt-catalog.tsv"
LOCK="/tmp/pas-debt-file.lock"

exec 9>"$LOCK"; flock -n 9 || { echo "another filer running"; exit 1; }

file_one() {
  local id="$1" title="$2" priority="$3" deps="$4" bucket="$5" size="$6"
  # Skip if an issue with this slug already exists (resumable).
  if gh issue list --state all --search "$id in:title" --json number \
       | grep -q '"number"'; then
    echo "skip: $id already filed"; return 0
  fi

  local body
  body=$(cat <<EOF
Sub-issue of paideia-as#${UMBRELLA} (debt-catalog).

**Catalog entry:** \`${id}\` — Bucket ${bucket}, size ${size}.
**Priority:** ${priority}
**Depends on:** ${deps:-none}

See \`design/paideia-as-debt-catalog.md\` §${bucket} for the site
citation, symptom, and fix-category taxonomy. This issue is the
per-item filing; do not close paideia-as#${UMBRELLA} until every
sub-issue has landed.
EOF
)

  gh issue create \
    --title "[${id}] ${title}" \
    --body "$body" \
    --label "debt,${priority}" \
    --milestone "next-wave"
}

# TSV columns (tab-separated):
#   id    title    priority    deps    bucket    size
while IFS=$'\t' read -r id title priority deps bucket size; do
  [[ "$id" == \#* || -z "$id" ]] && continue     # skip comments + blanks
  file_one "$id" "$title" "$priority" "$deps" "$bucket" "$size"
done < "$TSV"
```

Filing order follows §10.3 (dependency-first: B5 → B6 → B4-001 →
B4-002..004 → B3-* → B2-* → B1-001..007 → B1-008..016 → B7-*).

---

## Provenance

- **Umbrella:** [paideia-as#1396](https://github.com/paideia-os/paideia-as/issues/1396) — filed per MASTER_PLAN.md Phase A + manifest v2 (Q-A-3 resolution, challenger §5.6).
- **Source figures:** `CHANGELOG.md` v0.22.0 entry ("426 passed / 12 failed (12 pre-existing)") + `.plans/scratch/CHANGELOG-v0.32-M1-003.md`.
- **Companion design docs:** `design/roadmap/paideia-as-tactical-issues.md` §2.1 (encoder gap forecast), `design/compiler/lambda-arity-stack-spill.md` (register discipline exemplar).
- **Sizing convention:** stated in §0.3; matches Wave 0 batch conventions.
- **Filing plan:** §10 mirrors `.plans/scratch/file-issues.sh` shape.
