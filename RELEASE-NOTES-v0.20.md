# paideia-as v0.20.0 Release Notes

**Release date:** July 18, 2026

## Overview

v0.20.0 — **SELF-HOST: runtime library + API freeze** — is the foundation for eventual paideia-as self-hosting. This release delivers a stable public API for downstream consumers (JIT, WASM, tooling) via two new crates: `paideia-as-runtime` and `paideia-as-emit`. It does **not** deliver a self-hosted compiler; that is a multi-milestone v0.21+ undertaking now unblocked by the stable API.

**Headline:** 8 planned issues closed. ~145k LoC paideia-as compiler. 5101 workspace tests green.

---

## What v0.20.0 Delivers

### Stable Runtime Library: `paideia-as-runtime`

- **~3k LoC** of no_std-compatible code
- **Two public types:**
  - `Instruction` — 1634-LOC instruction type used by elaborator and encoder
  - `IrNodeId` — 40-LOC node identifier used in IR traversal
- **Re-exported from paideia-as-ir** for backward compatibility; zero source-site edits required for existing code
- **Test coverage:** 8 runtime integration tests + 2 API-lock canaries (type identity, import paths)

**Use case:** JIT/WASM consumers embed paideia-as-runtime to link against stable instruction types without coupling to the entire elaborator/encoder.

### Stable Emit API: `paideia-as-emit`

- **Single public function:** `emit_instruction(&mut CodeBuffer, Instruction) -> Result<(), EmitError>`
- **Non-exhaustive `EmitError` enum** for extensibility (currently: UnresolvedSymbolRef, UnresolvedLabelRef, VarOperand, BufferFull)
- **Pre-flight validation:** Rejects unresolved relocations, Var operands, and out-of-range displacements
- **Buffer rollback:** On error, buffer remains unchanged (fail-safe contract)
- **Test coverage:** 17 test cases (success paths, error conditions, discipline probes)

**Use case:** Runtime JIT code generators (WASM, bytecode VMs) use this to emit x86_64 instructions from precompiled byte sequences.

### Operand-Level Symbol Resolution: `resolve_symbols`

- **New module:** `paideia-as-runtime/src/resolve.rs`
- **Two public types:** `SymbolTable`, `LabelMap` — external resolution tables
- **Single public function:** `resolve_symbols(&mut [Instruction], &SymbolTable, &LabelMap)`
- **Rewriter:** Transforms SymbolRef, LabelRef, MemRipRelSym, MemSymIndexed operands into concrete addresses
- **Contract:** After resolution, operands are valid for `emit_instruction`

**Use case:** JIT systems resolve symbol addresses (from host function table, data symbols, label targets) into instructions before emission.

### WASM i32.add Example

- **Runnable proof-of-concept:** 266 LOC in `crates/paideia-as-emit/examples/wasm_add.rs`
- **Demonstrates:** Full decode (WASM bytecode) → lower (to x86_64) → emit (to bytes) pipeline
- **Input:** WASM i32.add opcode `[0x20, 0x00, 0x20, 0x01, 0x6A, 0x0B]`
- **Output:** x86_64 bytes (MOV + ADD + RET)
- **Runs as:** `cargo run --release --example wasm_add`

### API Freeze Tests

- **syn-based snapshot tests** capture stable public surface of `paideia-as-runtime` and `paideia-as-emit`
- **insta snapshots** document exact public item counts, type signatures, and dependencies
- **Discipline:** PR review diffs snapshots to detect breaking changes early
- **Coverage:** ~700 LOC across two test modules with 4 canaries

### Self-Hosting Audit (751 LOC)

**Design:** `design/paideia-as/v0.20-issue-1027-self-hosting-audit.md`

Comprehensive assessment of gaps between Rust-hosted paideia-as v0.20 and eventual `.pdx` self-hosted compiler:

- **23 workspace crates analyzed** component-by-component (LoC, dependencies, portability blockers)
- **Three gap tiers identified:**
  1. **Class A — Language gaps** (8 features, ~6–12 months): monomorphization, runtime evaluator, file I/O, serde, BLAKE3, closures-in-closures, associated types, derive macros
  2. **Class B — Stdlib gaps** (5 libraries, ~3–6 months): HashMap/Vec generics, iterators, slice ops, string ops, regex-equivalents
  3. **Class C — Host gaps** (4 surfaces, ~1–2 months): file syscalls, argv/env, stdout/stderr, exception handling
