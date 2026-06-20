# Phase 3 plan

**Status:** drafted at Phase 2 closure (v0.2.0 tag); pre-implementation.
**Scope:** what `paideia-as` builds in Phase 3 to retire the carryover from `phase-transition-2.md` §5 and the operational deferrals in `docs/g4-prep.md` §5. **Self-hosting the assembler in `.pdx` is out of scope** — that is Phase 4+ work and depends on several Phase 3 outputs as preconditions.

## 0. Framing

Phase 2 left the substrate complete but with three named honesty gaps and a backlog of design-clarification deferrals (see `phase-transition-2.md` §1 disposition table; the D-marked rows are the Phase 3 backlog).

The three honesty gaps share a common root: **the IR is kind-only.** `IrNodeData` in `crates/paideia-as-ir/src/node.rs` carries `IrKind + LinClass + EffectRowId + Span` (20 B, well under the 48 B budget) but no per-node operand or instruction payload. Consequences:

- m1 walkers run silently on real source — nowhere to read structured perform-op metadata from.
- m8 LSP hover / definition / completion fall back to lexical heuristics — the elaborator has no per-position type result table.
- m9 opt passes emit "would-fire" markers — no per-node x86_64 mnemonics to rewrite.

Phase 3 picks one forcing function — **raw pointer types** — and uses it to drive the IR payload work that unblocks every downstream item. Pointer types are the smallest surface change that demands real operand structure (a `*u64` value has width, alignment, load/store form), they are required to retire the `Array` placeholder + `unsafe { }` wrappers in `examples/15..17`, and they are a hard precondition for any future self-hosting work.

The plan is ordered by dependency, not by perceived importance.

## 1. Milestones at a glance

| # | Name | Purpose | Size | Depends on |
|---|------|---------|------|------------|
| m1 | pointer-types-and-raw-memory | `*T` raw-pointer + `index_*` primitives; retire `Array` placeholder | L | — |
| m2 | per-node-ir-payload | Side-table carrying per-node instruction payloads | M | m1 |
| m3 | opt-pass-real-rewrites | Flip m9 "would-fire" markers to actual IR rewrites | M | m2 |
| m4 | elaborator-driven-lsp | Replace m8 lexical stand-ins with elaborator queries | M | m2 |
| m5 | stage-0b-gas-source | GNU `as` entry-point source; activates dual-stage-0 DDC | S | — |
| m6 | hardware-hsm-integration | PKCS#11 + YubiHSM2 backends behind the m7 Signer trait | M | — |
| m7 | substructural-and-effects-cleanup | Row-poly scope subsumption; S0902/S0904/S0905 | S | m2 |
| m8 | signature-lifecycle | Timestamping + revocation; true NIST ACVP vectors | S | — |
| m9 | documentation-closure | Phase-3 outcome appendices; tag v0.3.0 | XS | m1–m8 |

Nine milestones, 67 tasks total. Cross-cutting items (§11): CI billing restoration, CI reactivation sweep, PR-batching experiment.

Size legend: XS ≤ 1 day, S 2–4 days, M 1–2 weeks, L 3–5 weeks (per-author calendar).

## 2. m1 — pointer types and raw memory (the case study)

**Purpose.** Extend the surface language and IR with raw-pointer types (`*T`) and the `index_*` primitive family, retire the `Array` placeholder in `examples/15`, and shrink (or retire) the `unsafe { }` wrappers in `examples/16` and `examples/17`. m1 is the forcing function for m2 and m3; every downstream milestone reuses pieces of m1's surface, IR, or codegen.

**Substructural-class decision.** Raw `*T` is unrestricted (pointers can be aliased and copied at will); borrowed references `&T` and `&mut T` are **out of scope for Phase 3** — they would require a region calculus + borrow-checker pass, multi-milestone work in its own right. m1 lands the raw form and defers the borrowed forms with a rationale in `syntax-reference.md`.

