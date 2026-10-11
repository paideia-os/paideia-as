# paideia-as design documents

**Status:** Index — AUTHORED as of paideia-as v0.37.0 (2026-10-11).

This directory is the paideia-as side of the memory record
"[Design documentation lives in design/](../../../.claude/)" —
every architectural and implementation decision for the toolchain
lands here as a markdown file. For the cross-OS source of truth
(surface-language specification, syntax reference, phase roadmap),
see the parent repository under
`paideia-os/design/toolchain/`.

Documents are tagged on their first non-title line with one of:

- **AUTHORED** — specification or design note, implementation may be
  partial or deferred.
- **Landed / Phase-N closure** — matches the implementation in-tree.
- **FROZEN v<version>** — surface locked; changes require a Major-wave
  review (`paideia-fmt-acceptance.md` is the recent exemplar).
- **COMPLETE as of v<version>** — the subject has reached feature
  completion; follow-ups are polish, not regression.
- **Closed / Retrospective** — phase-transition retrospectives, archival.

## Toolchain (`design/toolchain/`)

### Surface language and ABI

- [`abi.md`](toolchain/abi.md) — PaideiaOS x86_64 ABI (**Specification v1.0**).
- [`calling-convention.md`](toolchain/calling-convention.md) — register
  file, R15 handler table, R12/R13 capability handles, UEFI thunk
  (**Specification v1.0**).
- [`syntax-reference.md`](toolchain/syntax-reference.md) — grammar
  stub, resurrected FMT-0 (**Stub**).
- [`reserved-word-policy.md`](toolchain/reserved-word-policy.md) —
  the 71-word reservation list and the add/remove criteria
  (**AUTHORED**).
- [`unsafe-block-statement-grammar.md`](toolchain/unsafe-block-statement-grammar.md)
  — `unsafe { … }` as a statement (**Partial** — call expressions
  routed; Let/While/Loop/Return deferred).
- [`cli-target-triplets.md`](toolchain/cli-target-triplets.md) —
  `--target` surface (**MVP delivered**).

### Phase roadmaps and retrospectives

- [`phase-3-plan.md`](toolchain/phase-3-plan.md) — drafted at Phase 2
  closure.
- [`self-hosting-phase5-plan.md`](toolchain/self-hosting-phase5-plan.md)
  — Phase 5 self-hosting (**Phase 4 m13 catalogue**).
- [`phase-6-decision-gate-g8.md`](toolchain/phase-6-decision-gate-g8.md)
  — Phase 6 → Phase 7 self-hosting gate (**Phase 6 m7 gate**).
- [`phase-transition-2.md`](toolchain/phase-transition-2.md) through
  [`phase-transition-9.md`](toolchain/phase-transition-9.md) —
  per-phase retrospectives (**Closed**).
- [`rust-dep-gap-analysis.md`](toolchain/rust-dep-gap-analysis.md) —
  Rust-dependency surface the self-host must retire (**Phase 4 m13
  catalogue**).

### Phase 2 outcomes (elaborator, encoder, DWARF, link, bootstrap)

- [`bootstrap.md`](toolchain/bootstrap.md) — dual-bootstrap
  (NASM / GNU `as`) + DDC discipline (**Phase 2 m10 decision**).
- [`optimization-passes.md`](toolchain/optimization-passes.md) — the
  eleven O1500-series passes (**Phase 2 m9 closure**).
- [`debug-info.md`](toolchain/debug-info.md) — DWARF 5 + paideia
  vendor sections (**Phase 2 m11 closure**).
- [`paideia-link.md`](toolchain/paideia-link.md) — PAX format and
  the 4-phase linker contract (**Specification v1.0**).

### Phase 3 outcomes (per-node IR + LSP + pointers)

- [`per-node-ir-payload-phase3.md`](toolchain/per-node-ir-payload-phase3.md)
  — per-node IR payload (**Phase 3 m2 closure**).