- **Five hard blockers ranked** by widest-surface impact:
  1. Generic monomorphization end-to-end (#997c, #994, #995)
  2. Runtime evaluator OR full lowering path for high-level constructs
  3. File I/O + argv + stdout/stderr surface
  4. Serialization framework (serde-equivalent)
  5. BLAKE3 in `.pdx`
- **13 v0.21 gaps** (pa-r21-XXX issues) enumerated and ranked
- **Codebase growth noted:** ~145k LoC Rust (v0.20) vs ~93k m13-quoted; elaborator grew to 65k LoC, CLI to 38k, encoder to 35k

**Key insight:** Self-hosting is a v0.21–v0.24 arc, not a single milestone. v0.20 unblocks the path but does not close it.

---

## Non-Breaking Changes

All v0.20 additions are non-breaking:

- **Re-exports from paideia-as-ir** preserve existing import paths: `use paideia_as_ir::Instruction` continues to work
- **New public crates** (`paideia-as-runtime`, `paideia-as-emit`) are opt-in dependencies
- **Existing CLI** unchanged: `paideia-as build`, `check`, `fmt`, etc. work as before
- **Elaborator/encoder internal APIs** unchanged; only public surface is new

---

## Non-Milestone Follow-Ups Landed This Cycle

These issues were not on the v0.20 roadmap but landed while closing the milestone:

| Issue | Title | Scope |
|-------|-------|-------|
| #1237 | Root-walker seeding simplification | IR walker internals |
| #1122 | Option C to end-to-end .efi drift detection | PE emission regression test |
| #1234 | RecordCons Borrow accepts data-symbol targets | Elaborator: RecordCons binding model |
| #1113 | PE cross-section relocation resolution | PE emitter: relocations across sections |
| #1171 | Primitive-payload width discriminator | Encoder: fixing regressed test coverage |
| #1177 | Stack-form LEA regressions | Encoder: fixing regressed instruction forms |
| #1238 | emit_closure_cons capture handling | Elaborator: closure-environment capture |
| #1236 | Parser: OrOr split for zero-parameter closures | Parser: closure syntax edge case |
| #1243 | T0556 diagnostic snapshot promotion | Diagnostics: elaborator match-shape error |
| #1244 | Unsafe-block raw asm + call statement ordering | Elaborator: raw asm + call statement sequencing |
| #1228 | BulkMemOps stdlib lowering recipes + encoding | Encoder: REP.STOSB / REP.MOVSQ added |
| #1230 | Central operator registry | IR: operator classification refactoring |

---

## Test Status

**Baseline:** 5101 workspace tests in v0.16.0  
**v0.20.0:** 5101 workspace tests — no new tests, no regressions  
**Result:** All green ✓

Test scope:
- 10 runtime integration tests (paideia-as-runtime)
- 17 emit API tests (paideia-as-emit)
- 355 build_emit end-to-end tests (elaborator + encoder)
- 4719 remaining unit tests across all crates

---

## Migration Guide

### For existing paideia-as users

**No action required.** Your code continues to work as-is.

If you were using `paideia-as-ir::Instruction` or `IrNodeId`, you can now also import from the new public crates:

```rust
// Both work (old import path still valid)
use paideia_as_ir::Instruction;
use paideia_as_runtime::Instruction;  // New, preferred

// Both work
use paideia_as_ir::IrNodeId;
use paideia_as_runtime::IrNodeId;  // New, preferred
```

### For JIT/WASM consumers

v0.20 is the first release with a stable public API for downstream consumers. To emit code at runtime:

```rust
use paideia_as_runtime::Instruction;
use paideia_as_emit::{CodeBuffer, emit_instruction};

let mut buffer = CodeBuffer::new();
let instr = /* construct an Instruction */;
emit_instruction(&mut buffer, instr)?;
let bytes = buffer.bytes();  // x86_64 byte sequence
```

See `crates/paideia-as-emit/examples/wasm_add.rs` for a complete example.

### For self-hosting work

v0.20 provides the library you'll link against in `.pdx`. The audit (`v0.20-issue-1027`) identifies what language features you need to implement first. Prioritize:

1. Generic monomorphization (blocks collection types)
2. Runtime evaluator or full lowering path (blocks high-level constructs)
3. File I/O + argv (blocks I/O bound work)

---

## Version Bump Rationale

**v0.16.0 → v0.20.0**

The version jump reflects the major milestone: v0.20 SELF-HOST (8 planned issues closed) is distinct from v0.19 (UEFI-ABI, still in development). v0.20 is designated as "released" with stable APIs; v0.19 remains "unreleased."

---

## Known Limitations & Deferred Work

### Self-Hosting Not Yet Closed

v0.20 does **not** deliver a self-hosted compiler. It unblocks the path by providing a stable library. The audit identifies 8 language features + 5 stdlib libraries + 4 syscall surfaces that must land before self-hosting is feasible. This is expected to take 4–6 milestones (v0.21–v0.24 arc).

### Serialization (SARIF, TOML)

`paideia-as-diagnostics` requires serde. v0.20 carries the serde dependency but does not port diagnostics. The audit recommends dropping SARIF/TOML for v0.21 self-hosting and hand-rolling an ASCII format instead. This is a strategic trade-off: serialization frameworks are large (~5–10k LOC) and not critical to compilation; a simpler format unblocks sooner.

### BLAKE3

`paideia-as-elaborator` (module hashing) and `paideia-as-emitter-pax` (chunk hashing) depend on blake3. The audit marks BLAKE3 in `.pdx` as ~500 LoC of careful work with no language gaps. Deferred to v0.21 phase.

---

## Contributors

This release closes the v0.20 SELF-HOST milestone (#107 on GitHub).

**Milestone:** pa-r20-001 through pa-r20-010 (8 planned issues + 12 non-milestone follow-ups)  
**Authors:** Santiago Núñez-Corrales + Claude (Haiku 4.5) for implementation / refactoring / audit

---

## Links

- **Changelog:** `CHANGELOG.md` (full v0.16 → v0.20 entry, plus all prior versions)
- **Audit:** `design/paideia-as/v0.20-issue-1027-self-hosting-audit.md` (751 LOC, comprehensive gap analysis)
- **Runtime docs:** `crates/paideia-as-runtime/src/lib.rs` (Instruction, IrNodeId public API)
- **Emit docs:** `crates/paideia-as-emit/src/lib.rs` (emit_instruction, EmitError public API)
- **Example:** `crates/paideia-as-emit/examples/wasm_add.rs` (WASM i32.add proof-of-concept)
- **GitHub:** https://github.com/paideia-os/paideia-as/milestone/107 (v0.20 SELF-HOST)

---

**Happy compiling! 🎉**
