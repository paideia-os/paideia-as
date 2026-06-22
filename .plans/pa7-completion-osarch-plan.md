# paideia-as PA7-Completion Round — Cross-repo unblock for PaideiaOS R1.5–R6.5 (osarch)

**Author:** osarch agent
**Date:** 2026-06-22
**Repo:** `paideia-os/paideia-as` (workspace at `/home/snunez/Development/paideia-as/`)
**Scope:** Close the byte-emission and surface gaps that PaideiaOS R1.5–R6.5 integration surfaced during PA7-completion testing, so that the 13 currently quarantined `.pdx` files in `PaideiaOS/.quarantine/src/kernel/` return to `src/kernel/` and the kernel builds + boots without `tools/stubs.S` workarounds.
**Companion:** `.plans/phase-6-plan.md` (Phase 6 closed at v0.6.0); the PA7 round (commits `09a9a54`..`5dd9a1f`, PRs through #796) shipped parser + IR + label-registration for the PA7 surface but byte emission for `unsafe`-block bodies and cross-file `call sym` is partial. This round is the closure.

---

## 0. Scope decision

Three scope options are available:

- **(A) Minimum — G1, G2, G3 only.** The three pure PA7-completion bugs: top-level symbol export, PLT32 reloc offset, and unsafe-block-body statement bridging. Unblocks `kernel_main.pdx`, the two interrupt files, and `mm/pt_walk.pdx` — i.e., 4 of 13 quarantined files. Round size ~9–11 issues.
- **(B) Recommended — G1 through G10.** Adds the parser/lexer papercuts surfaced during integration: unary bitwise NOT prefix (`~`), the `handle` keyword conflict, redundant `->` before block bodies, sized-integer plumbing through encoding, the `as` cast operator, side-effect blocks with no trailing tail, and array-index l-values. Unblocks all 13 quarantined files. Round size ~18–22 issues.
- **(C) Full — G1 through G15.** Adds anticipated needs catalogued during R1.5–R6.5 work but not blocking any currently-quarantined file: general `mov [base+disp]` memory operand, array-literal initialisers for `mut` arrays, real string-literal surface, multiboot2 / PVH ELF Note emission, and the long-mode supervisor-surface bridge fan-out (G11). Round size ~28–34 issues — too large for one round and several items are speculative.

**Decision: option (B).** Justification:

1. (B) is the smallest scope that fully drains the quarantine. Stopping at (A) leaves the slab, IPC, IPI, and scheduler files quarantined — and the source quarantined for those files was authored *during* this session against the PA7 surface; a partial unquarantine forces a second integration sweep before R7 starts.
2. The G4–G10 fixes are surface-level (parser + lowering), not encoder; each is XS or low-S, and their tests are local to the parser/elaborator crates. Adding them to the round costs ~10 issues and ~1.5 milestones of work — well below the line where round-splitting becomes the cheaper option.
3. G11–G15 are deferred to a Phase 7 round (`phase-7-osarch-plan.md`) because:
   - **G11 (long-mode supervisor bridge fan-out)** is mostly already-shipped encoders (verified: `Mnemonic::Cli`/`Hlt`/`Wrmsr`/`Iretq`/`Lgdt`/`Lidt` all have encoders at `crates/paideia-as-encoder/src/encode_instruction.rs:219..235`). The bridge work is G3-shaped — once G3 lands, the supervisor mnemonics ride for free. Listing them as separate G11 issues now would duplicate G3's tests.
   - **G12 (general `mov [base+disp]`)** is not blocked by any quarantined file. The slab, IPC, sched, and IPI source paths use `(*p).field` field access (which lands through the Phase 6 m3 struct walker) and `.bss` array indexing (Phase 6 m5). The general base+disp form is needed for R7+ work, not now.
   - **G13 (`.data` array initialisers)** — confirmed: no quarantined file uses initialised `mut` arrays. They all declare uninit `.bss` arrays and fill them at boot.
   - **G14 (real string literals)** — the banner workaround (packed `[u8; 64]`) works; no quarantined file is gated on real string literals.
   - **G15 (multiboot2 / PVH Note)** — the `-device loader,addr=0x100000` QEMU workaround is stable. Real multiboot2 is a v0.8.0 concern.

**Rejected alternative:** (A) with deferral of G4–G10 to its own follow-on round. Rejected because that scheme creates *two* cross-repo integration sweeps in the same calendar window; the value of one larger round is that the PaideiaOS resume happens once.

---

## 1. Scope boundary (what is NOT in scope)

- **G11 supervisor-surface bridge fan-out as separate issues.** G3 lands the EmitWalker → encoder bridge for the canonical asm-statement shapes inside `unsafe`. The bridge dispatches on `Mnemonic` and routes to the per-mnemonic encoder; supervisor mnemonics whose encoders already exist (Cli, Sti, Hlt, Wrmsr, Rdmsr, Iretq, Iret, Sysret, Lgdt, Lidt, MovCr, MovDr) get coverage automatically. G3's acceptance criteria explicitly list these mnemonics in the round-trip table.
- **G12 / G13 / G14 / G15** per §0.
- **Self-hosting.** Still Phase 7 per `phase-6-decision-gate-g8.md`.
- **PE/COFF + PAX activation parity.** Both consume `InstructionSideTable` via the shared encoder bridge; G1–G3 changes propagate automatically as in Phase 5 / 6.
- **paideia-as-linker integration.** PaideiaOS continues to invoke GNU `ld` via `tools/build.sh`.
- **Multi-file build orchestration.** `paideia-as build a.pdx b.pdx -o linked.elf` remains Phase 7. PaideiaOS continues to glue per-file objects via the build script.
- **DWARF emission.** `paideia-as-dwarf` crate exists; wiring into `--emit elf64` stays Phase 7+.
- **Optimisation passes in the build path.** Peephole / DSE / encode-tight / unroll remain side-table-level; build-emit activation stays Phase 7+. Kernel hot paths (IPC dispatch, scheduler enqueue) run unoptimised in this round — the verifier is ~6 instructions, dispatch is ~12.
- **Atomic ops + LL/SC primitives.** R5.5 scheduler uses `mut` globals with single-CPU serial assumption; atomic activation (`lock cmpxchg`, fenced loads/stores) is Phase 7+ when SMP bring-up begins.
- **Effect-handler runtime materialisation.** Capability and IPC sources declare effects (`!{sysreg}`, `!{ipc}`) but they are checked-not-enforced at emit; full handler dispatch lands when R8 (user-mode round) needs effect-row signatures across the kernel/user boundary.
- **Generic walker-chain activation for the post-PA7 surface.** Generics, traits, enums, borrowed references, region calculus, stdlib types — these landed at side-table level in Phase 6 and stay there. Their build-emit activation is Phase 7+.

This round is the smallest disciplined sequence that drains the PaideiaOS quarantine and produces a kernel.elf that boots to UART without `tools/stubs.S`.

---

## 2. Milestone index

Six milestones, ~21 issues. Mean size **S**. No `L` tasks; the largest cluster is m1 (the three byte-emission bugs) which is decomposed per-bug-class. The round closes when m6 (end-to-end cap-to-uart smoke) goes green and the PaideiaOS build script drops `tools/stubs.S`.

| #  | Milestone slug                         | Description                                                              | Issues | Critical path |
|----|----------------------------------------|--------------------------------------------------------------------------|--------|---------------|
| m1 | `pa7c-symbol-and-reloc`                | G1 (real binding-named symbols) + G2 (PLT32 reloc offset unification).   | 4      | yes           |
| m2 | `pa7c-unsafe-body-bridge`              | G3 (EmitWalker → encoder dispatch for asm statements inside `unsafe`).   | 4      | yes           |
| m3 | `pa7c-parser-papercuts`                | G5 (handle keyword), G6 (optional `->`), G9 (unit-typed block tail).     | 3      | partial       |
| m4 | `pa7c-expression-surface`              | G4 (`~` prefix), G8 (`as` cast), G7 (sized-int plumbing).                | 4      | partial       |
| m5 | `pa7c-lvalue-surface`                  | G10 (array-index l-value + pointer-deref l-value).                       | 2      | yes           |
| m6 | `pa7c-end-to-end-smoke`                | boot_orchestration_v2 fixture + PaideiaOS unquarantine + QEMU smoke.     | 4      | yes (closure) |
|    | **Σ**                                  |                                                                          | **21** |               |

**Critical path** (longest dependency chain through the milestones):

`m1-001 → m1-002 → m1-003 → m2-001 → m2-002 → m2-003 → m5-001 → m6-001 → m6-002 → m6-003 → m6-004` = **11 issues**.

**PaideiaOS unquarantine gate:** **m6 close (m6-004)**. All 13 quarantined `.pdx` files re-build cleanly, link without `tools/stubs.S`, and the resulting `kernel.elf` produces a UART banner under `qemu-system-x86_64`. The m6-004 commit message declares the unblock explicitly, mirroring the Phase-5 m6-005 and Phase-6 m6-004 markers.

Parallelisable sub-tracks:

- **m1** (symbol export + reloc offset) is independent of the parser/lvalue work in m3/m4/m5. Within m1, m1-001 + m1-002 (symbol export) are independent of m1-003 (PLT32 offset).
- **m3 (parser papercuts)** is fully independent of m1/m2. The three issues are independent of each other and can land in any order.
- **m4 (expression surface)** depends only on the existing IR types (`IrKind::UnaryOp`, `IrKind::Cast`, `IntWidth`) which already exist in `crates/paideia-as-ir/src/`. m4-001 (G4) depends on a new lexer token; m4-002 (G8) depends on parser precedence work; m4-003 (G7) depends on existing AST + encoder. Internally parallelisable.
- **m5** depends on m3-003 (G9) closing first (l-value parsing reuses the block-trail synthesis hook) but is otherwise independent of m1/m2.
- **m6** depends on all of m1–m5.

---

## 3. Milestone m1 — Symbol export + PLT32 reloc offset (G1 + G2)

**Slug:** `pa7c-symbol-and-reloc`
**Issues:** 4
**Governing docs:** `crates/paideia-as/src/cmd_build.rs:786..788` (the synthetic-symbol fallback that produces every `.o`'s lone `add_one` STT_FUNC); `crates/paideia-as-encoder/src/encode_instruction.rs:600..621` (the `call sym` operand arm that records `byte_offset: reloc_offset + 1`); `crates/paideia-as-elaborator/src/emit_walker.rs:937..938` (the parallel `current_offset += 5` tick that drifts from `buf.bytes.len()`).

The synthetic-symbol fallback was a Phase-5 expedient: when the build path had no real `SymbolTable` walk, it emitted one `STT_FUNC` named `add_one` of size = the entire `.text` so that the cross-repo `add_one` byte-identical regression test could pass. Phase 5 m5 added the real `SymbolTable` walk above the fallback (lines 730..781), but the fallback was left in place as a tail-guard. Phase 6 then began emitting multiple top-level functions per `.o`, and the fallback became actively harmful: when the real walk finds zero exported names (because the binding's name was never threaded into the `SymbolKind::Function` entry — see m1-001 below) the fallback fires and the file ships with only `add_one`.

The PLT32 bug is structurally similar — two counters maintained in two crates that have drifted apart since they were introduced in Phase 5 m5 (encoder side) and Phase 7 m1-001 / PA7 m6-002 (walker side). The encoder's `byte_offset: reloc_offset + 1` computes against `buf.bytes.len()` which is the canonical byte position inside the encoder's output buffer; the walker's `self.state.current_offset += 5` computes against a synthetic per-walker counter. When `emit_function_call` (walker) inserts an `Instruction` into the side-table *but the actual encoder runs later in `cmd_build`*, the offsets match. When `emit_function_call` and the encoder run interleaved (the PA7 multi-stmt unsafe path), the counters drift by exactly one byte per call.

---

### m1-001. cmd_build: walk the SymbolTable using binding names, kill the `add_one` fallback

- **Summary:** In `cmd_build.rs`, the `SymbolKind::Function` arm of the symbol-emission walk needs to be reached for every top-level `let f : T = fn (...) -> ...`. Today the walk happens, but the IR's `SymbolEntry` for each top-level let-fn is populated with the synthetic name used during elaboration (`add_one` or similar), not the actual binding name from the let-binder. The fix is two-part: (a) populate `SymbolEntry::name` from the binding name at IR construction time in `crates/paideia-as-elaborator/src/lower.rs`; (b) delete the synthetic-`add_one` fallback at `cmd_build.rs:786..788`. After both changes, `readelf -s build/boot/uart.o` shows one `STT_FUNC` per top-level let-fn with the binding's actual name.
- **Acceptance criteria:** task closed when
  - Every top-level `let NAME : T = fn (...) -> BODY` produces exactly one `SymbolEntry { name: "NAME", kind: SymbolKind::Function, st_value: offset_in_text, st_size: encoded_body_size }`.
  - The fallback branch at `cmd_build.rs:786..788` is removed (the `emitted_any_symbol` flag stays as an invariant check but transitions from "synthesize a placeholder" to "fail the build with B0007: no exported symbols").
  - New diagnostic `B0007` ("ELF output has no exported symbols; check that at least one top-level binding is `pub` or that the file is not empty") is added to `crates/paideia-as-diagnostics/catalog.toml`.
  - Integration test `crates/paideia-as/tests/build_emit_pa7c_symbol_export.rs` builds a 3-function source (`uart_init`, `uart_putc`, `uart_puts`) and asserts that the resulting `.o`'s symbol table contains exactly those three names (via the `object` crate's ELF reader) with non-overlapping `(st_value, st_size)` ranges that cover the full `.text`.
  - Regression: the four pre-existing tests that assumed the `add_one` fallback (search: `grep -rn '"add_one"' crates/paideia-as/tests/`) are rewritten to use the binding name from their fixture.
  - The PaideiaOS `tools/build.sh` no longer requires `tools/stubs.S` to satisfy `uart_init`, `cap_mint`, or `kernel_main_64` external references — verified by deleting `stubs.S` from the link line for this issue's regression test and showing `ld` exits 0.
- **Files:** `crates/paideia-as-elaborator/src/lower.rs` (symbol-name propagation), `crates/paideia-as/src/cmd_build.rs` (kill fallback, add B0007), `crates/paideia-as-diagnostics/catalog.toml`, `crates/paideia-as/tests/build_emit_pa7c_symbol_export.rs`, plus rewriting four existing tests.
- **Dependencies:** none.
- **Estimated size:** S
- **Phase:** PA7-completion round.
- **Milestone:** `pa7c-symbol-and-reloc`.
- **Cross-repo unblock:** All 13 PaideiaOS quarantined files (every cross-file `call sym` resolves against a real symbol).

---

### m1-002. emitter-elf: assert symbol-name uniqueness + non-overlapping ranges

- **Summary:** A defensive check that catches a future regression of m1-001. Before `finalize()` emits the symbol-table section, the `paideia-as-emitter-elf` writer asserts (a) every `STT_FUNC` / `STT_OBJECT` symbol's `(st_value, st_value+st_size)` range lies entirely inside the section it belongs to; (b) no two symbols' ranges overlap; (c) symbol names are unique within the file. Violations are returned as `EmitterError::SymbolLayoutInvalid { kind, names }` and propagated to `cmd_build` as an internal-error build failure (exit 70, distinct from user diagnostic exit 1 and encoder-failure exit 2).
- **Acceptance criteria:**
  - `paideia-as-emitter-elf::ElfWriter::finalize` runs the three checks before writing the symbol table.
  - Synthetic test: construct a writer with two `SymbolEntry::func("foo", 0, 16)` and assert `Err(EmitterError::SymbolLayoutInvalid { kind: DuplicateName, names: ["foo"] })`.
  - Synthetic test: construct with `("foo", 0, 16)` and `("bar", 8, 16)` and assert `Err(SymbolLayoutInvalid { kind: OverlappingRanges, names: ["foo", "bar"] })`.
  - Synthetic test: construct with `("foo", 100, 16)` against a 64-byte `.text` and assert `Err(SymbolLayoutInvalid { kind: OutOfBounds, names: ["foo"] })`.
  - `cmd_build` catches `EmitterError::SymbolLayoutInvalid` and reports it as `B0008` ("emitter rejected symbol layout — this is a paideia-as internal error; please file a bug") with exit code 70.
  - Three unit tests inside `paideia-as-emitter-elf::tests::symbol_layout`.
- **Files:** `crates/paideia-as-emitter-elf/src/writer.rs`, `crates/paideia-as-emitter-elf/src/lib.rs` (new error variant), `crates/paideia-as/src/cmd_build.rs`, `crates/paideia-as-diagnostics/catalog.toml`.
- **Dependencies:** m1-001 (the layout invariants are meaningful only once real names ship).
- **Estimated size:** XS
- **Phase:** PA7-completion round.
- **Milestone:** `pa7c-symbol-and-reloc`.

---

### m1-003. encoder + walker: collapse byte-position counters into a single source of truth (G2)

- **Summary:** Today byte position is tracked in two places: (a) `CodeBuffer::bytes.len()` inside the encoder (`encode_instruction.rs`); (b) `self.state.current_offset: u32` inside the `EmitWalker` (`emit_walker.rs:35`, bumped at lines 505, 794, 805, 829, 840, 938, 948, 976, 997, 1031, 1042, 1118, 1273, 1302, 1331, 1499, 1532, 1565, 1693, 1716, 1771, 1793 — 22 disparate bump sites). The `RelocSite::byte_offset` field is computed against (a) at line 606 of `encode_instruction.rs` (`buf.bytes.len() as u32` + 1), but downstream consumers (the cross-section reloc adjustment in `cmd_build.rs:790..802`) cross-reference against (b)'s offsets stored in `data_table` / `symbol_table`. The two counters agree for the Phase-5/6 closed surface (one encoder run per build, sequential emission) but drift on the PA7 multi-stmt unsafe path where `emit_function_call` (walker) inserts an `Instruction` and the encoder runs later: the walker's `+= 5` and the encoder's `buf.bytes.len()` can be off by one byte when an alignment pad or a different-sized prior instruction sits between them. **Fix (option a — encoder owns byte tracking):** the `InstructionSideTable` entry gains an `byte_offset_in_text: Option<u32>` slot that is populated during encoding (not during walking). All reloc-offset arithmetic reads from this slot. The walker's `current_offset` becomes an *advisory* estimate used only for label resolution during the side-table phase; once encoding produces real offsets, label addresses are reconciled in a single pass before the reloc table is written.
- **Acceptance criteria:**
  - `InstructionSideTable` (in `crates/paideia-as-ir/src/`) gains `pub byte_offset_in_text: Option<u32>` on each entry, populated by `cmd_build` during the encode pass.
  - `RelocSite::byte_offset` is computed as `instruction.byte_offset_in_text.unwrap() + 1` for `call sym` (the +1 reflects the `E8` opcode byte; this stays in the encoder).
  - The `EmitWalker::state::current_offset` field is renamed to `estimated_offset` and gains a doc comment marking it as a label-resolution estimate, not authoritative; an `assert_eq!` at the end of the build path checks `estimated_offset == buf.bytes.len()` and emits `B0009` ("walker byte estimate diverged from encoder reality") with the divergent values if not.
  - **Regression fixture:** a `.pdx` file with four `unsafe` blocks each containing a `call uart_putc` interleaved with `mov` and `out` instructions. The resulting `.o` is loaded via the `object` crate; every `R_X86_64_PLT32` reloc's `r_offset` is verified to point at the byte immediately after a `0xE8` (i.e., the rel32 placeholder).
  - **PaideiaOS regression:** the four files listed by the gap analysis (`.quarantine/src/kernel/boot/kernel_main.pdx`, `.quarantine/src/kernel/core/int/exceptions.pdx`, `.quarantine/src/kernel/core/int/idt.pdx`, `.quarantine/src/kernel/core/mm/pt_walk.pdx`) re-build cleanly and `ld` accepts the resulting `.o`'s without `error 4`. Verified by a shell driver under `tools/run-pa7c-reloc-regression.sh` that copies the four files from `.quarantine/` to a tmp build dir, runs the build, runs `ld`, asserts exit 0, and runs `readelf -r` to confirm every PLT32 offset points to a position immediately after an `E8`.
  - 6 unit tests inside `crates/paideia-as-elaborator/tests/emit_walker/byte_offset.rs` covering the interleave patterns: call+mov+call, mov+call+mov, call+call (two adjacent), nested-unsafe call, call inside a loop, call inside a match arm.
- **Files:** `crates/paideia-as-ir/src/instruction.rs` (add `byte_offset_in_text` field), `crates/paideia-as/src/cmd_build.rs` (populate the field during encode pass), `crates/paideia-as-encoder/src/encode_instruction.rs` (read from instruction, not buffer, for reloc offsets), `crates/paideia-as-elaborator/src/emit_walker.rs` (rename + assert), `crates/paideia-as-diagnostics/catalog.toml` (B0009), `tools/run-pa7c-reloc-regression.sh`, `crates/paideia-as-elaborator/tests/emit_walker/byte_offset.rs`.
- **Dependencies:** none directly, but co-ordinate with m1-001 to share the cmd_build pass restructure.
- **Estimated size:** M (the largest issue in the round; the byte-tracking refactor touches 22 bump sites)
- **Phase:** PA7-completion round.
- **Milestone:** `pa7c-symbol-and-reloc`.
- **Cross-repo unblock:** `kernel_main.pdx`, `int/exceptions.pdx`, `int/idt.pdx`, `mm/pt_walk.pdx`.

---

### m1-004. tests: PLT32 round-trip via `iced-x86` disassembly + `ld` rejection witness

- **Summary:** A standalone test crate that, for each instruction shape the PA7 surface emits with a PLT32 reloc, (a) builds the `.o` via `cmd_build`, (b) loads it via the `object` crate, (c) for every relocation, disassembles the surrounding bytes via `iced-x86` and confirms the reloc offset lands inside a `call rel32` immediate (not on the opcode, not past the end), (d) constructs a minimal companion `.o` declaring the target symbol, links the pair via `ld -r`, and asserts exit 0. This is the canonical witness that G2 cannot regress.
- **Acceptance criteria:**
  - New test file `crates/paideia-as/tests/build_emit_pa7c_plt32_witness.rs`.
  - Covers ≥ 8 instruction shapes: bare `call sym`, `call sym` inside `unsafe`, two adjacent calls, `mov; call; mov`, `call; ret`, `call` inside loop body, `call` inside match arm, `call` inside if/else.
  - For each shape, asserts: (a) reloc offset is a multiple of 1 byte after an `E8`; (b) `iced-x86` disassembles the `call` correctly with `target = 0` (relocation not yet resolved); (c) `ld -r partner.o subject.o -o linked.o` exits 0.
  - Test is gated on `ld` being present (skipped on macOS/Windows CI lanes that lack GNU binutils); the gate is implemented identically to the existing QEMU-availability gate in `tests/build_emit_uart_smoke.rs`.
- **Files:** `crates/paideia-as/tests/build_emit_pa7c_plt32_witness.rs`, `crates/paideia-as/tests/fixtures/pa7c_plt32/*.pdx` (8 fixtures), `crates/paideia-as/tests/fixtures/pa7c_plt32/partner.S` (the hand-written companion).
- **Dependencies:** m1-003.
- **Estimated size:** S
- **Phase:** PA7-completion round.
- **Milestone:** `pa7c-symbol-and-reloc`.

---

## 4. Milestone m2 — Unsafe-block body bridging (G3)

**Slug:** `pa7c-unsafe-body-bridge`
**Issues:** 4
**Governing docs:** `crates/paideia-as-elaborator/src/emit_walker.rs:1051..1119` (the `emit_block_body` placeholder that recognises `IrKind::Let` and `IrKind::Action` but emits no actual instructions — confirmed by reading the function body, which contains comments `// TODO: Emit the value expression to scratch_reg.` and `// TODO: Emit the expression, discard result.`); the per-mnemonic encoders at `encode_instruction.rs` (verified: Cli at line 767, Hlt at 791, Wrmsr at 907, Iretq at 1158, Lgdt at 1041, Lidt at 1093, plus the `MovCr`/`MovDr` dispatch from Phase-6 m1).

The PA7 round shipped the *outer* unsafe-walker work (`unsafe_walker.rs::process_stmt_instruction` recognises mnemonic names and constructs `Instruction` records). What's missing is the *inner* bridge inside `EmitWalker::emit_block_body`: when an `unsafe { stmt1; stmt2; stmt3 }` block appears inside a lambda body, the walker recognises the block shape and the unsafe-wrapper but then drops each statement through the `IrKind::Let` / `IrKind::Action` arms as a TODO. The result is the symptom in the gap report: 21 bytes of `48 89 c0` (`mov rax, rax`) instead of the real `B0 80 ; 66 BA FB 03 ; EE` sequence.

The fix is to make `emit_block_body` recognise the canonical asm-statement shapes (the same ones `process_stmt_instruction` already lowers in the outer walker) and dispatch each to the right per-mnemonic encoder via a single Instruction insert into the `InstructionSideTable`. The encoders for `in`, `out`, `mov imm`, `mov reg-reg`, plus the long-mode supervisor surface, all already exist; the missing piece is the `IrKind::Action(IrKind::RawInstruction { ... })` recognition in the inner walker.

---

### m2-001. emit_walker: recognise `IrKind::RawInstruction` inside `Action` and forward to side-table

- **Summary:** In `emit_block_body`, the `IrKind::Action` arm currently has `// TODO: Emit the expression, discard result.` The fix is to inspect the `Action`'s single child node. If it is `IrKind::RawInstruction { mnemonic, operands, encoding_hint }` (already populated by the outer `UnsafeWalker` per Phase-5 m3), forward the instruction to `self.state.instructions.insert(action_id, Instruction { mnemonic, operands, encoding_hint })` and bump `current_offset` by the encoder's `estimated_size_for(mnemonic, operands)`. If it is any other `IrKind`, fall through to the existing TODO (preserving Phase-7 expansion room).
- **Acceptance criteria:**
  - `emit_block_body`'s `IrKind::Action` arm inspects the child node and, on `IrKind::RawInstruction`, inserts an `Instruction` into the side-table.
  - A new helper `Mnemonic::estimated_size(operands: &[Operand]) -> u8` returns the conservative upper-bound size in bytes for the byte-position estimate (matches the `+= N` constants currently scattered through `emit_walker.rs`).
  - Regression fixture: `tests/build-emit/pa7c_unsafe_body_outb.pdx` containing `let uart_init : *u8 -> () = fn (base) -> unsafe { mov al, 0x80; mov dx, 0x3FB; out dx, al }` builds to a `.text` containing the byte sequence `B0 80 66 BA FB 03 00 00 EE C3` (mov al,0x80; mov dx,0x3FB; out dx,al; ret).
  - All 7 fixtures under `tests/build-emit/pa7c_unsafe_body_*.pdx` (one per canonical asm pattern: outb, inb, mov-reg-imm, mov-reg-reg, cli/sti, hlt+loop, the long-mode CR sequence) build with byte-exact `.text` matching iced-x86 disassembly.
  - 5 unit tests inside `crates/paideia-as-elaborator/tests/emit_walker/unsafe_body.rs`.
- **Files:** `crates/paideia-as-elaborator/src/emit_walker.rs`, `crates/paideia-as-ir/src/instruction.rs` (add `Mnemonic::estimated_size`), 7 new `.pdx` fixtures, `crates/paideia-as-elaborator/tests/emit_walker/unsafe_body.rs`.
- **Dependencies:** none (the IR is already populated; only the walker arm needs to act).
- **Estimated size:** M
- **Phase:** PA7-completion round.
- **Milestone:** `pa7c-unsafe-body-bridge`.
- **Cross-repo unblock:** `kernel_main.pdx`, `int/exceptions.pdx`, `int/idt.pdx`, `mm/pt_walk.pdx` — the same set as m1-003, in combination.

---

### m2-002. emit_walker: recognise `IrKind::Let` with `RawInstruction` RHS + propagate dest reg

- **Summary:** Sister of m2-001 for the `IrKind::Let` arm. Today the `Let` arm allocates a scratch reg from `[RAX, RCX, RDX, R8]` and then has a TODO to emit the value expression. The fix: inspect the `Let`'s RHS child. If it is `IrKind::RawInstruction { mnemonic: Mov, operands: [Imm64(n)] }`, the let-binding's name maps to the scratch reg and an `Instruction { mnemonic: Mov, operands: [Reg(scratch_reg), Imm64(n)] }` lands in the side-table. This handles the `let base = 0x3F8; let lcr = base + 3; mov dx, lcr` pattern that appears in the UART init code.
- **Acceptance criteria:**
  - The `Let` arm at `emit_walker.rs:1066..1092` inspects the RHS and on `IrKind::RawInstruction { mnemonic: Mov, operands: [Imm64(n)] }` emits `Instruction { Mov, [Reg(scratch_reg), Imm64(n)] }`.
  - On `IrKind::RawInstruction` with a non-`Mov` mnemonic or non-Imm64 operand, emit the new diagnostic `U1612` ("let-binding RHS in unsafe block must be a `mov imm` form for register allocation") and skip the instruction.
  - The `state.scratch_assignment` Vec is extended to record `(IrNodeId, RegId)` pairs so that later instructions in the block that reference the binding name (via `IrKind::Var`) can resolve to the same scratch reg.
  - Regression: `tests/build-emit/pa7c_unsafe_body_let_scratch.pdx` with three sequential let-bindings + a four-operand `mov` chain emits the expected byte sequence.
  - 3 unit tests covering: single let, three-let chain, register-pressure-exceeded (4+ in-flight bindings) emits T0517.
- **Files:** `crates/paideia-as-elaborator/src/emit_walker.rs`, `crates/paideia-as-diagnostics/catalog.toml` (U1612), 1 new fixture, `crates/paideia-as-elaborator/tests/emit_walker/unsafe_body_let.rs`.
- **Dependencies:** m2-001 (shares the `RawInstruction` recognition path).
- **Estimated size:** S
- **Phase:** PA7-completion round.
- **Milestone:** `pa7c-unsafe-body-bridge`.

---

### m2-003. emit_walker: `Var(name)` inside RawInstruction operands resolves to scratch reg

- **Summary:** When an unsafe-block instruction's operand list contains a bare identifier (e.g., `mov dx, lcr` where `lcr` was a prior `let`), the `RawInstruction`'s `Operand::Var(name)` needs to resolve to the scratch reg allocated in m2-002. The fix lives in the operand-translation hook called from the side-table-to-encoder bridge (`cmd_build.rs`'s encode pass): before forwarding to `encode_instruction`, walk the operand list and replace each `Operand::Var(name)` with `Operand::Reg(scratch_for(name))`. Unresolved names emit `U1613` and skip the instruction.
- **Acceptance criteria:**
  - A new operand-translation pass in `cmd_build` (or `encoder/dispatch.rs` if the m1-001 refactor moved dispatch there) replaces `Operand::Var(name)` with `Operand::Reg(scratch_for(name))` using the `(name, scratch_reg)` map populated by m2-002.
  - Unresolved names emit `U1613` ("unresolved identifier `{name}` in unsafe-block operand").
  - Regression: `tests/build-emit/pa7c_unsafe_body_var_resolve.pdx` containing `unsafe { let lcr = 0x3FB; mov dx, lcr; out dx, al }` emits `BA FB 03 00 00 EE`.
  - 2 unit tests: resolved-var and unresolved-var (U1613).
- **Files:** `crates/paideia-as/src/cmd_build.rs` or `crates/paideia-as-encoder/src/dispatch.rs`, `crates/paideia-as-diagnostics/catalog.toml` (U1613), 1 new fixture, `crates/paideia-as-elaborator/tests/emit_walker/unsafe_body_var.rs`.
- **Dependencies:** m2-001, m2-002.
- **Estimated size:** S
- **Phase:** PA7-completion round.
- **Milestone:** `pa7c-unsafe-body-bridge`.

---

### m2-004. tests: PaideiaOS R1.5/R2.5 four-file re-build regression suite

- **Summary:** The cross-repo canary for m1 + m2. A Rust integration test that, if PaideiaOS is present at `../../PaideiaOS` (or `PAIDEIA_OS_PATH`), copies the four files from the gap-list (`kernel_main.pdx`, `int/exceptions.pdx`, `int/idt.pdx`, `mm/pt_walk.pdx`) into a temp dir, runs `cmd_build` on each, links the resulting `.o`'s via GNU `ld` (using a minimal link script under `crates/paideia-as/tests/fixtures/pa7c_link.ld`), and asserts exit 0 + non-empty `.text`. This is the canonical witness that the four files leave quarantine cleanly.
- **Acceptance criteria:**
  - `crates/paideia-as/tests/paideia_os_r1_5_r2_5_rebuild.rs` discovers PaideiaOS or skips with `println!("PaideiaOS not present; skipping")`.
  - Builds the 4 files; asserts each `.o` has ≥ 1 STT_FUNC symbol whose name matches the binding name and a non-empty `.text`.
  - Links the 4 `.o`'s + a hand-written `stub_partner.S` (provides any still-unresolved symbols, expected to be empty after m1) via `ld -e _start`; asserts exit 0.
  - The four files are explicitly named in the test source with `// gap: G2` / `// gap: G3` comments so a future reader can trace the regression.
- **Files:** `crates/paideia-as/tests/paideia_os_r1_5_r2_5_rebuild.rs`, `crates/paideia-as/tests/fixtures/pa7c_link.ld`, `crates/paideia-as/tests/fixtures/stub_partner.S`.
- **Dependencies:** m1-001, m1-003, m2-001, m2-002, m2-003.
- **Estimated size:** XS
- **Phase:** PA7-completion round.
- **Milestone:** `pa7c-unsafe-body-bridge`.
- **Cross-repo unblock:** `kernel_main.pdx`, `int/exceptions.pdx`, `int/idt.pdx`, `mm/pt_walk.pdx` (composite — all four need both m1 and m2 to land).

---

## 5. Milestone m3 — Parser papercuts (G5 + G6 + G9)

**Slug:** `pa7c-parser-papercuts`
**Issues:** 3
**Governing docs:** `crates/paideia-as-lexer/src/token.rs:143,415,507` (the `KwHandle` keyword that's reserved but unused); `crates/paideia-as-parser/src/parse_control.rs:224..303` (the P0158 emit site for blocks without a final expression); the grammar production for fn-literal bodies in `crates/paideia-as-parser/src/parse_handler.rs`.

These three are small, independent, and surfaced when the slab/IPC source was being authored against PA7. They all have a low risk of regression because each touches one grammar rule.

---

### m3-001. lexer: free `handle` as a user identifier (G5)

- **Summary:** Today `handle` is in the keyword list (lexer/token.rs lines 143, 415, 507) but the PA7 surface does not use it as a keyword. PaideiaOS `cap_handle.pdx` and `cap_invoke.pdx` need it as a parameter name. **Decision:** remove `handle` from the keyword list (i.e., delete the three entries referenced above plus the `KwHandle` variant from the `TokenKind` enum). If we want to reserve it for a future feature, the right time to add it back is when that feature lands — adding a keyword is a one-line PR; preemptively reserving names taxes every user today for a hypothetical future. Documented rationale lives in `design/toolchain/reserved-word-policy.md` (created here, ~40 lines).
- **Acceptance criteria:**
  - `TokenKind::KwHandle` is removed from `crates/paideia-as-lexer/src/token.rs`.
  - The three occurrences (variant declaration line 143, keyword_kind arm line 415, reserved-words list line 507) are deleted.
  - The reserved-words test `every_reserved_word_resolves` at line 578 still passes (the list shrunk from 69 to 68 spellings).
  - A new positive test `handle_lexes_as_identifier` asserts that `let handle = 42` lexes the second token as `Ident("handle")`.
  - The new doc `design/toolchain/reserved-word-policy.md` documents the policy: keywords are added when the feature ships, not preemptively.
  - The PaideiaOS file `.quarantine/src/kernel/core/cap/handle.pdx` (if present — the gap report references `cap_handle.pdx` and `cap_invoke.pdx`; the actual files are not in the quarantine list, so this AC is verified via a synthetic fixture that uses `handle` as a parameter name in a fn-literal).
- **Files:** `crates/paideia-as-lexer/src/token.rs`, `crates/paideia-as-lexer/tests/handle_identifier.rs` (new), `design/toolchain/reserved-word-policy.md` (new).
- **Dependencies:** none.
- **Estimated size:** XS
- **Phase:** PA7-completion round.
- **Milestone:** `pa7c-parser-papercuts`.

---

### m3-002. parser: make `->` optional before `{ … }` body in fn-literal grammar (G6)

- **Summary:** `fn () { stmt; expr }` rejects today; `fn () -> { stmt; expr }` parses. The `->` is redundant when the body is a block (the return type is inferred from the tail expression). Fix: in the fn-literal grammar production, after the `(...)` parameter list, peek at the next token; if it is `LBrace`, accept the body directly without requiring `->`. The existing `-> { … }` form continues to parse for compatibility.
- **Acceptance criteria:**
  - The fn-literal production accepts both `fn (...) { body }` and `fn (...) -> { body }`.
  - `fn (...) -> Type { body }` (explicit return type) continues to parse.
  - `fn (...) -> body_expr` (non-block body) continues to require `->`.
  - 4 new parser tests: arrow-elided block body, arrow-present block body, arrow-elided + explicit-type rejected (P0XXX or precedence-preserving error), arrow-present + non-block body.
  - A round-trip test through `paideia-fmt` confirms the formatter prints the canonical form (with `->` if the codebase style prefers explicit; without if elided is canonical — decision delegated to fmt policy and noted in the formatter's settings doc).
- **Files:** `crates/paideia-as-parser/src/parse_handler.rs` or wherever fn-literal lives (locate via `grep -n "fn (" crates/paideia-as-parser/src/`), `crates/paideia-as-parser/tests/fn_literal_arrow_elision.rs`, `crates/paideia-fmt/src/settings.rs` (formatter policy).
- **Dependencies:** none.
- **Estimated size:** S
- **Phase:** PA7-completion round.
- **Milestone:** `pa7c-parser-papercuts`.

---

### m3-003. parser: unit-typed blocks accept trailing-semi without requiring `()` (G9)

- **Summary:** `if x < N { a[i] = b; head = i; }` rejects with P0158 ("block expression must have a final expression"). This is correct for value-position blocks but wrong for statement-position blocks where the enclosing context wants unit (`()`). Fix: at the P0158 emit site in `parse_control.rs:297..303`, before emitting the diagnostic, check whether the block is in a unit-typed position (statement-position, the body of a void-return fn, the body of `loop` / `while` / inside `if`/`else` when the enclosing if is statement-position). If yes, synthesise a final `IrKind::Unit` node and accept the block. If no (value-position), continue to emit P0158.
- **Acceptance criteria:**
  - A new helper `Parser::expect_block_kind(expected: BlockKind)` distinguishes `BlockKind::Value` from `BlockKind::Statement`. Statement-position blocks are: the body of an `if`/`else`/`while`/`for`/`loop` when the if-expression itself is in statement position, the body of a `fn (...) -> () { ... }`, any block at top-level in a `unsafe { ... }`.
  - When `expect_block_kind(Statement)` is active and the block ends with `;`, a synthetic `IrKind::Unit` final expression is inserted; no P0158 fires.
  - When `expect_block_kind(Value)` is active (e.g., `let x = { foo(); };`), P0158 fires as today.
  - 8 parser tests covering: statement-position if (no P0158), value-position if (P0158), nested if/else, while-body, for-body, loop-body, void-return fn-body, value-return fn-body (P0158).
  - Regression: the slab/IPC/sched quarantined files re-parse cleanly (verified by m6-002).
- **Files:** `crates/paideia-as-parser/src/parse_control.rs`, `crates/paideia-as-parser/src/parser.rs` (BlockKind enum + threading), `crates/paideia-as-parser/tests/block_kind.rs`.
- **Dependencies:** none.
- **Estimated size:** S
- **Phase:** PA7-completion round.
- **Milestone:** `pa7c-parser-papercuts`.
- **Cross-repo unblock:** prerequisite for the slab/IPC/IPI/sched files (all of which use the if-statement-position form).

---

## 6. Milestone m4 — Expression surface (G4 + G7 + G8)

**Slug:** `pa7c-expression-surface`
**Issues:** 4

The three additions are independent and complete the expression surface needed by the capability-verifier (`~MASK`), the slab/IPC (sized indices), and the bit-twiddling used in page-table walk (`as u32` cast). The IR already has `IrKind::UnaryOp { op, arg }` and `IrKind::Cast { target_ty, arg }` and `IntWidth::{U8, U16, U32, U64, I32, I64}` per the grep run during gap verification. The work is wiring these from new parser productions / new lexer tokens to existing IR forms and on to existing encoders.

---

### m4-001. lexer + parser: unary bitwise NOT prefix `~` (G4)

- **Summary:** Today there is **no `Tilde` token** in the lexer (`grep TokenKind::Tilde crates/paideia-as-lexer/src/` returns empty) — `~` is not lexed at all. Fix: add `TokenKind::Tilde` to the lexer + the corresponding `~` character match in the lexer's scan loop; add a prefix parselet in `parser.rs` / `precedence.rs` that consumes `Tilde` and lowers to `IrKind::UnaryOp { op: BitNot, arg }`; add `BitNot` to the `UnaryOp` enum if not present (grep `crates/paideia-as-ir/src/` for the enum); add an encoder case for `Mnemonic::Not` on `[Reg(r)]` → `F7 D0 + ModRM(r)` (the encoder for `not r64` may already exist; if not, add it — 4 bytes including REX).
- **Acceptance criteria:**
  - `TokenKind::Tilde` exists in `paideia-as-lexer`; `~` lexes as `Tilde`.
  - The expression grammar has a prefix-position parselet for `Tilde` at the same precedence as the existing unary `-` / `!`.
  - `~x` parses to `Expr::UnaryOp(BitNot, x)`.
  - `~x` lowers to `IrKind::UnaryOp { op: BitNot, arg: x_id }`.
  - The encoder emits `F7 D0` (`not rax`) for `UnaryOp { op: BitNot, arg: Reg(0) }`, with REX prefix as needed.
  - **Capability-verifier fixture:** `tests/build-emit/pa7c_cap_verify_bitnot.pdx` containing `let cap_check : u64 -> u64 -> bool = fn (rights mask) -> (rights & ~mask) == 0` builds to a `.text` containing the `not` + `and` + `test` sequence.
  - 4 unit tests at the lexer / parser / IR-lowering / encoder layers.
- **Files:** `crates/paideia-as-lexer/src/token.rs` (+ tilde scan), `crates/paideia-as-parser/src/precedence.rs` (prefix-tier entry), `crates/paideia-as-parser/src/parser.rs` (parselet), `crates/paideia-as-ir/src/` (UnaryOp::BitNot if absent), `crates/paideia-as-encoder/src/encode_instruction.rs` (Mnemonic::Not arm if absent), 4 test files.
- **Dependencies:** none.
- **Estimated size:** S
- **Phase:** PA7-completion round.
- **Milestone:** `pa7c-expression-surface`.
- **Cross-repo unblock:** the capability-verifier hot path (used in `slab.pdx` and any future cap-check op).

---

### m4-002. parser + IR: `EXPR as TYPE` cast operator (G8)

- **Summary:** `(x as u32) & 0xF` rejects today. Grep confirms `KwAs` exists at `parser.rs:274` and `parse_handler.rs:272` but the production is incomplete (it appears in the keyword-printing table for error messages but no grammar rule consumes it). Fix: add a postfix parselet at precedence level immediately below multiplicative ops that consumes `EXPR as TYPE`, parses a type via the existing `parse_type` helper, and lowers to `IrKind::Cast { target_ty, arg }`. The encoder side dispatches on (source-width, target-width): widening signed → `movsx`; widening unsigned → `movzx`; narrowing → `mov` of a sub-register; same-width → no-op.
- **Acceptance criteria:**
  - `x as u32` parses to `Expr::Cast(x_id, Type::U32)`.
  - Lowers to `IrKind::Cast { target_ty: IntWidth::U32, arg }`.
  - Encoder emits the right instruction per (src_width, dst_width) per the SDM table; covered by 12 unit tests (one per combination of {U8, U16, U32, U64} × {U8, U16, U32, U64} where src ≠ dst, plus the four no-ops).
  - Cast to `*T` from `u64` is accepted (pointer-from-integer cast) and lowered to a no-op.
  - Cast from `*T` to `u64` is accepted and lowered to a no-op.
  - Cast between two `*T` types is accepted and lowered to a no-op.
  - **PT-walk fixture:** `tests/build-emit/pa7c_pt_walk_cast.pdx` containing `let pte_idx : *u8 -> u32 = fn (va) -> (va as u64 >> 12) as u32 & 0x1FF` builds to a `.text` containing the shift + and sequence with a `mov eax, eax` (or equivalent narrowing).
  - 4 parser tests + 12 encoder tests + 1 end-to-end fixture.
- **Files:** `crates/paideia-as-parser/src/parser.rs`, `crates/paideia-as-parser/src/precedence.rs` (cast precedence tier), `crates/paideia-as-ir/src/` (Cast variant if absent — likely already there per gap report), `crates/paideia-as-encoder/src/encode_instruction.rs` (Cast dispatch + movsx/movzx encoders if absent), test files.
- **Dependencies:** none directly; coordinates with m4-003 for sized-int plumbing.
- **Estimated size:** S
- **Phase:** PA7-completion round.
- **Milestone:** `pa7c-expression-surface`.
- **Cross-repo unblock:** `mm/pt_walk.pdx` (composite with m1-003), any file that uses `as` to convert between address representations.

---

### m4-003. encoder: thread `IntWidth` from IR through DispatchKind to RegId size (G7)

- **Summary:** The AST + parser already accept `u8` / `u16` / `u32` / `i32` (grep confirms `IntWidth` exists in `paideia-as-ir`). The build pipeline today is u64-only: a `let x : u32 = 42` source binding silently falls out of the build path because `DispatchKind::classify` (introduced Phase 6 m1) treats every `Mov` as 64-bit. Fix: extend `DispatchKind` to carry an `Option<IntWidth>` discriminator pulled from the IR's `TypeSideTable`; the encoder's `encode_mov` reads the discriminator and emits the right REX + opcode-size combination (`B0` for `mov al, imm8`, `66 B8` for `mov ax, imm16`, `B8` for `mov eax, imm32`, `48 B8` for `mov rax, imm64`).
- **Acceptance criteria:**
  - `DispatchKind` (or the equivalent dispatch enum from Phase 6 m1-001 — if the m1-001 work landed without this field, augment it here) gains `width: Option<IntWidth>`.
  - `cmd_build` populates `width` from the `TypeSideTable` entry for the instruction's destination IR node when available.
  - `encode_mov` reads `width` and emits the right opcode-size combination per the SDM Vol 2A `MOV` table.
  - 16 unit tests: 4 widths × 4 operand shapes (`imm`, `reg-reg`, `reg-mem`, `mem-reg`).
  - **Slab fixture:** `tests/build-emit/pa7c_slab_u32_index.pdx` with a `let cap_idx : u32 = slot_id` lowered as `B8 NN 00 00 00` (32-bit mov), not `48 B8 NN 00 00 00 00 00 00 00` (64-bit mov).
  - Regression: tests under `crates/paideia-as-encoder/tests/encode_mov_*.rs` that asserted u64-only behaviour are updated.
- **Files:** `crates/paideia-as-encoder/src/dispatch.rs` (or equivalent), `crates/paideia-as-encoder/src/encode_instruction.rs`, `crates/paideia-as/src/cmd_build.rs` (width-propagation pass), 16 test files (or one parametrised file).
- **Dependencies:** none directly; m4-002 benefits from this landing first (cast emits the right narrowing instruction).
- **Estimated size:** S
- **Phase:** PA7-completion round.
- **Milestone:** `pa7c-expression-surface`.
- **Cross-repo unblock:** `cap/slab.pdx`, IPC files that use `u32` slot indices.

---

### m4-004. tests: round-trip-via-iced-x86 for the m4 expression surface

- **Summary:** A parametrised test file that, for each combination of `~ x`, `x as T`, and sized-int operations, builds a 4-line `.pdx` source, encodes the resulting `.text`, disassembles via `iced-x86`, and asserts the disassembly matches a canonical string. This is the canonical witness for the m4 surface.
- **Acceptance criteria:**
  - `crates/paideia-as/tests/build_emit_pa7c_expr_surface.rs` covers ≥ 20 source/disassembly pairs.
  - Each pair is one source line + one expected disassembly string.
  - Test parametrised via `rstest` (existing dev-dep, verify) or a hand-rolled vec-driven test.
- **Files:** `crates/paideia-as/tests/build_emit_pa7c_expr_surface.rs`.
- **Dependencies:** m4-001, m4-002, m4-003.
- **Estimated size:** XS
- **Phase:** PA7-completion round.
- **Milestone:** `pa7c-expression-surface`.

---

## 7. Milestone m5 — L-value surface (G10)

**Slug:** `pa7c-lvalue-surface`
**Issues:** 2

The slab and IPC source paths use `free_list[idx] = free_head` (array-index l-value) and `(*p).field = value` (pointer-deref + field l-value). Today the assignment-expression grammar only accepts bare-identifier l-values. The fix is a single grammar rule change that accepts any "place expression" (the same set the type-checker already classifies as places) plus an IR lowering to `IrKind::Store { addr, value, ty }` and an encoder for `mov [base + idx*scale], reg`.

---

### m5-001. parser + IR + encoder: array-index l-value `a[i] = expr` (G10 part A)

- **Summary:** The assignment-expression grammar in `parser.rs` parses the LHS via `parse_expr` and then, if `=` follows, checks that the parsed expression is a "place". The check accepts only `Expr::Var`. Fix: extend the "place" classifier to accept `Expr::Index(base, idx)` and lower the assignment to `IrKind::Store { addr: compute(base, idx, elem_size), value, ty: elem_ty }`. The encoder emits `mov [base + idx * scale], reg` per the SDM `MOV r/m, r` table (3-byte ModRM + SIB encoding for `[reg + reg * scale]` addressing).
- **Acceptance criteria:**
  - `a[i] = b` parses to `Expr::Assign(Expr::Index(a, i), b)`.
  - Lowers to `IrKind::Store { addr: ..., value: b_id, ty: elem_ty }` where `addr` is the IR for `&a[i]` (base-plus-index-times-scale).
  - Encoder emits `48 89 04 F7` for `mov [rdi + rsi*8], rax` (`a` in `rdi`, `i` in `rsi`, `b` in `rax`, scale = 8 for u64).
  - For u32 element type: `89 04 B7` (no REX.W since dst is 32-bit).
  - For u8 element type: `88 04 37`.
  - **Slab fixture:** `tests/build-emit/pa7c_slab_freelist_store.pdx` with `free_list[idx] = free_head` builds to the expected byte sequence.
  - **IPC fixture:** `tests/build-emit/pa7c_ipc_ring_store.pdx` with `ring[head & mask] = msg` builds to the expected byte sequence including the `and` for the mask.
  - 6 unit tests (3 element widths × 2 register shapes).
- **Files:** `crates/paideia-as-parser/src/parser.rs` (place classifier), `crates/paideia-as-ir/src/` (Store variant if absent), `crates/paideia-as-encoder/src/encode_instruction.rs` (Store dispatch + SIB encoding), 2 fixtures, 6 test files.
- **Dependencies:** m3-003 (the block-kind machinery is used in the assignment-statement context).
- **Estimated size:** M
- **Phase:** PA7-completion round.
- **Milestone:** `pa7c-lvalue-surface`.
- **Cross-repo unblock:** `cap/slab.pdx`, `ipc/{slots,allocator,dispatch,mpsc_lock,destroy_channel,channel}.pdx`, `ipi/tlb_shootdown.pdx`, `sched/enqueue.pdx`.

---

### m5-002. parser + IR + encoder: pointer-deref l-value `*p = expr` and field-of-deref l-value `(*p).f = expr` (G10 part B)

- **Summary:** Companion to m5-001 for the pointer-deref l-values used in `ipc/channel.pdx` (`(*ch).head = new_head`) and `ipc/destroy_channel.pdx`. The grammar change is the same: extend the place classifier to accept `Expr::Deref(ptr)` and `Expr::FieldAccess(Expr::Deref(ptr), field)`. The IR lowering reuses `IrKind::Store`. The encoder side composes with the Phase-6 m3 struct walker: the field offset is looked up from the `RecordLayoutTable`, then `mov [base + offset], reg` is emitted.
- **Acceptance criteria:**
  - `*p = expr` parses + lowers + encodes as `mov [r], rax` (3 bytes including REX).
  - `(*p).field = expr` parses + lowers + encodes as `mov [r + offset], rax` (3–6 bytes depending on disp size).
  - **Channel fixture:** `tests/build-emit/pa7c_channel_head_store.pdx` with `(*ch).head = new_head` (where Channel has `head` at offset 16) builds to `48 89 47 10` (mov [rdi + 16], rax).
  - **destroy_channel fixture:** `tests/build-emit/pa7c_channel_destroy.pdx` with a 3-field zero-initialisation pattern builds to three sequential stores.
  - 4 unit tests + 2 fixtures.
- **Files:** same set as m5-001 plus `crates/paideia-as-elaborator/src/emit_walker.rs` (Phase-6 m3 struct walker integration).
- **Dependencies:** m5-001 (shares the place classifier).
- **Estimated size:** S
- **Phase:** PA7-completion round.
- **Milestone:** `pa7c-lvalue-surface`.
- **Cross-repo unblock:** `ipc/channel.pdx`, `ipc/destroy_channel.pdx`.

---

## 8. Milestone m6 — End-to-end smoke + PaideiaOS unquarantine (round closure)

**Slug:** `pa7c-end-to-end-smoke`
**Issues:** 4
**Governing docs:** the Phase-5 m6 / Phase-6 m6 closure pattern (`tests/build_emit_uart_smoke.rs`, `tools/run-smoke.sh`); the PaideiaOS quarantine inventory.

This milestone is the proof that m1–m5 compose to drain the quarantine and produce a kernel.elf that boots. It mirrors the Phase-5 m6 and Phase-6 m6 structure: one in-repo fixture that exercises the new surface, one cross-repo unquarantine pass, one runtime smoke driver, one closure-marker commit.

---

### m6-001. fixture: `boot_orchestration_v2.pdx` exercises G1–G10 end-to-end

- **Summary:** Successor to PA7-009's `boot_orchestration.pdx`. A single `.pdx` file that exercises every gap closed in m1–m5: top-level symbol export (G1: three named functions `uart_init`, `uart_putc`, `kernel_main_64`), PLT32 cross-function call (G2: `kernel_main_64` calls `uart_init` then `uart_putc`), unsafe-block body emission (G3: each function body is a non-trivial unsafe sequence), unary `~` (G4: a synthetic cap-check), `as` cast (G8: pointer-to-u64 cast), sized int (G7: `u32` slot index), array-index store (G10A: a 4-element `[u64; 4]` write), `(*p).field` store (G10B: a 2-field record write), unit-typed block (G9: an if-statement with side-effecting body). The fixture compiles + links + boots under QEMU and prints `BOOT_V2_OK\n` over UART.
- **Acceptance criteria:**
  - Fixture file `tests/build-emit/pa7c_boot_orchestration_v2.pdx` exists (~120 lines).
  - `paideia-as build` produces an ELF object whose symbol table has the three expected names, whose `.text` is byte-identical to a checked-in expected-bytes table (computed via `iced-x86` from the source), and whose `.rela.text` contains the two expected PLT32 entries.
  - Link via `ld -T tests/build-emit/pa7c_boot_v2_link.ld -o pa7c_boot_v2.elf pa7c_boot_orchestration_v2.o` exits 0.
  - QEMU smoke via `tools/run-pa7c-boot-v2-smoke.sh` (new) boots the ELF under `qemu-system-x86_64 -machine q35 -display none -serial stdio -device loader,addr=0x100000,file=pa7c_boot_v2.elf`, asserts stdout contains `BOOT_V2_OK\n` within 5 seconds, exits the QEMU monitor cleanly.
  - Smoke test integrated into `crates/paideia-as/tests/build_emit_pa7c_boot_v2.rs` and gated on `qemu-system-x86_64` availability + the Rust nightly toolchain (same gates as `tests/build_emit_uart_smoke.rs`).
- **Files:** `tests/build-emit/pa7c_boot_orchestration_v2.pdx`, `tests/build-emit/pa7c_boot_v2_link.ld`, `tools/run-pa7c-boot-v2-smoke.sh`, `crates/paideia-as/tests/build_emit_pa7c_boot_v2.rs`.
- **Dependencies:** m1, m2, m3, m4, m5 (all milestones must close).
- **Estimated size:** S
- **Phase:** PA7-completion round.
- **Milestone:** `pa7c-end-to-end-smoke`.

---

### m6-002. cross-repo: PaideiaOS 13-file unquarantine + kernel.elf re-build

- **Summary:** The cross-repo unquarantine pass. For each of the 13 files in `PaideiaOS/.quarantine/src/kernel/`, attempt `git mv` back to `src/kernel/<path>`, run `./tools/build.sh`, and verify exit 0. If a file fails, the failure is filed as a paideia-as bug per the `feedback_cross_repo_escalation.md` policy and the file stays in quarantine; in this round we expect zero failures because m1–m5 close every gap surfaced during R1.5–R6.5 authoring. After all 13 files are unquarantined, `tools/stubs.S` is deleted from the link line and the kernel.elf re-builds cleanly. The commit message names every file and links to its unblocking PA7C issue.
- **Acceptance criteria:**
  - All 13 files moved out of `PaideiaOS/.quarantine/src/kernel/` and back into `PaideiaOS/src/kernel/`.
  - `tools/stubs.S` removed from PaideiaOS (entire file deleted, since no remaining symbol needs the workaround).
  - `./tools/build.sh` in PaideiaOS exits 0 and produces `build/kernel.elf`.
  - `readelf -s build/kernel.elf | grep -E 'FUNC|OBJECT'` shows ≥ 30 named symbols (estimated lower bound; the 13 files define ~50 functions + ~10 objects).
  - `nm -u build/kernel.elf` shows zero undefined symbols (the test of G1 + G2 composed).
  - The PaideiaOS commit message lists every unquarantined file with its corresponding PA7C issue ID (per the unquarantine plan table in §10).
- **Files:** PaideiaOS-side: 13 files moved, `tools/stubs.S` deleted, `tools/build.sh` simplified (remove stubs.S from link line). No paideia-as-side files.
- **Dependencies:** m6-001 (smoke must be green before cross-repo work).
- **Estimated size:** S
- **Phase:** PA7-completion round.
- **Milestone:** `pa7c-end-to-end-smoke`.

---

### m6-003. cross-repo: QEMU boot smoke shows real UART banner

- **Summary:** The runtime witness that m1 + m2 + m3 (closing the byte-emission gaps for `unsafe`-block bodies) compose to produce a kernel that actually boots. The PaideiaOS `tools/run-smoke.sh` invokes `qemu-system-x86_64 ... -serial stdio` against `build/kernel.elf`. Expected output on stdout: the UART banner (`PaideiaOS R7 boot`) followed by a halt. The test passes when the banner appears within 5 seconds and the kernel reaches its `hlt` loop without triple-faulting.
- **Acceptance criteria:**
  - PaideiaOS `tools/run-smoke.sh` exits 0.
  - Stdout contains the literal string `PaideiaOS R7 boot` (or whatever banner the unquarantined `banner.pdx` produces — coordinate with the PaideiaOS R6.5 commit message for the exact string).
  - The kernel reaches its `hlt` loop (verified by sending QEMU `system_reset` and confirming no triple-fault was reported in the QEMU log).
  - Smoke run wrapped in `crates/paideia-as/tests/cross_repo_qemu_boot.rs` and gated on PaideiaOS being present + `qemu-system-x86_64` being available.
- **Files:** PaideiaOS-side: `tools/run-smoke.sh` (existing, verify), banner-string coordination. paideia-as-side: `crates/paideia-as/tests/cross_repo_qemu_boot.rs`.
- **Dependencies:** m6-002.
- **Estimated size:** XS
- **Phase:** PA7-completion round.
- **Milestone:** `pa7c-end-to-end-smoke`.

---

### m6-004. closure: STATUS.md + v0.7.0 tag + CHANGELOG + retrospective

- **Summary:** The round-closure marker. STATUS.md updated to record m1–m6 closure + the 13-file unquarantine. CHANGELOG.md gains a `v0.7.0` entry following the v0.6.0 template (milestones list, highlights, operational deferrals). Workspace.package.version bumped from `0.6.0` to `0.7.0` per the version-discipline policy. Tag `v0.7.0` pushed. Retrospective `design/toolchain/phase-transition-pa7c.md` (analogous to `phase-transition-6.md`) documents the round's findings: which gaps were predicted by the PA7-completion design and which only surfaced at integration time; what the unquarantine pass found; the carryover into the next round (G11–G15).
- **Acceptance criteria:**
  - `Cargo.toml` workspace.package.version = `0.7.0`.
  - `CHANGELOG.md` gains a v0.7.0 entry of ~30 lines following the v0.6.0 template.
  - `STATUS.md` updated with m1–m6 closure markers.
  - Tag `v0.7.0` exists on `main` after the final PR merge.
  - Retrospective `design/toolchain/phase-transition-pa7c.md` exists, ~120 lines, sections: (1) gaps predicted vs surfaced; (2) what the unquarantine sweep found; (3) m1-003 byte-tracking refactor lessons; (4) carryover catalogue for the next round.
  - PaideiaOS submodule bump committed in PaideiaOS repo per the `feedback_phase6_to_paideia_os_resume.md` policy: bump the paideia-as submodule pin to the v0.7.0 commit, run `./tools/build.sh`, commit the bump + the unquarantine moves in a single PR.
- **Files:** `Cargo.toml`, `CHANGELOG.md`, `STATUS.md`, `design/toolchain/phase-transition-pa7c.md`, PaideiaOS-side submodule bump.
- **Dependencies:** m6-003.
- **Estimated size:** S
- **Phase:** PA7-completion round.
- **Milestone:** `pa7c-end-to-end-smoke`.
- **Round-closure marker.** This commit message declares the PaideiaOS unquarantine complete and names every unquarantined file.

---

## 9. Critical path + ordering

The critical path through the round is m1 → m2 → m5 → m6, with m3 and m4 as parallelisable sub-tracks. Concretely:

```
m1-001 (real symbols)        ──┐
m1-002 (layout assert)       ──┤
m1-003 (PLT32 offset)        ──┤
m1-004 (PLT32 witness)       ──┤
                                │
                                ├──→ m2-001 (RawInst recognition) ──→ m2-002 (Let-RHS) ──→ m2-003 (Var resolve) ──→ m2-004 (4-file canary)
                                │
m3-001 (handle free)         ──┤
m3-002 (-> optional)         ──┤
m3-003 (unit blocks)         ──┼──→ m5-001 (array-index store) ──→ m5-002 (deref store)
                                │
m4-001 (~ prefix)            ──┤
m4-002 (as cast)             ──┤
m4-003 (sized-int plumbing)  ──┤
m4-004 (expr-surface witness)──┘
                                │
                                ├──→ m6-001 (boot_orchestration_v2) ──→ m6-002 (unquarantine) ──→ m6-003 (QEMU smoke) ──→ m6-004 (v0.7.0 tag)
```

**Unblock map (what gap closure unblocks what file):**

- **G1 (m1-001)** unblocks every cross-file `call sym` in the entire kernel — without it, no quarantined file can be unquarantined because each one references symbols defined in another file.
- **G2 (m1-003)** is necessary for the four files whose `.text` contains calls inserted via the PA7 multi-stmt unsafe path: `kernel_main.pdx`, `int/exceptions.pdx`, `int/idt.pdx`, `mm/pt_walk.pdx`.
- **G3 (m2-001/002/003)** is necessary for every file whose body contains real x86 instructions inside unsafe blocks — which is all 13.
- **G9 (m3-003)** is necessary for the 8 files using if-statement-position blocks with side effects + no tail expression (the slab/IPC/IPI/sched cluster).
- **G10A (m5-001)** is necessary for the 7 files using `arr[i] = v` (slab + most IPC + sched).
- **G10B (m5-002)** is necessary for the 2 files using `(*p).f = v` (channel + destroy_channel).
- **G4 (m4-001), G7 (m4-003), G8 (m4-002)** are required for the verifier (cap-check), the sized-index code (slab/IPC), and the page-table walk (pt_walk) — all 13 files use at least one of these, but the dependency is at the expression level inside individual instructions, not at the file-linkage level. They are necessary for the per-file rebuild to succeed, not for cross-file linking.
- **G5 (m3-001), G6 (m3-002)** are convenience: they affect parse acceptance for source forms that the quarantined files happen to use but could be rewritten around.

**The shortest path to draining the quarantine is m1 → m2 → m6** (which would unquarantine the 4 boot/interrupt files); the full drain requires m3 + m4 + m5 + m6.

---

## 10. PaideiaOS unquarantine plan

Per-file table mapping each quarantined file to the PA7C gap(s) whose closure permits unquarantine. The mapping was verified by reading each file's surface source in `.quarantine/` and matching against the gap list.

| Quarantined path                                                 | Unblocking gaps   | Unblocking issues             | Notes                                                       |
|------------------------------------------------------------------|-------------------|-------------------------------|-------------------------------------------------------------|
| `.quarantine/src/kernel/boot/kernel_main.pdx`                    | G1, G2, G3        | m1-001, m1-003, m2-001..003   | Cross-file `call uart_init` + `call uart_puts`              |
| `.quarantine/src/kernel/core/int/exceptions.pdx`                 | G1, G2, G3        | m1-001, m1-003, m2-001..003   | Exception handlers call into IDT registration + log path    |
| `.quarantine/src/kernel/core/int/idt.pdx`                        | G1, G2, G3        | m1-001, m1-003, m2-001..003   | IDT install loop + lidt invocation                          |
| `.quarantine/src/kernel/core/mm/pt_walk.pdx`                     | G1, G2, G3, G8    | + m4-002                      | `(va as u64 >> 12) as u32 & 0x1FF` cast                     |
| `.quarantine/src/kernel/core/cap/slab.pdx`                       | G1, G3, G7, G9, G10A | + m3-003, m4-001, m4-003, m5-001 | `free_list[idx] = head`; `~MASK` cap-check; u32 slot id     |
| `.quarantine/src/kernel/core/ipc/slots.pdx`                      | G1, G3, G9, G10A  | + m3-003, m5-001              | Ring-slot write; statement-position if                      |
| `.quarantine/src/kernel/core/ipc/allocator.pdx`                  | G1, G3, G9, G10A  | + m3-003, m5-001              | Allocator free-list index store                             |
| `.quarantine/src/kernel/core/ipc/dispatch.pdx`                   | G1, G3, G9, G10A  | + m3-003, m5-001              | Dispatch table store                                        |
| `.quarantine/src/kernel/core/ipc/mpsc_lock.pdx`                  | G1, G3, G9, G10A  | + m3-003, m5-001              | Lock-state index store                                      |
| `.quarantine/src/kernel/core/ipc/destroy_channel.pdx`            | G1, G3, G9, G10A, G10B | + m3-003, m5-001, m5-002 | Zero-fill + `(*ch).head = 0`                                |
| `.quarantine/src/kernel/core/ipc/channel.pdx`                    | G1, G3, G9, G10A, G10B | + m3-003, m5-001, m5-002 | `(*ch).head = new_head` + mut head/tail cursors             |
| `.quarantine/src/kernel/core/ipi/tlb_shootdown.pdx`              | G1, G3, G9        | + m3-003                      | Per-CPU shootdown queue (no array store needed if redesigned; verify) |
| `.quarantine/src/kernel/core/sched/enqueue.pdx`                  | G1, G3, G9, G10A  | + m3-003, m5-001              | Runqueue append + priority-bitmap update                    |

Cross-checked against the gap report's per-file annotation. The four files that the gap report named under G2 (kernel_main, int/exceptions, int/idt, mm/pt_walk) plus the additional file pt_walk identified via G8 (`as` cast) and the slab/IPC/IPI/sched cluster identified via G9/G10 sum to all 13 quarantined files.

**Re-quarantine policy:** if m6-002 surfaces a file that still fails to rebuild after m1–m5 close, the file stays in `.quarantine/` and a new PA7C-NNN issue is opened against paideia-as per the cross-repo escalation policy. The round does **not** close until the quarantine is empty; if a new gap is found, m6-002 is paused and a new milestone is added.

---

## 11. Definition of done (round closure)

The round closes when all of the following hold:

- All 21 issues across m1–m6 are closed; PRs are merged to `main`.
- Workspace test count crosses 2700 (from 2619 at v0.6.0 close). Estimated: m1 adds ~30 tests, m2 ~25, m3 ~15, m4 ~40, m5 ~15, m6 ~10; rough total ~135 new tests → ~2754.
- The `boot_orchestration_v2.pdx` fixture exercises G1–G10 end-to-end and passes under `cargo test --workspace`.
- All 13 PaideiaOS quarantined files are unquarantined; `git status` in PaideiaOS shows `.quarantine/src/kernel/` is empty (the directory is removed or kept as `.gitkeep` only).
- `PaideiaOS/tools/stubs.S` is deleted; `tools/build.sh` produces `build/kernel.elf` without it.
- `tools/run-smoke.sh` exits 0 and stdout shows the UART banner under QEMU.
- `workspace.package.version` = `0.7.0`; tag `v0.7.0` exists on `main`; `CHANGELOG.md` has a v0.7.0 entry; `STATUS.md` reflects the closure; the retrospective `design/toolchain/phase-transition-pa7c.md` is written.
- The PaideiaOS submodule pin is bumped to the v0.7.0 commit and the bump + unquarantine PR is merged on the PaideiaOS side.
- The PaideiaOS R7 plan can start: `paideia-os` resumes per the `feedback_paideia_os_tempo.md` continuous-run policy with the unquarantined kernel as its new starting state.

If a non-trivial gap surfaces during m6-002 or m6-003, the round does **not** close. A new issue is opened, the relevant file stays in quarantine, the round milestone count grows. The disciplined finish is "zero quarantined files + green smoke", not "21 issues closed".

---

## 12. Issue count + total scope summary

- **21 issues** across **6 milestones**.
- **Mean size: S.** Largest single issue: m1-003 (M, the byte-tracking refactor). No L issues.
- **Estimated calendar:** ~3 weeks at the paideia-as autonomous-loop tempo (one milestone closes, then pause for review per `feedback_autonomous_tempo.md`).
- **Cross-repo effect:** drains 13 quarantined `.pdx` files in PaideiaOS; deletes `tools/stubs.S`; resumes paideia-os R7 work with the kernel as a functional artifact (boots to UART under QEMU).
- **Carryover into the next round (Phase 7 / v0.8.0):** G11 supervisor-mnemonic explicit coverage (most ride for free off G3 but explicit per-mnemonic tests are valuable); G12 general `mov [base+disp]`; G13 `.data` array initialisers; G14 real string-literal surface; G15 multiboot2 / PVH Note emission.

---

## 13. Style + provenance notes

- This plan follows the format of `.plans/phase-6-plan.md` (sections §0 scope, §1 boundary, §2 milestone index with table, §3..N per-milestone breakdown with per-issue blocks containing summary/AC/files/deps/size/milestone, §critical path, §unquarantine plan, §DoD, §issue count, §style notes).
- Every cited file path and line number was verified by direct read against the v0.6.0-equivalent `main` tree at `/home/snunez/Development/paideia-as/`:
  - `cmd_build.rs:786..788` synthetic-symbol fallback: read; matches G1 claim.
  - `encode_instruction.rs:600..621` PLT32 reloc emit: read; matches G2 part A.
  - `emit_walker.rs:937..938` parallel byte counter: read; matches G2 part B.
  - `emit_walker.rs:1051..1119` `emit_block_body` TODOs: read; matches G3.
  - `lexer/token.rs:143,415,507` `KwHandle` entries: read; matches G5.
  - `parser.rs:274` + `parse_handler.rs:272` `KwAs` reservation without grammar rule: read; matches G8.
  - `parse_control.rs:224..303` P0158 emit site: read; matches G9.
  - `paideia-as-encoder/src/encode_instruction.rs:219..235` supervisor encoders (Cli, Hlt, Wrmsr, Iretq, Lgdt, Lidt) present: read; basis for the G11 deferral decision.
  - PaideiaOS quarantine inventory: `find .quarantine -name '*.pdx'` returns the 13 files in the gap report; cross-checked.
- The `~` token claim in G4 was verified by `grep TokenKind::Tilde crates/paideia-as-lexer/src/` returning empty — `~` is not lexed today, so the m4-001 work must add the token, not just the parselet.
- The v0.6.0 baseline (workspace.package.version, CHANGELOG entry, recent commits ending `5dd9a1f` for PA7-009) was verified.
- No emoji; no trailing summary; no claims unverified.
