# paideia-as Phase 4 status (decision gate G5-stamped, G6-ready)

**Phase 4 substrate complete as of m14-002.** All fourteen Phase 4 milestones (m1–m14) are closed; 101 issues across PRs #592–#693. PaideiaOS-aware re-ordering applied: m7 → m9 → m10 → m8 → m11 → m1 → m2 → m3 → m4 → m5 → m6 → m12 → m13 → m14. See `design/toolchain/phase-transition-4.md` for the Phase 4 retrospective.

## Phase 4 milestone closure (m1–m14)

- **m7 — records + enums**: `struct` types + RecordLayoutTable; pattern bindings + P0199; record codegen; `enum` sum types + EnumLayoutTable; match exhaustiveness T0512; enum discriminant; corpus; IR RecordCons / FieldAccess / EnumCons / EnumDiscriminant; records-enums-phase4.md appendix.

- **m9 — generics + traits**: `<T>` grammar P0200; Type::Var + HrKind::{Star, Arrow}; trait declarations P0201 + impl blocks P0202; trait-bound resolution T0514; coherence T0513; monomorphisation table; associated types; derive-macro infrastructure (Eq, Hash, Debug); corpus; generics-and-traits-phase4.md appendix.

- **m10 — allocator + memory model**: Allocator trait + Layout; BumpAllocator; Arena; SystemAllocator with C1401/C1402 cfg-gates; Box<T>; allocator-phase4.md with Q3 dual-default Arena/SystemAllocator.

- **m8 — strings + loops**: string literals E0010/E0011; Type::Str fat pointer; heap String; for/while/loop/break/continue keywords; IR Loop/Break/Continue + LoopMetaTable; m3-006 unroll over explicit loops; corpus; strings-and-loops-phase4.md appendix.

- **m11 — stdlib bring-up**: Option, Result, Vec, String + Str ops, HashMap, IO effect + paideia.io capability + Stdin/Stdout/Stderr, File + Read + Write traits, Iterator + Map/Filter adapters; stdlib-phase4.md + stdlib-smoke kitchen-sink (135 LoC).

- **m1 — walker hookups**: Call/Match/Handle/Branch walkers; PositionIndex + NameResolutionTable population; macro-fusion / branch-hint / align / pool-constants 4-pass real-rewrite flip; walker-hookups-phase4.md.

- **m2 — encoder real-rewrites**: PE/COFF + DWARF + PAX emitters consume InstructionSideTable; per-emit DDC fixture; optimization-passes.md §2.2 m2 closure.

- **m3 — runtime integrations**: real cryptoki + yubihsm + reqwest TSA; hardware-lane activation guide; pq-trust-root.md Phase 4 m3 appendix.

- **m4 — borrowed references grammar**: `&T` / `&mut T` grammar P0196; Type::Ref interner; `&x` / `&mut x` / `*r` expressions; substructural Affine/Linear; IR Borrow/BorrowMut/Deref + BorrowSideTable; codegen-as-pointer; corpus; borrowed-references-phase4.md appendix.

- **m5 — region calculus**: RegionId + RegionGraph + transitive closure; lexical region inference; lifetime-variable surface syntax; per-binding region metadata in PositionIndex; region elision rules + L2001; region appendix.

- **m6 — borrow checker**: BorrowWalker S0906/S0907 (renamed from A0700/A0701); LifetimeWalker S0908 (was A0702); MutationWalker S0909 (was A0703); two-phase borrows; NLL precise drop + LastUseAnalyzer; ExtendedBorrowDiagnostic with SARIF relatedLocations; 40-fixture corpus; borrow-checker-phase4.md.

- **m12 — paideia-as tooling**: paideia-as test runner; paideia-as fmt CLI subcommand; paideia-as doc generator; tooling-phase4.md.

- **m13 — self-hosting groundwork**: port-target inventory across 21 crates; bootstrap-fixture .pdx mini-compiler in tests/self-hosting/; Rust-dependency gap analysis (10 stdlib expansions identified); stage-1 hash + DDC fixture in tools/ddc/run.sh; Phase 5 opening conditions in self-hosting-phase5-plan.md §8.

- **m14 — documentation closure**: phase-transition-4.md retrospective (m14-001); this STATUS.md update (m14-002); v0.4.0 tag + CHANGELOG (m14-003); examples README + stdlib walkthrough (m14-004).

## Workspace test totals

- Phase 2 close (m11-006): ~1614 tests.
- Phase 3 m9-002: 1829 tests.
- Phase 4 m14-002 (this PR): **2172 tests** across the workspace.

+343 delta vs Phase 3 close.

## Where to look next

- `design/toolchain/phase-transition-4.md` — Phase 4 retrospective.
- `design/toolchain/phase-4-plan.md` — the plan all 14 milestones derive from.
- `design/toolchain/self-hosting-phase5-plan.md` — Phase 5 plan + opening conditions.
- `design/toolchain/rust-dep-gap-analysis.md` — Phase 5 stdlib-expansion gap list.
- `design/toolchain/tooling-phase4.md` — paideia-as test/fmt/doc subcommand reference.
- `design/toolchain/borrow-checker-phase4.md` — m4+m5+m6 borrow stack reference.
- `CHANGELOG.md` — release-notes view of Phase 4 (v0.4.0 lands at m14-003).

Below is the Phase 3 closure (G4) followed by the original Phase 1 / Phase 2 history.

---

# paideia-as Phase 3 status (decision gate G4-stamped, G5-ready)

**Phase 3 substrate complete as of m9-002.** All nine Phase 3 milestones (m1–m9) are closed; #525 (NIST ACVP vectors for ML-DSA-65) stays open by design per its own AC. Phase 3 covers PRs #475–#587. See `design/toolchain/phase-transition-3.md` for the Phase 3 retrospective.

## Phase 3 milestone closure (m1–m9)