**Pointer-arithmetic discipline.** Raw `*T` supports `index_T` and `index_T_set` primitives (typed load and store at element offset `i * sizeof(T)`) plus pointer equality and `ptr_sub*`. Arbitrary `p + n` arithmetic is deferred. Reason: keeping the surface narrow makes the indexed load/store the only shape the IR has to lower, which is what m9 peephole targets.

**Effect contract.** `*T` load is `!{RawMem}`; `*T` store is `!{RawMem}`; both require `@{paideia.raw_mem}`. This matches today's `effects: { MemRead }` / `effects: { MemCopy }` declarations in `examples/16` and `examples/17`, so the unsafe wrappers can retire (the contract moves into the function signature).

### Tasks

#### m1-001 — pointer type-grammar + parser

- **AC:**
  - `*T` parses at every type-grammar position (param, return, let-binding, signature decl).
  - Pretty-printer round-trips `*u8`, `*u64`, `**u8`, `*(u8, u64)`, `*(u64 -> u64)`.
  - 6 accept + 4 reject fixtures in `tests/end-to-end/corpus/ptr-*/`; new code P0195.
- **Crates:** `paideia-as-parser`, `paideia-as-ast`, `paideia-as-diagnostics`.
- **Size:** S.
- **Example impact:** `examples/15` can rename `Array` → `*u64` in commentary (actual swap is m1-008).

#### m1-002 — type interner + `Type::Ptr` variant

- **AC:**
  - `Type::Ptr { pointee: TypeId, mutable: bool }` added to the `Type` enum.
  - Structural hashing; `*u64` and `*u64` cons to the same `TypeId`.
  - Unifier handles `Ptr` (same-pointee unifies; pointee inference propagates through `Var`).
- **Crates:** `paideia-as-types`.
- **Size:** S.
- **Example impact:** `*u64` becomes a real type in `examples/15`.

#### m1-003 — elaborator pointer-kind + substructural class

- **AC:**
  - `elaborator::type_kind` returns `LinClass::Unrestricted` for `*T` regardless of pointee class.
  - `T0511` reserved (warning) for the future-borrowed form.
  - 2 walker fixtures verifying `*T` doesn't trigger S0901 when copied.
- **Crates:** `paideia-as-elaborator`, `paideia-as-diagnostics`.
- **Size:** XS.
- **Example impact:** `xs: *u64` can be passed without linearity diagnostics.

#### m1-004 — `index_*` primitive set in the elaborator

- **AC:**
  - `index_u8` / `u16` / `u32` / `u64` (+ signed + float variants) registered as elaborator intrinsics with signature `(*T, u64) -> T !{RawMem} @{paideia.raw_mem}`.
  - Mutating siblings `index_*_set : (*T, u64, T) -> () !{RawMem} @{paideia.raw_mem}`.
  - Resolver dispatches before user-name lookup.
- **Crates:** `paideia-as-elaborator`, `paideia-as-effects`.
- **Size:** S.
- **Example impact:** `examples/15` calls `index_u64(xs, i)` directly; the placeholder `read_index = fn ... -> 0` goes away.

#### m1-005 — `RawMem` effect + capability registration

- **AC:**
  - `RawMem` effect declared in `src/toolchain/abi/abi.pdx` prelude.
  - `paideia.raw_mem` capability registered as built-in dotted capability.
  - 3 fixtures in `tests/effects-corpus/` verifying F1100 fires when `index_u64` is called outside a `!{RawMem}` row.
- **Crates:** `paideia-as-effects`, `paideia-as-elaborator`.
- **Size:** XS.
- **Example impact:** `examples/16` and `17` can drop their local `MemCopy` / `MemRead` declarations.

#### m1-006 — pointer IR lowering (Load / Store nodes)

- **AC:**
  - `IrKind::Load` + `IrKind::Store` variants added.
  - `Load` children `[pointer, index]`; `Store` children `[pointer, index, value]`.
  - Lowerer translates `index_u64(xs, i)` → `Load` with side-table entry.
  - New `LoadStoreSideTable` records `{width, signedness, alignment}` — keeps `IrNodeData` ≤ 48 B.