- [`pointer-types-phase3.md`](toolchain/pointer-types-phase3.md) —
  typed raw-pointer surface (**Phase 3 m1 closure**).
- [`lsp-phase3.md`](toolchain/lsp-phase3.md) — tower-lsp architecture
  (**Phase 3 m4 closure**).

### Phase 4 outcomes (substructural + traits + stdlib + tooling)

- [`walker-hookups-phase4.md`](toolchain/walker-hookups-phase4.md) —
  walker scaffolding (**Phase 4 m1 closure**).
- [`borrowed-references-phase4.md`](toolchain/borrowed-references-phase4.md)
  — borrowed-reference syntax (**Phase 4 m4 closure**).
- [`borrow-checker-phase4.md`](toolchain/borrow-checker-phase4.md) —
  substructural lattice walker (**Phase 4 m6 closure**).
- [`records-enums-phase4.md`](toolchain/records-enums-phase4.md) —
  record / enum lowering (**Phase 4 m7 closure**).
- [`strings-and-loops-phase4.md`](toolchain/strings-and-loops-phase4.md)
  — control-flow + `Str` type (**Phase 4 m8 closure**).
- [`generics-and-traits-phase4.md`](toolchain/generics-and-traits-phase4.md)
  — monomorphising generics (**Phase 4 m9 closure**).
- [`allocator-phase4.md`](toolchain/allocator-phase4.md) — allocator
  and memory model (**Phase 4 m10 closure**).
- [`stdlib-phase4.md`](toolchain/stdlib-phase4.md) — `Option<T>` /
  `Result<T,E>` surface (**Phase 4 m11 closure**).
- [`tooling-phase4.md`](toolchain/tooling-phase4.md) — doc generator
  and test runner (**Phase 4 m12 closure**).

### Attributes

