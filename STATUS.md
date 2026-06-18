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
| S0900  | check_linearity: never used           | yes (per-pass)  |
| S0901  | check_linearity: overused             | yes (per-pass)  |
| S0903  | check_ordered: out-of-order use       | yes (per-pass)  |
| S0906  | branch_merge: branch mismatch         | yes (per-pass)  |
| S0907  | check_lambda: illegal capture         | yes (per-pass)  |

Codes S0902 / S0904 / S0905 are reserved for phase-2 substructural
refinements; reject-corpus fixtures exist (PR 60) and will light up
when those codes are allocated.

### Effects (F-category, 1100-1199)

| Code   | Source                                | Emitted by HEAD |
|--------|---------------------------------------|-----------------|
| F1100  | effect_infer: unhandled effect        | yes (per-pass)  |
| F1101  | effects::registry redecl / check_handler | yes          |
| F1102  | effect_unify: handler order           | yes (per-pass)  |
| F1105  | effect_unify: row mismatch            | yes (per-pass)  |
| F1106  | check_pure: forbidden effect          | yes (per-pass)  |

### Capabilities (C-category, 1300-1399)

| Code   | Source                                | Emitted by HEAD |
|--------|---------------------------------------|-----------------|
| C1300  | cap_infer: missing capability         | yes (per-pass)  |

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

## IR walker wiring

Several per-pass checks (substructural lattice, effect-row inference,
capability inference) are implemented and unit-tested but **not yet
wired into the lex → parse → lower pipeline**. The wiring waits on the
IR carrying node-level child pointers; once that lands (a single PR in
the early phase-2 weeks), the harness fixtures (PR 60) light up and the
S/F/C codes start surfacing through end-to-end CLI invocations.

This is the single largest piece of work between G2 and a fully
plumbed-through front end. It's a mechanical integration — every per-
pass API was designed with the walker as the consumer.

## Workspace test totals

- 800+ workspace tests across 17 crates + 2 test harnesses.
- `cargo test --workspace` runs in well under 60 seconds.
- CI: fmt / clippy / build / doc / test all gating; cargo-deny is
  advisory (pre-existing wildcard-dep warnings in the CLI manifest).

## Decision gate G2

Phase-1 closes here. The toolchain self-hosts the parse/lex/elaboration/
emission pipeline for the supported source-language subset and produces
valid ELF64 objects with debug info. The runtime substrate, calling
convention, and effect-row sidecar metadata are documented under
`design/`. Phase 2 begins with the IR-walker wiring (one PR) and proceeds
to the deferred deliverables above per the §7 phase-2 plan.
