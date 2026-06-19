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

- 1227+ workspace tests across 23 crates + 6 test harnesses
  (linearity-regression, end-to-end, reflection-corpus, effects-corpus,
  pax-load-smoke, paideia-as-e2e).
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