- **Crates:** `paideia-as-ir`, `paideia-as-elaborator`.
- **Size:** S.
- **Example impact:** `read_index(xs, i)` in `examples/15` lowers to a real IR `Load`, not a function-call stub.

#### m1-007 — codegen for indexed load/store

- **AC:**
  - `paideia-as-encoder` lowers `Load { width: 64 }` → `mov rax, [rdi + rcx * 8]` SIB form.
  - Per-width encoder tests for `u8` / `u16` / `u32` / `u64` loads and stores.
  - `tests/end-to-end/index_smoke` builds `examples/15` to ELF64 and verifies the SIB byte via `objdump -d`.
- **Crates:** `paideia-as-encoder`, `paideia-as-emitter-elf`.
- **Size:** M.
- **Example impact:** `examples/15` flips from "parses cleanly" to "compiles end-to-end" — the first algorithm example to do so.

#### m1-008 — retire the `Array` placeholder in `examples/15_sum_array.pdx`

- **AC:**
  - `Array` references replaced with `*u64`.
  - Placeholder `let read_index = fn ... -> 0` deleted.
  - Body uses `index_u64(xs, i)` directly.
  - Status header updated to "compiles end-to-end."
- **Size:** XS.
- **Example impact:** the load-bearing literal change that gates m1 review.

#### m1-009 — shrink the `unsafe { }` wrapper in `examples/16_memcpy.pdx`

- **AC:**
  - The `unsafe { }` block reduces to the single `rep movsb` instruction (no more `mov rax, rdi` / `mov rcx, rdx`).
  - dst/src/count typed `*u8` / `*u8` / `u64`; signature `(*u8, *u8, u64) -> *u8 !{RawMem} @{paideia.raw_mem}`.
  - Local `MemCopy` effect declaration retired in favour of prelude `RawMem`.
  - Justification updated to explain why `rep movsb` specifically still needs the escape.
- **Size:** XS.
- **Example impact:** the file becomes ~60% smaller and clearer about what's actually unsafe.

#### m1-010 — retire the `unsafe { }` wrapper in `examples/17_strlen.pdx`

- **AC:**
  - `read_byte_at` rewritten as `fn (p: *u8) -> u8 !{RawMem} @{paideia.raw_mem} -> index_u8(p, 0)`; `unsafe { }` block deleted.
  - `scan` takes `*u8` cursor + `*u8` start; difference via `ptr_sub_bytes` (see m1-011).
  - Status updated to "compiles end-to-end."
- **Size:** XS.
- **Example impact:** a pure tail-recursive scan with no unsafe escape — the cleanest demonstration that m1's surface + IR work retired the most common Phase 2 wrapper.

#### m1-011 — pointer subtraction primitive

- **AC:**
  - `ptr_sub : (*T, *T) -> u64` returns element-distance.
  - `ptr_sub_bytes : (*T, *T) -> u64` returns byte-distance.
  - Codegen: `sub rax, rdi` for `u8`-byte case; per-width shift for the general element case.
  - Unit tests: signature, element-vs-byte semantics, per-width shift, reject different pointees.
- **Crates:** `paideia-as-elaborator`, `paideia-as-encoder`.
- **Size:** S.
- **Example impact:** `examples/17`'s `cursor - start` becomes `ptr_sub_bytes(cursor, start)`, matching the NASM `sub rax, rdi`.

#### m1-012 — examples corpus regression test

- **AC:**
  - `tests/end-to-end/examples_compile.rs` exercises every `compiles end-to-end`-status file under `examples/`.
  - Harness asserts file exists, build succeeds, resulting ELF64 `.text` has ≥1 instruction.
  - At least 3 examples carry `compiles end-to-end`: 15, 16, 17.
- **Size:** XS.
- **Example impact:** turns "did the examples work" from manual review into a CI check.