- [`attribute-abi.md`](toolchain/attribute-abi.md) — `@abi(sysv|msx64)`
  calling-convention annotation (**MVP, codegen deferred to #1011**).
- [`attribute-interrupt.md`](toolchain/attribute-interrupt.md) —
  `@interrupt` / `@interrupt_error` ISR entry sugar
  (**Phase-2 landed**).
- [`attribute-link_section.md`](toolchain/attribute-link_section.md) —
  `@link_section("name")` custom ELF section emission
  (**v0.19 UEFI-ABI**).
- [`jump-table-attribute.md`](toolchain/jump-table-attribute.md) —
  `@jump_table` dense O(1) match dispatch
  (**AUTHORED — design doc for #964**).
- [`ring-attribute.md`](toolchain/ring-attribute.md) — `@ring` lock-free
  SPSC/MPMC ring synthesis (**Landed — Phase 14 r14-008**).
- [`embed-primitives.md`](toolchain/embed-primitives.md) — `@guid` /
  `@include_bytes` / `@include_str` (**v0.19 UEFI-ABI**).

### Idioms and lowering recipes

- [`percpu-idiom.md`](toolchain/percpu-idiom.md) — per-CPU GS-segment
  counter idiom (**AUTHORED — design doc for #965**).
- [`fnptr-unsafe-pattern.md`](toolchain/fnptr-unsafe-pattern.md) —
  manual load-and-call before `fn_ptr_types`
  (**Landed — pa-r14-009**).
- [`function-pointer-types.md`](toolchain/function-pointer-types.md) —
  first-class function-pointer types (**Design spec for #979**).
- [`stdlib-trait-lowering.md`](toolchain/stdlib-trait-lowering.md) —
  mnemonic-direct lowering for bodyless stdlib trait methods
  (**Landed — PA-r16-007-backtrack, #1036**).
- [`klog-migration-helper.md`](toolchain/klog-migration-helper.md) —
  tokenizer-driven UART → klog rewriter (**v0.20 post-SELF-HOST**).
- [`command-dispatch-pattern.md`](toolchain/command-dispatch-pattern.md) —
  terminal command-dispatch strategies (**AUTHORED — pa-r18-006**).
- [`array-storage-allocation.md`](toolchain/array-storage-allocation.md)
  — stack/pool/heap discipline for array literals
  (**Landed v0.21.0**).

### Macros and reflection

- [`macros-phase1.md`](toolchain/macros-phase1.md) — phase-1 pattern
  macros + phase-2 reflection outcome
  (**Superseded by reflection track**).

### Formatter (paideia-fmt — COMPLETE)

- [`paideia-fmt-style-spec.md`](toolchain/paideia-fmt-style-spec.md) —
  canonical style spec (**COMPLETE as of v0.36.82**).
- [`paideia-fmt-acceptance.md`](toolchain/paideia-fmt-acceptance.md) —
  FMT-17 final acceptance report
  (**COMPLETE as of v0.36.82**).

### Unicode substrate (paideia-as-unicode — COMPLETE, UNI-16, v0.37.0)

- [`paideia-as-unicode-spec.md`](toolchain/paideia-as-unicode-spec.md)
  — crate specification (**COMPLETE 2026-10-11, UNI-16, v0.37.0**).
- [`paideia-as-unicode-api-freeze.md`](toolchain/paideia-as-unicode-api-freeze.md)
  — UNI-11 public-API freeze contract
  (**FROZEN 2026-10-11 against workspace v0.37.0**).
- [`paideia-as-unicode-acceptance.md`](toolchain/paideia-as-unicode-acceptance.md)
  — UNI-16 final acceptance report (**COMPLETE as of v0.37.0**).

### Type-system research tracks

- [`rank-restricted-hm.md`](toolchain/rank-restricted-hm.md) —
  rank-restricted let-polymorphism (**Spec landing R220.M11**).

### Deferred artefacts

- [`corpus-sfc-sweep.md`](toolchain/corpus-sfc-sweep.md) —
  substructural/effect/capability corpus sweep tool
  (**AUTHORED — deferred artefact for #1245**).

## Compiler (`design/compiler/`)

- [`lambda-arity-stack-spill.md`](compiler/lambda-arity-stack-spill.md)
  — lambda arity > 6 SysV stack-spill design
  (**Design, 2026-08-22**).

## Effects (`design/effects/`)

- [`driver-effects.md`](effects/driver-effects.md) — the five
  driver-side effects (`MmioRead`, `MmioWrite`, `CachePolicy`, …)
  (**AUTHORED — pa-r14-010**).

## Encoding (`design/encoding/`)

- [`32bit-mode-survey.md`](encoding/32bit-mode-survey.md) — 32-bit
  instruction forms required by the PVH boot bridge
  (**AUTHORED — reference survey for encoder v1.5 m2–m6**).

## Security (`design/security/`)

- [`pq-trust-root.md`](security/pq-trust-root.md) — the hybrid
  Ed25519 + ML-DSA-65 trust root (**Phase 2 m7 closure**).

## Standard library (`design/stdlib/`)

- [`cpuid-record-return.md`](stdlib/cpuid-record-return.md) —
  CPUID record-return marshalling contract
  (**Scope note, v0.21 boot-intrinsics**).

## Roadmap (`design/roadmap/`)

- [`paideia-as-v0.13-through-v0.20.md`](roadmap/paideia-as-v0.13-through-v0.20.md)
  — strategic multi-release plan (**osarch deliverable**).
- [`paideia-as-tactical-issues.md`](roadmap/paideia-as-tactical-issues.md)
  — tactical per-issue plan
  (**Planning document, softarch, 2026-07-04**).

## Top-level

- [`paideia-as-debt-catalog.md`](paideia-as-debt-catalog.md) —
  cross-component debt catalogue
  (**Design, 2026-09-03; audit refreshed 2026-09-26**).

## See also

- [`../CHANGELOG.md`](../CHANGELOG.md) — per-release notes (every
  `workspace.version` bump since v0.11.0).
- [`../STATUS.md`](../STATUS.md) — deep-dive component status.
- [`../README.md`](../README.md) — workspace entry point.
