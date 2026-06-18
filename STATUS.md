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

- 905 workspace tests across 18 crates + 3 test harnesses.
- `cargo test --workspace` runs in well under 60 seconds.
- CI: fmt / clippy / build / doc / test all gating; cross-build is a
  separate advisory lane; cargo-deny is advisory (pre-existing
  wildcard-dep warnings in the CLI manifest).

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