#### m1-013 — phase-3 outcome appendix in `syntax-reference.md`

- **AC:**
  - New section "Pointer types (phase 3)" covers grammar, substructural-class decision, `index_*` family, `ptr_sub*` family, deferred `&T` rationale, `RawMem` + `paideia.raw_mem`.
  - One-line update in `custom-assembler.md` §6.1 indexing the appendix.
- **Size:** XS.

### m1 summary

13 tasks. Closes when `examples/15`, `16`, `17` carry `compiles end-to-end` and the `examples_compile.rs` harness pins them.

## 3. m2 — per-node IR instruction payload

**Purpose.** Generalise the m1-006 `LoadStoreSideTable` pattern to cover every per-node operand / instruction-shape payload the m9 opt passes need. Output: a structured `InstructionSideTable` exposing per-node x86_64 mnemonics, register allocations, and operand encodings — without inflating `IrNodeData` past 48 B.

### Tasks

#### m2-001 — `Instruction` payload schema

- **AC:**
  - `Instruction { mnemonic: Mnemonic, operands: SmallVec<[Operand; 3]>, encoding_hint: Option<EncodingHint> }` in `paideia-as-ir`.
  - `Mnemonic` covers the m9-targeted ops: mov, add, sub, cmp, jcc, jmp, call, ret, rep movsb, lea.
  - `Operand` covers reg / imm / mem-sib / mem-disp.
  - `InstructionSideTable` keyed by `IrNodeId`.
- **Crates:** `paideia-as-ir`. **Size:** M.

#### m2-002 — `Mnemonic` ↔ encoder bridge

- **AC:** `paideia-as-encoder::encode_instruction(&Instruction, &mut CodeBuffer)` delegates to per-mnemonic encoders. Round-trip tests via iced-x86 (tests-only).
- **Size:** S.

#### m2-003 — elaborator populates `InstructionSideTable`

- **AC:** `Load`/`Store` populate from m1-006; `Var` resolves to register operands per calling convention; `App` of an intrinsic populates mnemonic + operand list.
- **Crates:** `paideia-as-elaborator`. **Size:** M.

#### m2-004 — opt-pass helper signatures updated

- **AC:** `schedule_block` / `dse_block` / `tco_blocker` / `is_unroll_safe` accept `&InstructionSideTable` instead of synthetic op lists; all m9 unit tests pass (helpers were already correct).
- **Size:** S.

#### m2-005 — per-node payload regression

- **AC:** new `tests/ir-payload/` with 8 fixtures (leaf load, leaf store, conditional branch, tail call, indexed accumulator mirroring `examples/15`, REP MOVSB mirroring `16`, per-byte scan mirroring `17`, multi-call body). Each asserts the expected `Instruction` payload at named node ids.
- **Size:** S.

#### m2-006 — phase-3 appendix in `custom-assembler.md` §6.1

- **AC:** new §6.1.N "Per-node instruction payload (phase 3)" covers schema, side-table convention, encoder bridge, deferred per-mnemonic extensions.
- **Size:** XS.

### m2 summary

6 tasks. Closes when m9 can ask the IR "what mnemonic is this node?" and get a structured answer.

## 4. m3 — opt-pass real rewrites

**Purpose.** Flip every m9 pass from "would-fire" to actually rewriting the `InstructionSideTable`. Helpers from `optimization-passes.md` §2 are already correct; this milestone is plumbing.

### Tasks

#### m3-001 — `peephole` real rewrite (O1500/01/02)

- **AC:** `peephole_pass::apply` calls into the 8-rewrite table; mutates `InstructionSideTable`. O1501 / O1502 (already reserved) light up per-rewrite. `tests/opt-regression/peephole/` fixtures pin the rewrites.
- **Size:** S.

#### m3-002 — `schedule` real rewrite (O1503)

- **AC:** `schedule_block` mutates `InstructionSideTable` in place. Latency table unchanged. Corpus fixture verifies 3-instruction reorder.
- **Size:** S.