- **m1 — pointer types + raw memory** (13 issues, PRs #475–#487): `*T` in the type grammar (m1-001); type interner + Type::Ptr (m1-002); elaborator pointer-kind (Unrestricted) + T0511 reserved (m1-003); `index_*` intrinsic family (m1-004); RawMem effect + `paideia.raw_mem` capability (m1-005); IrKind::Load/Store + LoadStoreSideTable (m1-006); SIB-form codegen (m1-007); examples 15/16/17 retirement (m1-008/009/010); `ptr_sub` / `ptr_sub_bytes` intrinsics (m1-011); examples corpus regression test (m1-012); design/toolchain/pointer-types-phase3.md appendix (m1-013).

- **m2 — per-node IR payload** (6 issues, PRs #488–#493): Instruction payload schema + InstructionSideTable (m2-001); Mnemonic ↔ encoder bridge with iced-x86 round-trip tests (m2-002); elaborator populates InstructionSideTable (m2-003 — the chokepoint); opt-pass helper signatures migrated (m2-004); per-node payload regression corpus tests/ir-payload/ (m2-005); design/toolchain/per-node-ir-payload-phase3.md appendix (m2-006).

- **m3 — opt-pass real-rewrites** (9 issues, PRs #494–#502): peephole real rewrite (5/8 working; m3-001); schedule real rewrite (m3-002); dse real rewrite (m3-003); encode-tight real rewrite (m3-004); tailcall real rewrite (m3-005); unroll real rewrite + remainder loops (m3-006); macro-fusion / branch-hint / align / pool-constants (m3-007 — 4 passes as would-fire); pass-catalog regression (m3-008); optimization-passes.md Phase-3-m3 closure (m3-009).

- **m4 — elaborator-driven LSP** (7 issues, PRs #503–#509): elaborator per-position result store (PositionIndex, m4-001); hover uses PositionIndex (m4-002); definition + references use NameResolutionTable (m4-003); completion uses elaborator type context (m4-004); inlay hints use elaborator type results (m4-005); incremental engine integration + latency probe reactivation (m4-006); design/toolchain/lsp-phase3.md appendix (m4-007).

- **m5 — stage-0b GAS source** (3 issues, PRs #569–#571): GAS-syntax stage-0b entry point (m5-001 — byte-identical to NASM stage-0a `48 8d 47 01 c3`); ddc.yml dual-stage-0 activation in tools/ddc/run.sh (m5-002); bootstrap.md Phase 3 closure paragraph (m5-003).

- **m6 — hardware HSM integration** (5 issues, PRs #572–#576): PKCS#11 backend (cryptoki) (m6-001); YubiHSM2 backend with hybrid fallback + Q0902 (m6-002); HsmSigner trait + HybridSigner composer (m6-003); pq-corpus hardware lane #[ignore]'d tests (m6-004); design/security/pq-trust-root.md Phase 3 m6 section (m6-005).

- **m7 — substructural and effects cleanup** (5 issues, PRs #577–#582): S0902 linear let-shadow leak (m7-001); S0904 affine consumed twice across branches (m7-002 + #579 SARIF fix); S0905 ordered out of order in handler (m7-003); row-polymorphic scope subsumption — closes m7-004 D-row (m7-004); pq-trust-root.md §13 appendix update (m7-005).

- **m8 — signature lifecycle** (3 issues closed + 1 intentionally open, PRs #583–#586): RFC 3161 timestamping client (m8-001 — synthetic-token scaffold); revocation list + verify --revocation-list / --ignore-revocation (m8-002); NIST ACVP test vectors for ML-DSA-65 (m8-003 — STAYS OPEN per its own AC; upstream tracking in tests/pq-corpus/ML_DSA_ACVP_STATUS.md); design/security/pq-trust-root.md Phase 3 m8 section (m8-004).

- **m9 — documentation closure** (4 issues, PRs #587 + this PR): Phase 3 retrospective (m9-001); STATUS.md update (m9-002 / this PR); examples README refresh (m9-003); v0.3.0 tag + CHANGELOG (m9-004).

## Workspace test totals

- Phase 2 close (m11-006): ~1614 tests.
- Phase 3 m9-002 (this PR): **1829 tests** across the workspace.

The +215 delta covers: pointer-type tests (m1: +~50), IR payload schema + populate + opt-rewrite tests (m2/m3: +~70), LSP elaborator path tests (m4: +~25), HSM backends + revocation + timestamp (m6/m8: +~35), substructural-and-effects-cleanup walker tests (m7: +~15), plus regression-test scaffolding across milestones (m1-012, m2-005, m3-008, m6-004).

## Where to look next

- `design/toolchain/phase-transition-3.md` — Phase 3 retrospective.
- `design/toolchain/phase-3-plan.md` — the plan all milestones derive from.
- `docs/g4-prep.md` — G4 verification checklist (Phase 2 framework; Phase 4 will introduce G5).
- `CHANGELOG.md` — release-notes view of Phase 3.

Below is the original Phase 1 / Phase 2 history (decision gates G2 and G4) followed by per-milestone closure notes inserted in chronological order.

# paideia-as phase-1 status (decision gate G2)

This document tracks phase-1 completion against the eleven deliverables
in `design/toolchain/milestones.md` §2.3. Each is annotated with the PR
that closed it.

## Deliverables

| #   | Deliverable                                  | Closing PR(s)              | Status     |
|-----|----------------------------------------------|----------------------------|------------|
| 1   | Source / lexer / parser / AST / diagnostics  | #29–#62 (T0–T3)            | Closed     |
| 2   | Type checker (substructural lattice)         | #122–#129 (PR 35–39)       | Closed     |
| 3   | Effect rows + handlers (well-typedness)      | #130–#135 (PR 40–45)       | Closed     |
| 4   | Smoke-test elaboration (placeholder backend) | #116 (PR 34)               | Closed     |
| 5   | Pattern-based macros (decl/match/expand)     | #136–#138 (PR 46–48)       | Closed     |
| 6   | Macro hygiene (Lean 4 / Ullrich 2020)        | #139 (PR 49)               | Closed     |
| 7   | IR + ANF + effect rewrite                    | #140, #141 (PR 50, 51)     | Closed     |
| 8   | ELF64 emitter + x86_64 encoder               | #142, #143, #145, #146 (PR 52, 53, 55, 56) | Closed |
| 9   | Basic DWARF (`.debug_info` + `.debug_line`)  | #147 (PR 57)               | Closed     |
| 10  | LSP server                                   | —                          | **Phase 2**|
| 11  | Linearity-regression harness + smoke         | #149, #150, #151, #152 (PR 59, 60, 61, 62) | Closed |

Plus the calling-convention prologue/epilogue (#144 PR 54) and the
end-to-end CLI wire-up `paideia-as build --emit elf64` (#148 PR 58).

## Diagnostic catalog: emitted vs. catalogued

The diagnostic catalog (`paideia-as-diagnostics/diagnostics.toml`) defines
the `Cxxxx` code space; the table below reports which codes are actually
emitted by the front end as of HEAD.

### Lexer (E-category, 0001-0099)

| Range          | Catalogued | Emitted by HEAD                                |
|----------------|------------|------------------------------------------------|
| E0001–E0006    | yes        | yes (lexer)                                    |
| E0007, E0008   | no         | yes (scanner; out-of-catalog by `DiagnosticCode` semantics) |

### Parser (P-category, 0100-0299)

| Range          | Catalogued | Emitted by HEAD                                |
|----------------|------------|------------------------------------------------|
| P0101–P0109    | partial    | yes (Pratt + lookahead recovery)               |
| P0110          | no         | yes (`parse_macro` unknown fragment kind)      |

### Module system (M-category, 0300-0499)

| Code   | Source                                | Emitted by HEAD |
|--------|---------------------------------------|-----------------|
| M0308  | macro_match: no matching rule         | yes             |
| M0309  | macro_expand: unbound metavariable    | yes             |
| M0311  | macro_expand: recursion depth limit   | yes             |

### Types (T-category, 0500-0699)

T-codes are exercised by the type-environment + unifier in
`paideia-as-elaborator` and `paideia-as-types`. End-to-end wiring lands
when the IR walker dispatches on node payloads.

### Substructural (S-category, 0900-0999)

| Code   | Source                                | Emitted by HEAD |
|--------|---------------------------------------|-----------------|
| S0900  | check_linearity: never used           | yes (end-to-end)  |
| S0901  | check_linearity: overused             | yes (end-to-end)  |
| S0903  | check_ordered: out-of-order use       | yes (end-to-end)  |
| S0906  | branch_merge: branch mismatch         | yes (end-to-end)  |
| S0907  | check_lambda: illegal capture         | yes (end-to-end)  |

Codes S0902 / S0904 / S0905 are reserved for phase-2 substructural
refinements; reject-corpus fixtures exist (PR 60) and will light up
when those codes are allocated.

### Effects (F-category, 1100-1199)

| Code   | Source                                | Emitted by HEAD |
|--------|---------------------------------------|-----------------|
| F1100  | effect_infer: unhandled effect        | yes (end-to-end)  |
| F1101  | effects::registry redecl / check_handler | yes          |
| F1102  | effect_unify: handler order           | yes (end-to-end)  |
| F1105  | effect_unify: row mismatch            | yes (end-to-end)  |
| F1106  | check_pure: forbidden effect          | yes (end-to-end)  |

### Capabilities (C-category, 1300-1399)

| Code   | Source                                | Emitted by HEAD |
|--------|---------------------------------------|-----------------|
| C1300  | cap_infer: missing capability         | yes (end-to-end)  |

## Deliberately deferred to phase 2

Per `milestones.md` §2.3 + §2.5 and the project-vision constraints:

- **LSP server** (`paideia-lsp`) — deliverable 10.
- **Typed elaborator reflection** — phase-1 ships pattern-based macros only.
- **PE/COFF emitter** (`paideia-as-emitter-pe`) — UEFI loader stays NASM-built in phase 1.
- **PAX emitter** + **linker** (`paideia-as-emitter-pax`, `paideia-as-linker`) — phase 2 begins PaideiaOS subsystem migration.
- **PQ signing** (`paideia-pq-sign`) — PQ trust root is phase 2.
- **Formatter** (`paideia-fmt`) — explicitly not in deliverables §2.3.
- **Optimization passes** — phase 1 ships zero opt passes.
- **Full DWARF vendor-extension population** — phase 1 emits empty stubs (PR 57); phase 2 populates `.debug.paideia.caps` / `.debug.paideia.effects` / `.debug.paideia.sig`.

## Phase 2 m10 closure (DDC bring-up)

Diverse Double Compilation infrastructure is in place. The m10
series (PRs #458–#465) implements:

- **DDC harness scaffold (m10-001)** — `tools/ddc/run.sh`
  orchestrator that builds paideia-as twice (stable + nightly
  toolchains, falling back to a second stable build if nightly
  is unavailable). Logs toolchain versions. Drops both binaries
  at `tools/ddc/out/{a,b}/paideia-as`.
- **Byte-level differ + allowlist (m10-002)** — `tools/ddc/`
  promoted to workspace member with `ddc` lib + `ddc-diff` CLI.
  `Divergence` / `DiffReport` JSON output. `Allowlist`
  (start..=end + reason) TOML schema. Exit codes 0 (match modulo
  allowlist), 1 (unallowlisted divergence), 2 (error).
- **Build determinism env contract (m10-003)** — `det.rs` adds
  `build_timestamp` (honours `SOURCE_DATE_EPOCH`) +
  `map_path` (honours `PDX_PATH_PREFIX_MAP="OLD=NEW"`).
  `build_pe_object` threads `det::build_timestamp()` into the
  COFF `time_date_stamp`. `docs/build-determinism.md` documents
  the contract.
- **Format-gate corpus (m10-004)** — `tools/ddc/fixtures/` with
  10 .pdx fixtures + `tools/ddc/tests/format_gates.rs` that
  builds each fixture twice and asserts byte-identical output
  per emit format (pe-coff / elf64 / pax). Per-emit tests
  `#[ignore]`'d; fixture-count test active.
- **Nightly CI workflow (m10-005)** — `.github/workflows/ddc.yml`
  with cron schedule + workflow_dispatch + artifact upload.
  Advisory (continue-on-error). 30-day artifact retention.
- **Release-pipeline gate (m10-006)** — `.github/workflows/
  release.yml` with hard-fail ddc-gate job + audited bypass
  via `workflow_dispatch.ddc_bypass_justification`. Downstream
  build + sign jobs gated on DDC pass.
- **Operational docs (m10-007)** —
  `design/toolchain/bootstrap.md` records the **dual-stage-0
  decision** (NASM + GNU as) resolving OS-requirements §6
  design-clarification 1; single-stage-0 explicitly rejected
  as weakening Wheeler's argument. `docs/ddc.md` is the
  operational guide (8 sections: verification, local
  invocation, format-gate corpus, allowlist policy, CI
  integration, incident response, env-var contract,
  references).

Phase-2-m10 honesty:
- The infrastructure is in place; activation pairs with the
  GitHub Actions billing restoration. Both workflow files
  (ddc.yml, release.yml) are parseable + ready.
- The AC bullet "DDC running advisory on main ≥7 nights with
  no false positives" can't be met today because the nightly
  workflow has never run. m11 / Phase 3 carries the activation
  obligation.
- Stage-0b (GNU as) entry-point source is **not yet written**.
  The dual-stage-0 commitment is documented; the GAS-syntax
  source is m11 / Phase 3 work.

## Phase 2 m9 closure (optimization pass catalog)

The opt-in optimization catalog is in place. The m9 series
(PRs #446–#457) ships 11 passes:

- **Pass infrastructure (m9-001)** — `OptPass` trait,
  `OptDiagSink`, annotation parser, canonical-catalog
  dispatcher.
- **Peephole (m9-002)** — 8 canonical x86_64 rewrites. O1500.
- **Instruction scheduling (m9-003)** — latency table +
  `schedule_block`. O1503.
- **Macro fusion (m9-004)** — CMP+Jcc 16-byte fetch-window
  alignment. O1504.
- **DSE (m9-005)** — basic-block reverse-sweep dead-store
  elimination. O1505.
- **REX/EVEX tightening (m9-006)** — `can_shorten_add_to_32bit`
  + `can_use_rel8` + savings table. O1506.
- **Branch hint + align + pool-constants (m9-007)** — three
  small passes. O1507 / O1508 / O1509.
- **Tail-call elimination (m9-008)** — `TcoBlocker` enum;
  resolves OS-requirements §6 design-clarification 5. O1510.
- **Loop unrolling (m9-009)** — `TripCount` + `is_unroll_safe`.
  O1511 / O1512 (warning).
- **Catalog composition (m9-010)** —
  `dispatch_collecting_order` + proptest pins catalog-order
  invariance.
- **Diagnostic codes (m9-011)** — fills O1501 / O1502 reserved
  slots + regression test.

Phase-2-m9 honesty: each pass ships as a scaffolded "would-
fire" emitter. Helper functions (`schedule_block`, `dse_block`,
`pad_for_alignment`, `tco_blocker`, `is_unroll_safe`, etc.) ARE
callable today and unit-tested. The kind-only IR (m1-002)
doesn't expose per-node x86_64 mnemonics; flipping to real
rewrites is a single PR once that lands.

`design/toolchain/optimization-passes.md` is the canonical
phase-2 outcome appendix.

## IR walker wiring (Phase 2 m1: complete)

The substructural lattice, effect-row inference, and capability checks
are now wired through the lex → parse → lower → walk pipeline as of
Phase 2 m1-ir-walker-wiring (PRs #347–#360). The pieces:

- **m1-001 / 002 / 003** (PRs #347, #348, #349) — `IrArena.children_table`
  child-pointer schema, `IrWalker` trait + driver, `WalkerCtx` plumbing.
- **m1-004 / 005** (PRs #350, #351) — `LinearityWalker` for S0900 /
  S0901 / S0903 + Lambda capture S0907.
- **m1-006 / 007** (PRs #352, #353) — `EffectRowWalker` for F1100 /
  F1101 / F1102 / F1105 / F1106.
- **m1-008** (PR #354) — `CapWalker` for C1300.
- **m1-009** (PR #355) — `paideia-as build` runs all three walkers
  after lowering; diagnostics flow through to the human renderer.
- **m1-010** (PR #356) — `tests/linearity-regression/` corpus now
  drives the CLI via subprocess.
- **m1-011** (PR #357) — new `tests/end-to-end/` harness; one fixture
  per surfaceable code (S0900/01/03/06/07, F1100/01/02/05/06, C1300,
  T0501).
- **m1-012** (PR #358) — `design/toolchain/abi.md` + `src/toolchain/abi/abi.pdx`
  (canonical machine-readable ABI; ABI_VERSION = 1).
- **m1-013** (PR #359) — `tools/cross-build/` smoke infrastructure
  (NASM ↔ paideia-as ABI parity, instruction-stream diff level) +
  GitHub Actions CI lane.

Phase-2-m1 honesty: the walker state machines are unit-tested via
injection tables today. The walkers RUN on real `.pdx` source through
the CLI but mostly stay silent because the lowered IR is still
kind-only (no per-Perform op metadata, no per-Lambda declared cap
set). The reject corpus tests are `#[ignore]`'d with explicit
m2/m3/m5 unlock reasons. Diagnostics start firing on real source as
m2 (typed-elaborator reflection), m3 (full algebraic effects), and
m5 (modules + functors) thread structured payloads through the IR.

## Phase 2 m2 closure (typed elaborator reflection)

Q-A4 **typed elaborator reflection** is now at full power. The m2 series
(PRs #361–#372) implements:

- **Quote / Antiquote (m2-001 to m2-003)** — typed `Term` AST, grammar support,
  syntax validation (`quote { ... }` and `~(...)` within quotes).
- **Reflective elaborator API (m2-004 to m2-006)** — AST inspection + traversal,
  typed-term evaluator, splice operation (return elaborated Term to caller).
- **Typed macro expansion (m2-007 to m2-011)** — replaces pattern-only phase-1
  matcher; macros can call back into the elaborator, inspect types and effects,
  with hygiene guarantees (Lean-4-style, extended for capability systems).

M-codes (macro reflection):
- **M0308** — `macro_match`: no matching rule (end-to-end)
- **M0309** — `macro_expand`: unbound metavariable in template (end-to-end)
- **M0311** — `macro_expand`: recursion depth limit (end-to-end)
- **M0312** — `splice`: type mismatch in elaborated result (deferred to m3)

**Corpus harness**: new `tests/reflection-corpus/` workspace member (16+ accept,
8+ reject fixtures). Validates M-code emission on real source through the CLI
(subprocess model; mirrors `end-to-end` and `linearity-regression` patterns).

**Workspace count**: 22 crates + 4 test harnesses (added `reflection-corpus`).

## Phase 2 m7 closure (PQ signing)

The post-quantum signing infrastructure is now at full power.
The m7 series (PRs #424–#431) implements:

- **Ed25519 + ML-DSA-65 wrappers (m7-001)** — `paideia-pq-sign`
  hosts the `Signer` trait + thin newtypes around ed25519-dalek
  2.x and ml-dsa 0.1.1 (RustCrypto FIPS-204). Ed25519 uses
  `verify_strict` to reject malleable / small-order forms.
  RFC 8032 §7.1 Test 1 KAT pins the Ed25519 sign output;
  deterministic-rnd KAT pins ML-DSA-65.
- **Hybrid signature scheme (m7-002)** — `Hybrid = Ed25519 ||
  ML-DSA-65` with AND verification semantics. Wire sizes:
  PK 1984B, SK 64B (seed-only — ml-dsa 0.1 exposes only the
  32B seed form), Sig 3373B ≈ 3.4 KB matching paideia-link.md
  §1.1.
- **PAX header signature integration (m7-003)** — two-tier
  storage: the 32B header `pq_signature_placeholder` slot
  stores `BLAKE3(hybrid_signature)`; the actual 3373B
  signature lives in a separate `.paideia.sig` section.
  `pax_message_to_sign` + `embed_signature_hash` +
  `header_signature_hash_matches` wire the m4-007 content
  hash through hybrid sign / verify.
- **Delegation-scope check (m7-004)** — `KeyScope`
  + `check_delegation_scope` reads `.paideia.effects` (m4-004),
  enforces `pax.effects ⊆ key.scope`. New `Category::Q`
  (post-quantum) + Q0901 ("signing-key scope insufficient")
  added to the diagnostic catalog. The load-bearing
  rank-5-elaborator-reflection use case from pq-trust-root.md
  §12/§13.
- **Release-artifact signing (m7-005)** — `paideia-pq-sign
  release <path>` CLI subcommand + `release.rs` API
  (`hash_file` / `sign_release_artifact` /
  `write_detached_signature` / `verify_detached_signature`).
  Detached `.sig` alongside the artifact. `docs/
  release-signing.md` documents the flow.
- **Soft-HSM (m7-006)** — `SoftHsmFile` with Argon2id KDF +
  ChaCha20-Poly1305 AEAD over the HybridSecretKey. Versioned
  PDX-HSM\0 file format. CLI `hsm init` / `hsm release`
  subcommands replace m7-005's deterministic test keypair as
  the operational dev path. DEVELOPMENT-ONLY caveat
  documented.
- **PQ verification corpus (m7-007)** — `tests/pq-corpus/`
  workspace member with 6 happy-path + 4 failure-mode tests
  exercising m7-001..006 end-to-end (Ed25519, ML-DSA-65,
  Hybrid, PAX content-hash signing, scope-check, soft-HSM).

Q-codes (post-quantum, 0900-0999):

| Code  | Source                                | Status |
|-------|---------------------------------------|--------|
| Q0901 | scope_check: scope insufficient       | live   |

The m7 deliverable ships 6 new modules in paideia-pq-sign
(ed25519, mldsa, hybrid, pax, scope_check, soft_hsm, release)
+ 1 in paideia-as-emitter-pax (sign). Resolves the PQ trust
root question from pq-trust-root.md §5/§12/§13.

## Phase 2 m8 closure (LSP server)

The Language Server Protocol implementation for paideia-as is now at
full power. The m8 series (PRs #432–#444) implements:

- **tower-lsp scaffold (m8-001)** — Backend / capabilities() / initialize
  handler. ServerCapabilities advertises text_document_sync, hover,
  definition, references, completion, code_action, formatting,
  semantic_tokens, inlay_hint.
- **workspace manifest (m8-002)** — paideia-os.toml reader with
  load_from_dir + discover. NotFound is typed, not a panic.
- **textDocument sync (m8-003)** — DocumentStore + did_open/change/close
  + 3 snapshot edit sequences.
- **publishDiagnostics (m8-004)** — diagnose_document drives lexer +
  parser; to_lsp_diagnostic adapts severity / span / code / source.
- **parse cache (m8-005)** — LRU-bounded (default 64) by (Url, BLAKE3
  content hash). hits/misses counters; coarse invalidate_all.
- **hover (m8-006)** — Backend.hover + identify_token_at +
  format_hover_markdown. Phase-2 synthetic class inference via
  "linear:"/"affine:" prefix; real inference deferred to m8-008+.
- **definition + references (m8-007)** — text-based identifier
  matching with word-boundary discipline; cross-document refs via
  DocumentStore.iter().
- **incremental engine (m8-008)** — hand-rolled Salsa-style query
  memoisation. Per-file parse incremental; per-module elaborate
  coarse. AC bullet 1 met at file granularity.
- **completion (m8-009)** — context classifier (Statement /
  MemberAccess / TypeAnnotation / Identifier) + keyword + identifier
  + member completions.
- **code actions + formatting (m8-010)** — 5 code actions
  (drop_affine_binding / add_to_effect_signature / wrap_in_unsafe /
  convert_ascii_to_unicode / convert_unicode_to_ascii). paideia-fmt
  crate minimum-viable formatter + --stdin CLI.
- **semantic tokens + inlay hints (m8-011)** — tokenise + delta-
  encoded SemanticTokens; capability-binding sites via "cap:"
  prefix. Inlay hints after let/val with synthetic placeholders.
- **tree-sitter grammar (m8-012)** — tools/editor/tree-sitter-paideia/
  with 21-case test corpus across 4 fixture files. CI activation
  deferred.
- **editor configs (m8-013)** — VS Code, Helix, Emacs, Neovim
  recipes wiring paideia-lsp + tree-sitter-paideia.
- **lsp-harness (m8-014)** — tests/lsp-harness/ workspace member:
  4 correctness tests (diagnostics, hover, definition, references) +
  1 #[ignore]'d latency probe. Exercises paideia-lsp handlers
  programmatically via library API (no JSON-RPC stdio). Validates
  diagnostic publication, hover on linear: prefix, definition jumps,
  cross-document references, and single-char latency <100ms (release).

Workspace test totals refreshed: 1502+ tests across 25 crates + 11
test harnesses (added lsp-harness).

## Phase 2 m6 closure (PE/COFF emitter)

The PE/COFF emitter for Microsoft x64 / UEFI binaries is now at
full power. The m6 series (PRs #414–#422) implements:

- **PE/COFF headers (m6-001)** — DosHeader (64B), CoffFileHeader
  (20B), DataDirectory (8B), OptionalHeaderPe32Plus (240B) per
  the Microsoft spec. `const_assert` pins
  `240 = 24 + 88 + 16*8`. `new_efi_amd64()` constructor presets
  for UEFI use.
- **Shared encoder lift (m6-002)** — `paideia-as-encoder` crate
  hosts the x86_64 instruction encoder (Reg64, CodeBuffer +
  encoders + 39+ tests). emitter-elf's `encode.rs` becomes a
  one-line facade; emitter-pe gains the dep. Zero callsite
  churn.
- **Section emission + RVA (m6-003)** — 40B `SectionHeader` +
  `SectionTable` with `add_text` / `add_data` / `add_rdata` /
  `add_bss` + `finalize(section_alignment, file_alignment,
  headers_size)`. UEFI defaults 0x1000 / 0x200 produce first
  RVA 0x1000 and first file ptr 0x400. `IMAGE_SCN_*`
  characteristics constants + composed bundles for text /
  rdata / data / bss.
- **`.reloc` table (m6-004)** — `Relocation` + `RelocSection`
  serialise per-4KiB-page blocks (12B header + 2B entries +
  ABSOLUTE pad for odd-count blocks). Sort-by-RVA emission;
  `IMAGE_REL_BASED_DIR64 = 10` for x86_64 64-bit absolute
  fixups.
- **Imports + UEFI thunk (m6-005)** — `ImportSection`
  two-pass layout (descriptors → ILT → IAT → hint+name → DLL
  names) parameterised by `base_rva`. `emit_uefi_thunk`
  10-byte MS-x64 → PaideiaOS-native bridge (push r15 + call
  rel32 + pop r15 + ret). `calling-convention.md` §2.5
  documents the bridge as a SysV-bridge variant (resolves the
  third calling-convention target).
- **EFI subsystem + DLL characteristics (m6-006)** — 11
  `IMAGE_DLLCHARACTERISTICS_*` constants +
  `DLLCHARACTERISTICS_UEFI_APPLICATION` bundle (`DYNAMIC_BASE
  | NX_COMPAT`). `new_efi_amd64()` sets the bundle by
  default; subsystem already 0x0A EFI_APPLICATION from
  m6-001.
- **CLI `--emit pe-coff` (m6-007)** —
  `paideia-as build --emit pe-coff hello.pdx -o hello.efi`
  produces a structurally valid PE/COFF binary that
  `objdump -p` parses cleanly. Phase-2-m6-007 minimum: no
  imports / relocations / actual code in `.text`; m6-010+
  wires the elaborator-driven content.
- **UEFI smoke harness (m6-008)** — new
  `tests/uefi-smoke/` workspace member. `UefiEnv::probe`
  walks the host for OVMF firmware + qemu-system-x86_64;
  `build_hello_efi` composes a minimal `.efi` via the m6-007
  pattern; `boot_and_capture_serial` spawns QEMU with a
  30-second cap (AC). Env-check + structural-build tests
  ACTIVE; boot test `#[ignore]`'d behind a probe + a
  meaningful `.efi` (m6-010+).
- **Cross-build smoke (m6-009)** — second
  `tools/cross-build/` fixture (`uefi_loader`) + Rust
  harness in `tests/cross-build/`. Active tests pin the
  script and fixture file paths; full invocation
  `#[ignore]`'d because it needs `nasm` + `objdump`. Local
  invocation confirmed PASSing.

Workspace test totals refreshed: ~1360 across 24 crates + 9
test harnesses (added pax-load-smoke, uefi-smoke,
modules-multifile, cross-build).

## Phase 2 m5 closure (ML modules + functors)

Q-A7 **ML-style modules + applicative functors + first-class
modules** is now at full power. The m5 series (PRs #401–#412)
implements:

- **AST scaffolding (m5-001)** — `Signature`, `Structure`,
  `Functor` AST nodes with `SigDecl { Type, Val, Module, Include }`
  and `Def { Type, Val, Module }`. String placeholders for type
  and expression slots; m5-003 deferred to coordinate with arena
  threading.
- **Module-kind machinery (m5-002)** — `ModuleKind { Sig(SignatureKind),
  Pi { param_name, param_kind, body_kind, dependent } }` +
  `SignatureKind` + `kind_signature` / `kind_functor`. Establishes the
  applicative-vs-generative distinction via the dependent flag.
- **Structure elaboration → typed value (m5-003)** —
  `elaborate_structure(s, ctx, diags) → TypedValue { bindings,
  signature, span }`. Linearity threading via the existing
  `LinearityCtx`. Whole-word use detection (debugger fix from m5-003)
  prevents spurious S0901 when one field name is a prefix of another.
- **Signature matching (m5-004)** — `match_signature(structure,
  target, diags) → bool` for structural subtyping. M0301 (missing
  decl) + M0302 (kind/type mismatch). Recurses into nested
  modules; `Include` deferred to m5-006+ signature registry.
- **Applicative functor application (m5-005)** —
  `apply_functor(functor, argument, diags) → Option<TypedValue>`
  with BLAKE3 ApplyKey cache. Leroy (1995) applicative semantics:
  `F(M) == F(M)` along the same path is path-equal via cache hit.
- **Functor body elaboration (m5-006)** — `elaborate_functor_body`
  checks the body against the parameter signature's abstract
  shape. `"linear:"` / `"affine:"` prefix on ty placeholder strings
  drives substructural-class inference until phase-3 wires
  parser-level linearity syntax. S0900 / S0901 surface at the
  functor declaration site, not deferred to apply time.
- **Parser: functor application + sharing (m5-007)** —
  `parse_functor_app` accepts `F(M)`, `F(M)(N)`, and
  `F(M)(N) sharing (M::t = N::t, ...)`. New `ExprData::FunctorApp`
  + `SharingConstraint`. Standalone (NOT wired into parse_primary
  — protects value-level call). P0190 / P0191.
- **Sharing-constraint checker (m5-008)** —
  `check_sharing_constraints` with M0303 + multi-line
  `expected: / got: / diff: + N - M` form, plus Levenshtein-2
  "did you mean ..." suggestions. Reuses the `RowDiff` precedent
  from m3-013.
- **Parser: pack / unpack / let module (m5-009)** —
  `parse_pack_expr` (`pack M : S`), `parse_unpack_expr`
  (`unpack v`), `parse_let_module` (`let module N = unpack v in
  body`) for first-class modules. P0192 / P0193 / P0194.
- **Pack / unpack elaboration (m5-010)** — side-table convention:
  packed TypedValue has one `_packed_module` binding +
  `_pack_{blake3-hash}` sentinel signature. `elaborate_pack`,
  `elaborate_unpack`, `elaborate_let_module`. M0304 fires on
  unpack of a non-pack.
- **IR + PAX `.paideia.functors` (m5-011)** — `ModuleSideTable`
  + `FunctorEntry` (40B: symbol_id + param/result hashes +
  closure-data placeholders + flags). `SectionType::Functors`
  = 0x15. `functors_from_modules` bridge in the paideia-as
  driver (keeps PAX as a leaf wire-format crate).
- **File → module mapping (m5-012)** —
  `validate_file_module_mapping` enforces §7.6: one structure or
  functor per file; basename → PascalCase matches the module name.
  M0305 / M0306 (parser-emitted) / M0313. New
  `tests/modules-multifile/` workspace member.

M-codes (modules):

| Code  | Source                                | Status |
|-------|---------------------------------------|--------|
| M0301 | sig_match: structure missing decl     | live   |
| M0302 | sig_match: structure kind mismatch    | live   |
| M0303 | sharing checker: constraint violated  | live   |
| M0304 | pack: unpack expects a packed value   | live   |
| M0305 | file_module: name mismatch            | live   |
| M0306 | parser: multiple top-level modules    | live (parser) |
| M0313 | file_module: no top-level module      | live   |

P-codes (parser):

| Code  | Source                                | Status |
|-------|---------------------------------------|--------|
| P0190 | parser: malformed functor application | live   |
| P0191 | parser: malformed sharing constraint  | live   |
| P0192 | parser: malformed pack                | live   |
| P0193 | parser: malformed unpack              | live   |
| P0194 | parser: malformed let-module          | live   |

The m5 deliverables ship 8 new modules in `paideia-as-elaborator`
+ 1 in `paideia-as-ir` + 1 in `paideia-as-emitter-pax` + the
parser modules.rs from m5-007 + the file_module check in m5-012.
`tests/modules-corpus/` covers structure / signature / functor /
sharing / pack scenarios with 10 accept fixtures and 6 reject
fixtures. `tests/modules-multifile/` is the multi-file harness
target for m5-013+ cross-file imports.

## Phase 2 m4 closure (PAX + paideia-link)

Q-A5 **PAX format + capability-binding linker** is now at full power.
The m4 series (PRs #388–#399) implements:

- **PaxHeader + scaffolding (m4-001)** — 96-byte canonical header
  (magic + version + arch + flags + section-table offset + count +
  32B BLAKE3 hash + 32B PQ-sig placeholder). `const_assert` pins the
  size invariant.
- **SectionTable + standard sections (m4-002)** — 64-byte descriptor;
  Code 0x01, RoData 0x02, Data 0x03, Bss 0x04; plus reserved ids for
  the PaideiaOS-specific sections that follow.
- **`.paideia.caps` (m4-003)** — 32B per binding-site descriptor;
  SiteKind / LinClass / CapKind enums; BLAKE3-derived name hash so
  the section doesn't need a string-table reference for cross-PAX
  reconciliation.
- **`.paideia.effects` (m4-004)** — variable-length per-function
  effect-row entries; closed and open (row-polymorphic) rows
  round-trip.
- **`.paideia.unsafe` + `.paideia.opt-passes` + `.paideia.lin`
  (m4-005)** — 40B / 32B / 32B audit-trail entries. PassId enum
  covers Peephole, ANF, DSE, ConstFold, EffectRewrite.
- **`.symtab` + `.relocs` + `.imports` + `.exports` (m4-006)** —
  48B sym + 32B reloc (Abs64 / Pc32 / GotPc32 / PltPc32 / CapBind) +
  shared 32B CapDescriptor for imports + exports.
- **BLAKE3 content hash (m4-007)** — `CanonicalContent` zeroes the
  hash + sig slots before hashing so a verifier can recompute and
  compare deterministically.
- **CLI `--emit pax` (m4-008)** — `paideia-as build --emit pax`
  produces a minimal valid PAX; `pax-introspect` companion binary
  dumps the header + section table.
- **paideia-link parse (m4-009)** — `parse_pax` validates magic +
  version + section table; `parse_inputs` walks a list.
- **paideia-link resolve (m4-010)** — global symbol table +
  capability table across inputs; B1700 undefined-symbol + B1701
  unbound-capability diagnostics; Strong-wins-over-Weak semantics.
  (Note: linker codes use Category::B 1700-1799 per the existing
  diagnostic taxonomy; the issue body's "L0700" was off-spec since
  Category::L = linter/style 2000-2999.)
- **paideia-link relocate + emit (m4-011)** — Abs64 relocation
  application; final PAX emission with Executable flag + recomputed
  BLAKE3 hash; `link(inputs, output)` driver ties all four phases.
- **mock-supervisor smoke (m4-012)** — `tests/pax-load-smoke/` mock
  supervisor that loads a PAX, parses its metadata sections, and
  symbolically dispatches by BLAKE3 name hash. Sets the pattern the
  real m10 supervisor follows.

B-codes (binary emission, 1700-1799):

| Code  | Source                            | Status |
|-------|-----------------------------------|--------|
| B1700 | linker: undefined symbol          | live   |
| B1701 | linker: unbound capability        | live   |

The m4 deliverables ship 12 new modules in paideia-as-emitter-pax +
4 modules in paideia-as-linker. Resolves AS5 (BLAKE3 content hash)
and AS8 (PAX object format) from custom-assembler.md §15.

## Phase 2 m3 closure (full algebraic effects)

Q-A3 **full algebraic effects with handlers** is now at full power. The
m3 series (PRs #374–#386) implements:

- **Row schema + interner (m3-001, m3-002)** — `EffectRow::is_closed`,
  `EffectInterner::fresh_row_var` for monotonic allocation of fresh
  row variables across the elaborator pipeline.
- **Row-polymorphic inference (m3-003, m3-004)** — `generalize_row`
  attaches a fresh tail to closed rows at function exit (unless
  explicitly `!{}`); `call_site_instantiate_and_unify` composes fresh
  instantiation + unification at every call site.
- **Let-generalization scoping + T0510 (m3-005)** — `LetGenScope`
  stack tracks let-bound row variables; out-of-scope use fires T0510.
- **Handler well-typedness under polymorphism (m3-006)** —
  `check_handler_installation_polymorphic` composes F1101 op-set check
  with `handle_row` effect subtraction; tail preserved.
- **IR handler-value side-table (m3-007)** — `HandlerSideTable` carries
  the per-Handle payload (effect, ops, ret, finally) the kind-only IR
  can't hold directly. `pretty_handler` for snapshot tests.
- **ANF for handler bodies (m3-008)** — five new per-construct ANF
  helpers cover perform args, resume value, handler op body, finally
  clause, and the whole-handler walk.
- **Deep-handler compilation (m3-009)** — `ResumeMode` + `ResumeSiteTable`
  classify resume usage; `compile_deep_handler_op` lowers SingleShot to
  direct cont-call and MultiShot to capture-and-invoke.
- **Effect-rewrite extended (m3-010)** — `rewrite_perform_at_depth` for
  row-polymorphic perform sites; `rewrite_handler_install_trampoline`
  for multi-shot install loops. PBT verifies every resume site gets
  rewritten regardless of count.
- **Handler stack + AS3 (m3-011)** — `emit_handler_open` / `emit_handler_close`
  push/pop R15 around handler-bracketed regions; `sysv_bridge`
  push/pop R15 around C calls. Resolves AS3 from custom-assembler.md
  §15.
- **Effects regression corpus (m3-012)** — new `tests/effects-corpus/`
  with 15 accept + 8 reject fixtures (7 multi-shot, 4 nested
  handlers).
- **Row-mismatch diagnostic (m3-013)** — `RowDiff::render` produces
  `expected: / got: / diff: + N - M` form with tail tracking. F1105
  uses it.

F-codes (effect + capability under row polymorphism):

| Code  | Source                                | Emitted by HEAD |
|-------|---------------------------------------|-----------------|
| F1100 | effect_infer: unhandled effect        | yes (per-pass)  |
| F1101 | check_handler: handler well-typedness | yes (per-pass)  |
| F1102 | effect_unify: handler order           | yes (per-pass)  |
| F1105 | effect_unify: row mismatch (with diff)| yes (per-pass)  |
| F1106 | check_pure: forbidden effect          | yes (per-pass)  |
| T0510 | let-gen scope: row var out of scope   | yes (per-pass)  |

The m3 deliverables are unit-tested via injection tables; activation
through real `.pdx` source via the CLI tracks the IR-walker driver
work that lands as the elaborator threads structured handler /
perform payloads through.

## Phase 2 enabling deliverables (m1 outputs)

- **`design/toolchain/abi.md`** — canonical ABI specification (~330 lines).
  Covers register-file partitioning, calling convention, PaideiaOS
  extensions (R15 handler table, R12-R13 caps), version policy,
  object-file requirements.
- **`src/toolchain/abi/abi.pdx`** — machine-readable canonical form
  consumed by NASM (macro generator) and paideia-as (directly).
  Parses cleanly through `paideia-as check`. Pinned by an integration
  test.
- **`tools/cross-build/`** — orchestration + CI lane that builds the
  same module twice (NASM + paideia-as) and diffs the
  instruction-stream output. One m1 fixture (`add_one`); the matrix
  grows as m2/m5 ship per-node lowering.

## Workspace test totals

- 1502+ workspace tests across 25 crates + 11 test harnesses
  (linearity-regression, end-to-end, reflection-corpus, effects-corpus,
  lsp-harness, pax-load-smoke, modules-multifile, uefi-smoke, cross-build,
  pq-corpus, paideia-as-e2e).
- `cargo test --workspace` runs in well under 60 seconds.
- CI: temporarily disabled (GitHub Actions billing block); local
  `cargo test --workspace` is the gate. cargo-deny advisory remains
  pre-existing wildcard-dep warnings.

## Decision gate G2 → Phase 2

Phase 1 closed at decision-gate G2 with the toolchain self-hosting the
parse/lex/elaboration/emission pipeline for the supported source-
language subset and producing valid ELF64 objects with debug info.
**Phase 2 m1 is now complete**, removing the largest blocker between
G2 and a fully plumbed-through front end. Phase 2 proceeds with m2
(typed-elaborator reflection), m3 (full algebraic effects), m4 (PAX
+ paideia-link), m5 (ML modules + functors), m6 (PE/COFF emitter),
m7 (PQ signing), m8 (LSP server), m9 (optimization pass catalog),
m10 (DDC bring-up), m11 (closure) per the plan at
`.plans/phase-2/issues.md`.
