# Phase 6 decision gate G8: Phase 7 entry criteria (self-hosting)

**Status:** Phase 6 m7-004 gate documentation.
**Scope:** G8 is the formal entry checkpoint for Phase 7 (paideia-as self-hosting to `.pdx`). This document lists all prerequisite work completed in Phase 6 and all blockers that must clear before Phase 7 starts.

## 0. Scope

Phase 7 goal: **Self-host paideia-as Tier 1 crates (paideia-as-lexer, paideia-as-ast, paideia-as-parser, paideia-as-diagnostics; ~30k LoC) to `.pdx` source, with `.pdx`-to-native cross-compile working under Tier 1 + 2 crates (no execution required).**

G8 gate certifies that:
1. All Phase 6 stdlib expansions are complete in paideia-stdlib.
2. Tier 1 cross-compile infrastructure (bootstrap fixtures, test harness) is proven.
3. No architectural blockers were discovered in Phase 6 Tier 1 partial ports.
4. paideia-os Phase 2 work runs in parallel (no blocking dependency).

## 1. Phase 6 prerequisite: stdlib expansions

Per `design/toolchain/rust-dep-gap-analysis.md` m13 inventory, Tier 1 + 2 crates depend on the following stdlib types/modules. **All must ship in paideia-stdlib before Phase 7 Tier 1 ports start.**

### Tier 1 minimum (paideia-as-lexer + parser minimum):

1. **SmallVec<T, N>** — stack-allocated Vec for small collections. ~200 LoC port.
   - Used by: paideia-as-ast (attribute/argument lists), paideia-as-encoder (operand collections), paideia-as-ir.
2. **Unicode XID character tables** — XID_Start / XID_Continue for identifier scanning.
   - Used by: paideia-as-lexer (keyword/identifier classification).
3. **Serialisation framework** (serde-equivalent) — JSON / TOML / SARIF encode/decode.
   - Used by: paideia-as-diagnostics (SARIF emitter), paideia-lsp (LSP payload).

### Phase 7 forward (not blocking Tier 1 minimum, but required for Tier 2):

4. **BLAKE3 hash module** — for content-addressed caching / PAX payload signing.
   - Used by: paideia-as-elaborator, paideia-as-emitter-pax, paideia-lsp.
5. **Lru cache type** — LRU eviction for elaborator / LSP incremental state.
   - Used by: paideia-lsp.

**Gate checkpoint:** All 5 items MUST be in paideia-stdlib (even if Tier 2 items #4–#5 are not yet integrated into Tier 2 codebases). No Phase 7 start until all are present.

## 2. Phase 6 Tier 1 proof-of-concept: bootstrap status

Phase 6 m6–m7 validated the cross-compile infrastructure:

- **paideia-as-lexer partial port** (m6-001): core lexer structs + token classification ported to `.pdx`; Unicode XID substitutes validated.
- **paideia-as-parser bootstrap fixture** (m6-002): mini-parser in tests/self-hosting/ proves `.pdx` grammar can express AST + recursion.
- **No architectural surprises**: FFI shims for external crates (cryptoki, reqwest, tower-lsp) validated; they can stay Rust or become stubs for Phase 7.
- **Cross-compile workflow proven**: paideia-as build → ELF64 binary for Tier 1 crates works.

**Gate checkpoint:** Tier 1 architectural feasibility = GREEN. No unexpected gaps. Phase 7 is execution, not exploration.

## 3. Carryover from Phase 6 surface deferrals

Phase 6 completed full build-emit surface activation (records, generics, traits, borrowed-refs, stdlib types). All surface deferrals are consumed; none forward to Phase 7.

**Phase 7 surface additions** (lower priority; gates behind self-hosting):

- Associated-type codegen activation (trait-method resolution per impl block).
- Full const-generics (const `N: usize` in `[T; N]` type lowering).
- Curried multi-arg lambda eta-reduction.
- LEA symbolref direct RIP-relative encoding.
- `&mut` affine-mode lifecycle enforcement (loop-back post borrow-checker audit).

None block Phase 7 Tier 1 start.

## 4. paideia-os Phase 2 parallel schedule

Per `feedback_phase6_to_paideia_os_resume.md` rule, Phase 7 runs **in parallel** with paideia-os Phase 2 device I/O + Tier 1 firmware work:

- paideia-as Phase 7: Tier 1 → Tier 2 self-host ports + stdlib expansions.
- paideia-os Phase 2: device controller enumeration + interrupt routing + TSC calibration.

**No blocking dependency between the two.** paideia-as Phase 7 completion does not gate paideia-os Phase 2. paideia-os Phase 2 completion may trigger paideia-as Phase 8 (Tier 3 port + execution scaffolding for runtime tests).

## 5. G8 gate checkpoint (go/no-go for Phase 7 start)

**Conditions for Phase 7 START:**

- [ ] paideia-stdlib ships all 5 stdlib expansions (SmallVec, Unicode XID, serde-family, BLAKE3, Lru).
- [ ] Phase 6 m7-004 is merged to main + tagged v0.6.0.
- [ ] All Phase 6 surface deferrals are consumed (no open surface TODOs forward to Phase 7).
- [ ] paideia-os Phase 2 work is scoped and running in parallel.

**On Phase 7 entry, paideia-as-lexer full Tier 1 port begins (m1-001).**

## 6. Closing note

Phase 6 proved out self-hosting architecture. Phase 7 executes the Tier 1 port with confidence: no surprises, clear stdlib dependency list, proven cross-compile flow.

The formal gate signature: **G8 (Phase 6 → Phase 7) opens after m7-004 merge + stdlib validation.**