#### m3-003 — `dse` real rewrite (O1505)

- **AC:** `dse_block` removes dead `Store`s from the side-table; corresponding `Store` IR nodes flagged dead. Corpus fixture pins 2-store pair elimination.
- **Size:** S.

#### m3-004 — `encode-tight` real rewrite (O1506)

- **AC:** `can_shorten_add_to_32bit` + `can_use_rel8` consulted at encode time; shorter form emitted. Corpus fixture pins byte-length delta.
- **Crates:** `paideia-as-encoder`, `paideia-as-emitter-elf`. **Size:** S.

#### m3-005 — `tailcall` real rewrite (O1510)

- **AC:** Non-blocked recursive tail calls rewrite to `jmp`. Fixtures from `examples/13`, `14`, `15`, `17` verify single back-edge in emitted ELF64.
- **Size:** S.

#### m3-006 — `unroll` real rewrite (O1511) + remainder loops

- **AC:** Divisible trip-counts unroll inline. Indivisible counts emit a remainder loop (retires `phase-transition-2.md` D-row "remainder-loop generation"). Corpus fixture pins remainder.
- **Size:** M.

#### m3-007 — `macro-fusion` / `branch-hint` / `align` / `pool-constants` real rewrites

- **AC:** Each flips to real rewrites against `InstructionSideTable`. O1504 / O1507 / O1508 / O1509 emit "rewrote N sites" diagnostics. Per-pass corpus fixture pins each rewrite.
- **Size:** M.

#### m3-008 — pass-catalog real-rewrite regression

- **AC:** `tests/opt-regression/` asserts every pass diagnostic flipped from "would-fire" to "rewrote N sites"; sentinel "marker" diagnostics retired.
- **Size:** S.

#### m3-009 — phase-3 closure in `optimization-passes.md`

- **AC:** §2 "Phase-2-m9 honesty" loses its disclaimer and gains a "Phase-3-m3 closure" header documenting the flip.
- **Size:** XS.

### m3 summary

9 tasks. Closes when the m9 catalog rewrites real IR and DDC pins byte-stable optimised emissions across runs.

## 5. m4 — elaborator-driven LSP semantics

**Purpose.** Replace m8's `"linear:"` / `"affine:"` / `"cap:"` prefix heuristics with real per-position elaborator queries. The m8-008 `QueryEngine` is in place; this milestone wires it to the elaborator's type-result store.

### Tasks

#### m4-001 — elaborator per-position result store

- **AC:** `PositionIndex` maps `(FileId, ByteOffset) → TypeId + LinClass + EffectRowId + CapSetId`. Populated as a side-effect of walker passes. `O(log n)` lookup over sorted span vector.
- **Crates:** `paideia-as-elaborator`. **Size:** M.

#### m4-002 — LSP hover uses `PositionIndex`

- **AC:** `hover` consults `PositionIndex` instead of `"linear:"` prefix; output shows real type, substructural class, effect row, capability set. The 4 lsp-harness hover tests pass against real elaborator output.
- **Crates:** `paideia-lsp`. **Size:** S.

#### m4-003 — LSP definition + references use elaborator name resolution

- **AC:** `definition` consults the elaborator's name-resolution table; `references` returns elaborator-tracked uses (not textual occurrences). Cross-document fixture verifies references find uses in imported modules.
- **Size:** S.

#### m4-004 — LSP completion uses elaborator type context

- **AC:** `completion` at a `MemberAccess` site consults `PositionIndex` for the receiver type and offers only that type's members. `TypeAnnotation` offers in-scope type names only.
- **Size:** S.

#### m4-005 — LSP inlay hints use elaborator type results

- **AC:** Inlay hints after `let` / `val` show the inferred type from `PositionIndex`. Fixture pins three inlay positions.
- **Size:** XS.

#### m4-006 — incremental engine integration with `PositionIndex`

- **AC:** `QueryEngine::invalidate_module` invalidates the `PositionIndex` slice. Per-file edits don't re-elaborate the workspace. The `#[ignore]`'d m8-014 latency probe reactivates and asserts < 100 ms.
- **Crates:** `paideia-lsp`, `paideia-as-elaborator`. **Size:** M.

#### m4-007 — phase-3 LSP design appendix

- **AC:** New `design/toolchain/lsp.md` (or appendix in `paideia-link.md`) documents the m8 + m4 architecture; m8-006..009 synthetic-class-inference notes removed.
- **Size:** XS.

### m4 summary

7 tasks. Closes when the m8-006..009 lexical stand-ins are deleted and lsp-harness exercises real elaborator queries.

## 6. m5 — stage-0b GAS entry-point source

**Purpose.** Retire the "stage-0b GAS entry-point source" D-row. The dual-stage-0 commitment is already in `bootstrap.md`; this milestone writes the actual GAS source so `ddc.yml` activates dual-stage-0 verification.

### Tasks

#### m5-001 — GAS-syntax entry point

- **AC:** `src/toolchain/stage-0/entrypoint.s` (GAS syntax) is 1:1 with the NASM stage-0a entry point. `as` assembles cleanly on Linux + glibc. Diff against stage-0a output is empty modulo the m10-002 allowlist.
- **Size:** S.

#### m5-002 — ddc.yml dual-stage-0 activation

- **AC:** `ddc.yml` invokes `tools/ddc/run.sh` with both stage-0 variants. Differ confirms bit-identical output modulo allowlist. G4-prep §5 stage-0b item flips to checked.
- **Size:** S.

#### m5-003 — `bootstrap.md` phase-3 closure

- **AC:** §3 honesty paragraph removed; new §4 closure paragraph documents the m5-001 landing.
- **Size:** XS.

### m5 summary

3 tasks. Closes the bootstrap deferral end-to-end.

## 7. m6 — hardware HSM integration

**Purpose.** Retire the "hardware HSM integration" D-row. The m7 `Signer` trait is in place; this milestone adds PKCS#11 and YubiHSM2 backends behind it.

### Tasks

#### m6-001 — PKCS#11 backend (cryptoki)

- **AC:** `paideia-pq-sign::hsm::pkcs11` wraps a PKCS#11 session. Ed25519 + ML-DSA-65 keypairs read from a configured slot. `paideia-pq-sign hsm pkcs11 init` CLI subcommand. 4 unit tests against the `softhsm2` test backend.
- **Size:** M.

#### m6-002 — YubiHSM2 backend

- **AC:** `paideia-pq-sign::hsm::yubihsm` wraps the `yubihsm` crate. Ed25519 via firmware-derived keypair. ML-DSA-65 not yet supported by YubiHSM2 firmware: surface as `Q0902 hsm-no-pq-support`; hybrid signing falls back to soft-HSM for the PQ leg with explicit operator opt-in.
- **Size:** S.

#### m6-003 — HSM trait composition + soft-HSM fallback

- **AC:** `Signer::is_hardware()`. Hybrid composes hardware Ed25519 with soft-HSM ML-DSA-65 when YubiHSM is in use. `pq-trust-root.md` documents the composition rule.
- **Size:** S.

#### m6-004 — pq-corpus hardware lane

- **AC:** `tests/pq-corpus/` gains a `#[ignore]`'d hardware lane per backend; documented in `docs/release-signing.md`.
- **Size:** XS.

#### m6-005 — phase-3 appendix in `pq-trust-root.md`

- **AC:** New "Hardware HSM (phase 3)" section covers backends, composition rule, `Q0902`.
- **Size:** XS.

### m6 summary

5 tasks. m6 is independent of m1–m4 — schedule in parallel.

## 8. m7 — substructural and effects cleanup

**Purpose.** Retire three small D-rows: row-polymorphic scope subsumption and the reserved S0902 / S0904 / S0905 slots.

### Tasks

#### m7-001 — S0902 (linear resource leak across `let`)

- **AC:** Linearity walker fires S0902 when a linear binding is shadowed by `let` without being consumed. 3 corpus fixtures.
- **Size:** XS.

#### m7-002 — S0904 (affine resource consumed twice across branches)

- **AC:** Affine walker fires S0904 when two `match` arms both consume the same binding. 2 corpus fixtures.
- **Size:** XS.

#### m7-003 — S0905 (ordered resource used out of order across handler)

- **AC:** Ordered walker fires S0905 when handler bodies re-order ordered bindings. 2 corpus fixtures.
- **Size:** XS.

#### m7-004 — row-polymorphic scope subsumption

- **AC:** `check_handler_installation_polymorphic` accepts a strictly larger key-scope row when the function row is row-polymorphic with a fresh tail. 4 corpus fixtures in `tests/effects-corpus/`. Retires the `m7-004` D-row from `phase-transition-2.md` §1.
- **Crates:** `paideia-as-elaborator`. **Size:** S.

#### m7-005 — appendix update in `pq-trust-root.md` §13

- **AC:** Subsumption rule documented; Phase 2 "exact match only" caveat removed.
- **Size:** XS.

### m7 summary

5 tasks. Closes substructural lattice + scope-row deferrals.

## 9. m8 — signature lifecycle

**Purpose.** Retire "signature timestamping / revocation" and "true NIST ACVP test vectors for ML-DSA."

### Tasks

#### m8-001 — RFC 3161 timestamping client

- **AC:** `paideia-pq-sign timestamp` subcommand fetches an RFC 3161 token from a configurable TSA. Token attaches to the PAX `.paideia.sig` section as an additional sub-record. Verification chains TSA → release artifact.
- **Size:** S.

#### m8-002 — revocation list format + check

- **AC:** JSON-lines revocation list (key id + revocation date + reason). `paideia-pq-sign verify` consults the list; refuses revoked signatures unless `--ignore-revocation`. 4 unit tests.
- **Size:** S.

#### m8-003 — true NIST ACVP vectors for ML-DSA-65

- **AC:** Existing KAT replaced with NIST ACVP-format vectors once `ml-dsa` crate ships them. If upstream hasn't shipped by the m8 cut, document the upstream issue link; task stays open.
- **Size:** XS (assuming upstream lands; M otherwise).

#### m8-004 — phase-3 appendix in `pq-trust-root.md`

- **AC:** New "Signature lifecycle (phase 3)" section covers timestamping, revocation, ACVP-vector status.
- **Size:** XS.

### m8 summary

4 tasks.

## 10. m9 — documentation closure

**Purpose.** Phase 3 retrospective + design-clarification disposition update + v0.3.0 tag.

### Tasks

#### m9-001 — `design/toolchain/phase-transition-3.md`

- **AC:** Retrospective mirroring `phase-transition-2.md`: disposition table for every Phase 3 D-row, what didn't ship, what we got right, what we'd change, Phase 4 carryover.
- **Size:** S.

#### m9-002 — STATUS.md update

- **AC:** Per-milestone closure notes for m1–m8; workspace test totals refreshed.
- **Size:** XS.

#### m9-003 — examples README refresh

- **AC:** `examples/README.md` Phase-1 deferrals section updated; asm-reference equivalence section reflects 15/16/17 as `compiles end-to-end`.
- **Size:** XS.

#### m9-004 — v0.3.0 tag + CHANGELOG

- **AC:** Tag `v0.3.0` with release notes summarising m1–m8; `CHANGELOG.md` Phase-3 section.
- **Size:** XS.

### m9 summary

4 tasks. Closes Phase 3.

## 11. Cross-cutting

### 11.1 GitHub Actions billing restoration

Not in repo scope. Handed to release lead per `docs/g4-prep.md` §5. Gates the CI reactivation sweep.

### 11.2 CI reactivation sweep

When billing restores: reactivate `ci.yml`, `cross-build.yml`, `ddc.yml`, `release.yml` (currently parsed-but-disabled); run one-shot full workspace + DDC against m1/m2 outputs; activate tree-sitter grammar CI lane (m8-012); land as a separate small PR to avoid mixing infra and code changes. Size S.

### 11.3 PR-batching experiment

Per `phase-transition-2.md` §4, Phase 3 may batch trivial XS doc tasks. Rule: single-file XS = bundle-eligible; ≥2 files = per-PR. m9 (documentation closure) is the natural test bed.

## 12. Dependency graph

```
m1 ─┬─► m2 ─┬─► m3
    │       ├─► m4
    │       └─► m7-004 (elaborator wiring)
    │
m5  (independent)
m6  (independent)
m7-001..003,005 (independent of m2)
m8  (independent)
m9 closes after m1..m8.
```

Recommended ordering: m1 first (the L). Once m1-006 lands, m2-001 can start. m5 / m6 / m7-001..003 / m8 run in parallel from week 1. m3 + m4 follow m2. m7-004 follows m2-003. m9 wraps.

## 13. Non-goals (explicit)

- **Self-hosting the assembler in `.pdx`** — Phase 4+. Requires m1 / m2 / m3 as preconditions plus a non-trivial port of lexer / parser / elaborator.
- **Borrowed references (`&T`, `&mut T`)** — out of scope (region calculus + borrow-checker pass; multi-milestone).
- **Profile-guided opt-pass ordering** — already C-marked (scope-changed out) in `phase-transition-2.md` §1.
- **Garbage collection** — not on the roadmap; manual + linear is the discipline.
- **PaideiaOS kernel link + QEMU boot test** — paideia-os repo, not here.

## 14. Risks worth naming

- **m1-007 codegen overruns.** Mitigation: stage SIB-encoder unit tests first; small per-width PRs reduce regression blast radius.
- **YubiHSM2 lacks PQ firmware** (m6-002). Mitigation: hybrid-fallback documented (m6-003); ship anyway with the gap surfaced as `Q0902`.
- **m4-006 incremental engine misses < 100 ms.** Mitigation: latency probe first; ship m4 with the probe `#[ignore]`'d and defer to a follow-up if missed.
- **ML-DSA upstream doesn't ship ACVP vectors during Phase 3.** Mitigation: m8-003 documents the upstream link; doesn't block v0.3.0.
- **CI billing doesn't restore.** Mitigation: local `cargo test --workspace` remains the gate (as in Phase 2).

## 15. Open questions for the user

These decisions aren't yet baked in and want a check before m1 kicks off:

1. **Borrowed references genuinely deferred?** Plan defers `&T` / `&mut T` to a future phase. Alternative: bundle with m1 (M→L size hit). **Default: defer.**
2. **Pointer arithmetic surface.** Plan ships only `index_*` and `ptr_sub*`; raw `p + n` not surfaced. Alternative: add `ptr_add : (*T, u64) -> *T`. **Default: don't surface, keep narrow.**
3. **HSM hybrid fallback rule.** When YubiHSM2 lacks PQ firmware, hardware-Ed25519 + soft-HSM-ML-DSA-65 only with explicit opt-in. Alternative: refuse to sign. **Default: explicit opt-in with diagnostic.**
4. **m9 PR-batching experiment.** Try bundling XS doc tasks. Alternative: keep per-PR. **Default: try the batch in m9 only.**

## 16. Closing note

Phase 3 retires every D-row in `phase-transition-2.md` §1 except those formally pushed to Phase 4 (self-hosting, borrowed references). The pointer-enablement milestone (m1) is the load-bearing piece — without it, m2 has no concrete instruction shape to encode, m3 has nothing to rewrite, and the examples corpus stays stuck behind the `Array` placeholder + `unsafe { }` wrappers it has carried since Phase 2 m11.

Phase 3 closes with the v0.3.0 tag + the `phase-transition-3.md` retrospective.
