# paideia-as Phase 6 — Cross-repo unblock for PaideiaOS Phase 2 (osarch)

**Author:** osarch agent
**Date:** 2026-06-21
**Repo:** `paideia-os/paideia-as` (workspace at `/home/snunez/Development/paideia-as/`)
**Scope:** Close the five paideia-as gaps that PaideiaOS Phase 1 surfaced during stub-writing, so PaideiaOS Phase 2 (capability system + slab + LAM probe) can land without further per-file workarounds. Phase 6 is bounded by what unblocks **P2-001..P2-024**; surface that would only matter from P3 onward is deferred to Phase 6+/Phase 7.
**Companion:** `.plans/phase-5-build-emit-plan.md` (the closure that shipped at v0.5.0); the original self-hosting plan in `design/toolchain/self-hosting-phase5-plan.md` stays scheduled for Phase 7.

---

## 0. Why this plan exists

Phase 5 closed (commit `9edaf9f`, tag `v0.5.0`) with build-emit activation working for the lowest-common-denominator surface: `let : T = literal`, `fn (...) -> body`, `lambda`, `unsafe { block: { ... } }` payloads, and `*T` raw pointer types. The closure shipped 38 issues across PRs #695–#733 and brought the workspace from 2172 to 2416 tests.

PaideiaOS Phase 1 then closed on 2026-06-20..21 (14/14 issues, commits ending `04e0bbf` and `df11e01`), **but five of the eight `.pdx` boot files in `src/kernel/boot/` are structure-only stubs**. The stub headers all name the same upstream cause: paideia-as cannot emit what the kernel sources need, so the kernel sources had to be hand-cut down to what does emit. The five active stubs are:

- `kernel_main.pdx` — Phase-1 stub writes a single byte; the real banner orchestration needs inter-function `call` in unsafe blocks plus loop encoding.
- `uart.pdx` — `uart_putc` / `uart_puts` are stubs because polling 16550 LSR bit 5 needs `cmp` + conditional jumps.
- `pagetables.pdx` — only the anchor qwords are emitted; the three 4 KiB tables (PML4, PDPT, PD) need `[u64; 512]` zero-initialized arrays in `.bss`.
- `long_mode.pdx` — long-mode entry sequence does not orchestrate the descriptor load because `mov cr*, gpr` silently shipped placeholder bytes (paideia-as #734).
- `banner.pdx` — only 8 bytes of the banner are emitted as a `u64` constant; the full ~64-byte ASCII banner needs a string-literal surface.

Three of these are tracked as filed paideia-as bugs (`#734`, `#735`, `#736`). The remaining two — struct walker activation and `.bss` arrays — are Phase-5-deferred items that the PaideiaOS Phase 2 plan calls out by name (`P2-001` has `[gate paideia-as#struct-walker]`; `P2-002` is `256-entry static cap table in .bss`).

Phase 6 closes exactly the items that PaideiaOS Phase 2 reads. Banner / printk text is **not** an unblock for Phase 2: a 64-byte string buys console aesthetics but no Phase-2 task names it. String literals slip to **Phase 6+ (Phase 7 candidate)**.

---

## 1. Scope boundary (what is NOT in scope)

PaideiaOS Phase 2 — `P2-001..P2-024`, capability system bring-up — is the gating consumer of this Phase 6. m6 below is its unblocking milestone. Everything that does not appear in `PaideiaOS/.plans/paideia-os-osarch-plan.md` Phase P2 task list stays out, in keeping with the Phase-5 discipline of "stop the moment the consumer is unblocked":

- **Self-hosting.** Still Phase 7. The 21-crate / 93k-LoC port catalogued in `design/toolchain/self-hosting-phase5-plan.md` stays deferred — that document is the Phase 7 blueprint.
- **String-literal-as-`*u8` surface (`let s : *u8 = "..."`).** Banner text is the only Phase 1 / Phase 2 consumer; Phase-2 capability code uses structs and integer ops only. String-literal lowering to `.rodata` plus a `*u8` reference is **Phase 6+**, opens when PaideiaOS Phase 3 (IPC) needs `printk`-style trace output.
- **Generic walker-chain activation for the Phase-4 surface.** Generics, traits, enums, borrowed-references, region calculus, stdlib types — none appear in P2-001..024. Their `build` activation stays Phase 6+ per the Phase-5 §12 disposition.
- **Optimisation passes in the build path.** Peephole / DSE / encode-tight / unroll fire at the side-table level (Phase-4 m1-007..010); their build-emit activation remains Phase 6+. Phase-2 capability hot-paths can run unoptimised — the cap-verify call is ~6 instructions.
- **DWARF emission in the ELF path.** The `paideia-as-dwarf` crate exists; wiring it into `--emit elf64` remains Phase 6+. P2 source-level debugging continues to use raw asm-reference disassembly.
- **PE/COFF + PAX activation parity for the new mnemonics.** Both consume `InstructionSideTable` via the shared encoder bridge; m1 / m3 changes propagate automatically as in Phase 5. No new emitter work needed.
- **paideia-as-linker integration.** PaideiaOS continues to invoke GNU `ld` directly via `tools/build.sh`. Linker self-hosting is Phase 7.
- **Multi-file build orchestration.** `paideia-as build a.pdx b.pdx -o linked.elf` remains Phase 7. PaideiaOS glues per-file objects via the existing Makefile / shell driver.
- **Effect-handler runtime materialisation.** Capability code declares effects (`!{sysreg}`) but they are checked-not-enforced at emit; full handler dispatch lands when P3 IPC needs effect-row signatures.
- **`mut` bindings + atomic ops.** P2 capability code is read-mostly (the table grows append-only behind a slab-bump). `let mut` and atomic intrinsics are Phase 6+; their first consumer is P4 (scheduler).
- **Loops outside `unsafe` (i.e., typed `for` / `while` in the build path).** The Phase-4 m8 loop infrastructure stays at the side-table level. P2 uses tail-recursion or in-block `cmp`+`jcc` inside `unsafe`; typed-loop build-emit is Phase 6+.

The Phase 6 scope is the smallest disciplined sequence that lets PaideiaOS Phase 2 land, no more.

---

## 2. Milestone index

Seven milestones, ~37 issues. Mean size **S**. No `L` tasks; the two largest (m3 struct walker, m4 control-flow encoders) are decomposed per-operand-shape and per-encoder.

| #  | Milestone slug                  | Description                                                              | Issues | Critical path |
|----|---------------------------------|--------------------------------------------------------------------------|--------|---------------|
| m1 | `phase-6-encoder-bridge-fixes`  | Operand-shape dispatch fix (#734) + zero-arity walker fix (#736) + strict mode | 6  | yes           |
| m2 | `phase-6-parser-cleanups`       | Empty-arg `fn ()` (#735) + small parser papercuts surfaced during P1.    | 4      | partial       |
| m3 | `phase-6-struct-walker`         | Records lower through build emit; field-access in unsafe blocks works.   | 8      | yes           |
| m4 | `phase-6-control-flow-encoders` | `cmp`/`jcc` in unsafe blocks + `call sym` operand-shape dispatch.        | 6      | partial       |
| m5 | `phase-6-bss-arrays`            | `let mut arr : [u64; 512] = uninit` surface + `.bss` emission.            | 5      | yes           |
| m6 | `phase-6-end-to-end-smoke`      | Cap-system minimal fixture: `cap_mint` + `cap_verify` byte-identical.    | 4      | yes (closure) |
| m7 | `phase-6-docs-closure`          | Retrospective, STATUS.md, v0.6.0 tag, CHANGELOG, examples.               | 4      | no            |
|    | **Σ**                           |                                                                          | **37** |               |

**Critical path** (longest dependency chain through the milestones):
`m1-001 → m1-002 → m1-005 → m3-001 → m3-002 → m3-003 → m3-005 → m3-007 → m5-001 → m5-002 → m5-004 → m6-001 → m6-003 → m6-004 → m7-001 → m7-003` = **16 issues**.

**PaideiaOS Phase-2 unblock:** **m6 close (m6-004)**. P2-001..024 read a cap-descriptor struct + a 256-entry `.bss` table + `cmp`/`jcc` in the verifier hot path; m6's smoke proves the chain (struct walker + `.bss` arrays + control-flow encoders) reaches a real `.o`. m1..m5 are necessary stages; m6 is the gate. The m6-004 commit message declares the unblock explicitly, mirroring the Phase-5 m6-005 marker.

Three parallelisable sub-tracks:

- **m1 (encoder-bridge fixes)** is independent of m3/m4/m5 once m1-001 lands. m1-002 (the bug-fix proper) and m1-003 / m1-004 (the symmetric DR fix + strict mode) can land in parallel.
- **m2 (parser cleanups)** is fully independent. Can land at any tempo; sole gate is the m2-001 fix being on `main` before m6 fixtures are authored, so the cap-system fixture can use the clean `fn ()` form.
- **m4 (cmp/jcc + call)** is independent of m3 / m5. Each encoder issue (cmp / jcc / call-sym) is leaf work.

---

## 3. Milestone m1 — Encoder-bridge fixes (the #734 + #736 family)

**Slug:** `phase-6-encoder-bridge-fixes`
**Issues:** 6
**Governing docs:** paideia-as `#734` (mov cr/dr operand-shape dispatch); paideia-as `#736` (zero-arity operand parsing); `crates/paideia-as-encoder/src/encode_instruction.rs:170` (`encode_instruction` entry); `crates/paideia-as-elaborator/src/unsafe_walker.rs:687..786` (`process_stmt_instruction`).

Phase-5 m2-005 landed the `Mnemonic::MovCr { write }` and m2-006 landed `Mnemonic::MovDr { write }` encoders, both round-trip-tested via `iced-x86`. The leak is at the **bridge**: when PaideiaOS source writes

```
unsafe { block: { mov cr3, rdi } }
```

the m3-003 mnemonic resolver returns `Mnemonic::Mov` (the table at `unsafe_walker.rs:70` maps the bare `"mov"` token), and `encode_instruction` dispatches the resulting `Instruction { mnemonic: Mov, operands: [Reg(0x103), Reg(7)] }` to `encode_mov`, which rejects the `RegId(0x103)` as `Unsupported("invalid register id")`. The build path then swallows the error and ships the 5-byte placeholder — paideia-as #734 documents both bugs.

The fix is two-part:

1. **Bridge dispatch must promote `Mov` → `MovCr` / `MovDr` based on operand shape.** When `Mnemonic::Mov` arrives with either operand being a control-register or debug-register `RegId`, the dispatch routes to `encode_mov_cr_inst` / `encode_mov_dr_inst` instead of `encode_mov`. The `write` flag is derived from which operand carries the CR/DR id (dst → `write: true`; src → `write: false`).
2. **`cmd_build` must exit non-zero on encoder failure.** Today `cmd_build.rs` silently falls back to a placeholder byte sequence on `EncodeError`; the build reports `exit: 0` and an opaque "text emission failed" message. Phase 6 makes any non-recoverable encoder error a hard build failure (with a `--encoder-warn` opt-in for the case where a deliberately partial source is being iterated on).

m1-005 then fixes paideia-as `#736`: the `UnsafeWalker` calls `parse_operand_from_ast` once per AST operand in the `operands` list. For `cli` / `hlt` / `sti` / `nop` / `swapgs` / `cpuid` the AST operand list is empty, but the parser is still entered and synthesises a malformed-operand diagnostic (U1606). The fix is a one-line arity check before the operand loop: zero-arity mnemonics skip the loop entirely.

---

### m1-001. encoder: introduce `Mnemonic::dispatch_kind(operands)` operand-shape classifier

- **Summary:** Add a small classifier function on the encoder side that, given an `Instruction`, returns the actual encoder family it should route to — `MovGeneric`, `MovToCr`, `MovFromCr`, `MovToDr`, `MovFromDr`, `Generic`. The classifier inspects `operands[0]` and `operands[1]` and tests `RegId >= 0x100 && < 0x108` (CR class) or `>= 0x200 && < 0x208` (DR class). This isolates the routing logic from `encode_instruction`'s match arms and gives m1-002 / m1-003 a single point to extend.
- **Acceptance criteria:** task closed when
  - `crates/paideia-as-encoder/src/dispatch.rs` (new) defines `pub enum DispatchKind { MovGeneric, MovToCr, MovFromCr, MovToDr, MovFromDr, Generic }` and `pub fn classify(inst: &Instruction) -> DispatchKind`.
  - For an instruction with `Mnemonic::Mov, [Reg(0x103), Reg(7)]` (CR3 as dst) returns `MovToCr`.
  - For `Mnemonic::Mov, [Reg(7), Reg(0x103)]` returns `MovFromCr`.
  - For `Mnemonic::Mov, [Reg(0x203), Reg(7)]` returns `MovToDr`; reverse returns `MovFromDr`.
  - For `Mnemonic::Mov, [Reg(0), Imm64(42)]` returns `MovGeneric`.
  - For every non-Mov mnemonic returns `Generic` (existing arms continue to dispatch via the per-Mnemonic match).
  - 8 unit tests covering each variant; classifier is `const fn` where possible.
- **Files:** `crates/paideia-as-encoder/src/dispatch.rs` (new), `crates/paideia-as-encoder/src/lib.rs` (re-export).
- **Dependencies:** none.
- **Estimated size:** S
- **Phase:** Phase 6 — PaideiaOS Phase-2 unblock.
- **Milestone:** `phase-6-encoder-bridge-fixes`.

---

### m1-002. encoder: route `mov cr*, gpr` / `mov gpr, cr*` through the classifier (`#734` part A)

- **Summary:** In `encode_instruction.rs::encode_instruction`, route `Mnemonic::Mov` through `dispatch::classify` and dispatch the `MovToCr` / `MovFromCr` variants to the existing `encode_mov_cr` (with the appropriate `write` flag and `cr_idx = RegId - 0x100`). The dispatch derives the GPR index from whichever operand carries the GPR-class RegId.
- **Acceptance criteria:**
  - `Mnemonic::Mov` arm of `encode_instruction` first calls `classify(inst)` and on `MovToCr` / `MovFromCr` builds the `cr_idx` + `gpr_idx` and forwards to `encode_mov_cr`.
  - The synthetic `Instruction { mnemonic: Mov, operands: [Reg(0x103), Reg(7)] }` now encodes to `0F 22 DF` (mov cr3, rdi) — matching the m2-005 round-trip.
  - The reverse `Instruction { mnemonic: Mov, operands: [Reg(7), Reg(0x103)] }` encodes to `0F 20 DF` (mov rdi, cr3).
  - CR8 with REX.R: `mov cr8, rax` (`[Reg(0x108), Reg(0)]`) encodes to `44 0F 22 C0`.
  - **End-to-end fixture:** `tests/build-emit/long_mode_cr_moves.pdx` containing `unsafe { block: { mov cr3, rdi; mov cr4, rcx; mov cr0, rax } }` builds to a `.text` with the 9-byte sequence `0F 22 DF 0F 22 E1 0F 22 C0` (or the m2-005 byte-exact equivalents).
  - 6 integration tests covering the boot-time CR-set sequence and a CR-read sequence (`mov rax, cr3`).
  - `iced-x86` round-trip confirms each byte sequence disassembles back to the same mnemonic.
- **Files:** `crates/paideia-as-encoder/src/encode_instruction.rs`, `tests/build-emit/long_mode_cr_moves.pdx`, `crates/paideia-as/tests/build_emit_phase6_cr_moves.rs`.
- **Dependencies:** m1-001.
- **Estimated size:** S
- **Phase:** Phase 6 — PaideiaOS Phase-2 unblock.
- **Milestone:** `phase-6-encoder-bridge-fixes`.

---

### m1-003. encoder: route `mov dr*, gpr` / `mov gpr, dr*` through the classifier (`#734` part B)

- **Summary:** Symmetric counterpart of m1-002. PaideiaOS Phase 2 does not consume DR moves directly, but the bug class (operand-shape promotion of `Mnemonic::Mov` → privileged-register encoder) is the same — fixing both halves at once is cheaper than coming back per-subsystem, and PaideiaOS Phase 6+ debug-trap work (`P6+`) names DR0..DR7. Includes the same end-to-end fixture-and-round-trip pattern as m1-002 against `encode_mov_dr`.
- **Acceptance criteria:**
  - `Mnemonic::Mov` arm of `encode_instruction` dispatches `MovToDr` / `MovFromDr` via the classifier and forwards to `encode_mov_dr` (`dr_idx = RegId - 0x200`).
  - `Instruction { mnemonic: Mov, operands: [Reg(0x200), Reg(0)] }` encodes to `0F 23 C0` (mov dr0, rax).
  - `Instruction { mnemonic: Mov, operands: [Reg(0), Reg(0x207)] }` encodes to `0F 21 F8` (mov rax, dr7).
  - 8 round-trip tests (DR0..DR7 write + a DR0/DR7 read sample) via `iced-x86`.
  - **No end-to-end PaideiaOS fixture required** (no Phase 2 source uses DR moves); the per-encoder unit tests under `crates/paideia-as-encoder/tests/` suffice.
- **Files:** `crates/paideia-as-encoder/src/encode_instruction.rs`, `crates/paideia-as-encoder/tests/mov_dr_dispatch.rs`.
- **Dependencies:** m1-001.
- **Estimated size:** XS
- **Phase:** Phase 6 — PaideiaOS Phase-2 unblock.
- **Milestone:** `phase-6-encoder-bridge-fixes`.

---

### m1-004. cli: `cmd_build` exits non-zero on encoder failure + `--encoder-warn` opt-in (`#734` part C)

- **Summary:** Today `cmd_build.rs` swallows `EncodeError` for individual instructions and continues emitting; the result is `exit: 0` plus a placeholder text section. Phase 6 inverts the default — any `EncodeError` propagated out of `encode_instruction` aborts the build with `exit: 2` and prints a diagnostic naming the failing IR node ID + the offending source span. A new opt-in flag `--encoder-warn` restores the legacy warn-and-continue behaviour for partial iteration.
- **Acceptance criteria:**
  - `cmd_build::build_elf_object` returns `Err(BuildError::Encoder { node, source_span, encoder_message })` instead of `Ok(_)` with placeholder bytes.
  - The CLI exit code for this error is `2` (reserved for build-substantive failures, distinct from `1` for diagnostics-only).
  - The diagnostic format matches existing emit-stage diagnostics: includes the IR node ID, the originating source span (via `ast_to_ir` reverse lookup), and the lower-level encoder message.
  - `paideia-as build --encoder-warn ...` restores the Phase-5 behaviour: the offending instruction is dropped from the emit, a warning prints, the build exits 0.
  - Regression test: a `.pdx` fixture containing an instruction that the encoder rejects (e.g., the pre-m1-002 `mov cr3, rdi` shape) fails the build with exit 2 under default flags and exits 0 under `--encoder-warn`.
  - `STATUS.md`-style doc note: the change is mentioned in the Phase 6 retrospective (m7-001) as a behavioural change for build-script consumers.
- **Files:** `crates/paideia-as/src/cmd_build.rs`, `crates/paideia-as/src/cli.rs` (the `Build` subcommand flag list), `crates/paideia-as/tests/build_emit_encoder_strict.rs`.
- **Dependencies:** m1-002, m1-003.
- **Estimated size:** S
- **Phase:** Phase 6 — PaideiaOS Phase-2 unblock.
- **Milestone:** `phase-6-encoder-bridge-fixes`.

---

### m1-005. elaborator: `UnsafeWalker` skips operand-parser for zero-arity mnemonics (`#736`)

- **Summary:** In `unsafe_walker.rs::process_stmt_instruction` (around line 700), after `resolve_mnemonic` returns the `Mnemonic`, branch on its declared arity. Zero-arity mnemonics (`cli`, `sti`, `hlt`, `nop`, `swapgs`, `cpuid`, `wrmsr`, `rdmsr`, `iret`, `iretq`, `sysret`, `rep_stosq`) skip the operand-parser loop entirely and proceed to insert an `Instruction { operands: SmallVec::new() }`. Operands present in source for a zero-arity mnemonic become a new diagnostic `U1607` ("unexpected operands for zero-arity instruction"); operands absent in source for a non-zero-arity mnemonic stay U1606 ("operand parsing failed").
- **Acceptance criteria:**
  - Add `Mnemonic::arity(self) -> u8` returning 0 / 1 / 2 / variable per the SDM encoding family. `cli`/`sti`/`hlt`/`nop`/`swapgs`/`cpuid`/`wrmsr`/`rdmsr`/`iret`/`iretq`/`sysret`/`rep_stosq` return 0.
  - `process_stmt_instruction` checks `mnemonic.arity() == 0` before entering the operand-parse loop.
  - For `unsafe { block: { cli; hlt } }`, the resulting `InstructionSideTable` contains two entries with `operands` empty, encoded as `FA F4`.
  - For `unsafe { block: { hlt rax } }` (operand on zero-arity), emits new diagnostic `U1607` with the operand span; the instruction is still emitted (operand ignored) so iteration can continue, mirroring U1606's recovery posture.
  - `crates/paideia-as-diagnostics/catalog.toml` gains entry `U1607` with severity `error` and the message above.
  - Regression: rebuilding `src/kernel/boot/entry.pdx` from the PaideiaOS submodule now succeeds with `cli; hlt` in the unsafe block (no U1606), and the emitted bytes are `FA F4`.
  - Two unit tests in `unsafe_walker.rs::tests` covering zero-arity-without-operands (success) and zero-arity-with-operands (U1607 + recovery).
- **Files:** `crates/paideia-as-elaborator/src/unsafe_walker.rs`, `crates/paideia-as-ir/src/instruction.rs` (Mnemonic::arity), `crates/paideia-as-diagnostics/catalog.toml`, `crates/paideia-as-elaborator/tests/unsafe_walker/zero_arity.rs`.
- **Dependencies:** none (operates on existing Mnemonic enum).
- **Estimated size:** S
- **Phase:** Phase 6 — PaideiaOS Phase-2 unblock.
- **Milestone:** `phase-6-encoder-bridge-fixes`.

---

### m1-006. tests: PaideiaOS Phase-1 stub re-build regression suite

- **Summary:** A Rust integration test under `crates/paideia-as/tests/paideia_os_phase1_rebuild.rs` that, given the PaideiaOS submodule is present (skipped otherwise — `cargo test` on a thin checkout still passes), invokes `cmd_build` on each `.pdx` file under `PaideiaOS/src/kernel/boot/` and asserts exit 0 + non-empty `.text`/`.rodata` per file. This is the cross-repo canary: once m1 lands, the seven `unsafe`-block boot files re-build clean, proving #734 + #736 are dead. Files that still need m3 / m4 / m5 surface are excluded from the suite until those milestones close.
- **Acceptance criteria:**
  - Test discovers PaideiaOS at `../../PaideiaOS` (or honors env var `PAIDEIA_OS_PATH`); skipped with `println!("PaideiaOS not present; skipping")` otherwise.
  - Builds each of `entry.pdx`, `long_mode.pdx`, `gdt.pdx`, `uart.pdx`, `zero_bss.pdx`, `kernel_main.pdx`, `banner.pdx` (7 files) and asserts exit 0.
  - For each file, asserts the resulting `.text` is non-empty unless the file is data-only (`banner.pdx`, `gdt.pdx`'s descriptor block).
  - `pagetables.pdx` is explicitly **excluded** with a `// FIXME(phase6-m5): rebuild once .bss arrays ship` comment; the suite re-includes it in m5-005.
  - Suite runs on every paideia-as CI cycle when the submodule is initialised.
- **Files:** `crates/paideia-as/tests/paideia_os_phase1_rebuild.rs`.
- **Dependencies:** m1-002, m1-005.
- **Estimated size:** XS
- **Phase:** Phase 6 — PaideiaOS Phase-2 unblock.
- **Milestone:** `phase-6-encoder-bridge-fixes`.

---

## 4. Milestone m2 — Parser cleanups

**Slug:** `phase-6-parser-cleanups`
**Issues:** 4
**Governing docs:** paideia-as `#735` (empty-arg `fn ()` fails P0100); `crates/paideia-as-parser/src/parse_lambda.rs:101..115` (the comma-separated identifier loop that requires at least one identifier between `(` and `)`).

`#735` is the only filed parser bug in scope. Three small additional cleanups surface during a re-pass over the PaideiaOS boot stubs — they are filed alongside `#735` because the cost of fixing them is XS each and they all touch the same parse-lambda + parse-block-statement region. None individually unblock Phase 2; collectively they remove ugly per-file workarounds in the PaideiaOS source that would otherwise persist.

The three additional cleanups:

- **m2-002.** Trailing semicolon inside `unsafe { block: { ... } }` currently fails P0101 ("expected statement"). PaideiaOS `entry.pdx` works around with `mov rax, rax` as a single-line body; capability code wants `cli; hlt;` (with the trailing semi) for grep-uniformity with future multi-stmt blocks.
- **m2-003.** The mnemonic-token `lea` inside an unsafe block requires the source to write `lea rax, [rdi + 1]`; the bracket-form memory operand is sometimes mis-parsed when the preceding instruction had a comma-suffixed operand. Not a #735-class bug but the lexer-driven re-sync after a comma is the same code path.
- **m2-004.** `let _start : () -> () = fn () -> ...` (the `_`-prefixed identifier) is parsed correctly today but the resulting symbol export by m5-001 (Phase 5) defaults to `STT_NOTYPE` for any identifier whose first character is `_`. PaideiaOS depends on `_start` being `STT_FUNC` and `STB_GLOBAL` for the linker to honour `ENTRY(_start)`. Phase 5 m6-005 worked around with a synthetic `start` alias; m2-004 makes the underscore prefix orthogonal to symbol kind.

---

### m2-001. parser: `fn () -> body` empty-arg list accepted (`#735`)

- **Summary:** Extend the parameter-list parser in `parse_lambda.rs` to treat the `( )` (no identifiers) case as a valid empty parameter list and produce `Vec::new()` rather than entering the comma-loop and immediately failing P0100 on the closing `)`. Aligns the surface with `fn (x: ())` (the one-character workaround currently used across every PaideiaOS boot stub).
- **Acceptance criteria:**
  - `fn () -> 42` parses; the lambda's `params` is the empty vec.
  - `fn () -> unsafe { effects: {}, capabilities: {}, justification: "", block: { hlt } }` parses (the PaideiaOS `entry.pdx` shape, sans `x: ()` workaround).
  - `let _start : () -> () = fn () -> ()` parses without any P-class diagnostics.
  - The error `P0100 'expected pattern'` continues to fire on truly malformed inputs like `fn (,) -> 42` (leading comma) and `fn (x,,y) -> ...` (double comma).
  - 6 unit tests in `parse_lambda.rs::tests`: 3 success cases (`fn ()`, `fn () -> u64`, `fn () -> unsafe { ... }`) and 3 reject cases (`fn (,)`, `fn (,x)`, `fn (x,,y)`).
  - The pipe form `|| body` (zero-param pipe lambda) gains a parallel guard if it doesn't already; if it does, the test exercises it.
  - **Carry-over update:** every PaideiaOS `.pdx` that uses the `fn (x: ())` workaround is rewritten back to `fn ()` in m7-004 (examples-clean closure). The PaideiaOS-side rewrite is **not** part of this issue — it's a downstream consumer pass.
- **Files:** `crates/paideia-as-parser/src/parse_lambda.rs`, `crates/paideia-as-parser/tests/empty_fn_args.rs`.
- **Dependencies:** none.
- **Estimated size:** XS
- **Phase:** Phase 6 — PaideiaOS Phase-2 unblock.
- **Milestone:** `phase-6-parser-cleanups`.

---

### m2-002. parser: trailing semicolon inside `unsafe { block: { ... } }` accepted

- **Summary:** The unsafe-block payload parser fires `P0101 'expected statement'` when it encounters a trailing `;` before the closing `}`. Fix is a one-line peek-and-consume in the statement-list loop. Brings the unsafe-block surface into parity with the rest of the language (which accepts trailing `;` in `let`-blocks per Phase-1 m6-003).
- **Acceptance criteria:**
  - `unsafe { ..., block: { cli; hlt; } }` parses (note the trailing `;`).
  - `unsafe { ..., block: { hlt; } }` parses (single-statement with trailing `;`).
  - `unsafe { ..., block: { ;; } }` rejects with `P0101` (empty / leading-semi remains an error).
  - 4 unit tests covering accept / reject permutations.
- **Files:** `crates/paideia-as-parser/src/parse_unsafe.rs`, `crates/paideia-as-parser/tests/unsafe_block_trailing_semi.rs`.
- **Dependencies:** none.
- **Estimated size:** XS
- **Phase:** Phase 6 — PaideiaOS Phase-2 unblock.
- **Milestone:** `phase-6-parser-cleanups`.

---

### m2-003. parser: memory-operand re-sync after a comma-suffixed prior operand

- **Summary:** When a statement like `lea rax, [rdi + 1]` follows another instruction whose last operand was comma-terminated, the bracketed memory operand sometimes parses as the prior statement's continuation, surfacing as a confusing P0102 ("unexpected token `[`"). The fix is to require the statement separator (`\n` or `;`) before re-entering the instruction-statement parser. This issue is a P1 stub-writing pain-point but not a blocker; it is bundled with m2-001/m2-002 to keep parser cleanups in one PR window.
- **Acceptance criteria:**
  - `mov rax, rdi; lea rax, [rdi + 1]; ret` (single-line) parses without P0102.
  - `mov rax, rdi\nlea rax, [rdi + 1]\nret` (newline-separated) parses cleanly.
  - 4 fixtures under `tests/parser-corpus/instruction_resync/` covering the comma-after-operand boundary in 4 different mnemonic combinations.
- **Files:** `crates/paideia-as-parser/src/parse_unsafe.rs` (statement-list loop), `tests/parser-corpus/instruction_resync/`.
- **Dependencies:** none.
- **Estimated size:** XS
- **Phase:** Phase 6 — PaideiaOS Phase-2 unblock.
- **Milestone:** `phase-6-parser-cleanups`.

---

### m2-004. elaborator: `_`-prefixed top-level identifiers get the right `SymbolKind`

- **Summary:** Phase-5 m5-001 introduced the `SymbolTable` populated by `EmitWalker` on each module-level `IrKind::Let`. The kind classifier (`Function` vs `Object`) reads the body's `IrKind` (Lambda → Function, else Object). For identifiers starting with `_`, the classifier currently short-circuits to `STT_NOTYPE` on the suspicion that `_`-names are placeholders. Phase 6 removes that short-circuit: `_start` (Lambda body) becomes `STT_FUNC + STB_GLOBAL`; `_data_anchor` (literal body) becomes `STT_OBJECT + STB_GLOBAL`. The "magic name" path for `_start` set up in m5-001 (auto-flag as entry-point) is preserved unchanged.
- **Acceptance criteria:**
  - `let _start : () -> () = fn () -> unsafe { ... }` produces a symbol with `st_info = (STB_GLOBAL << 4) | STT_FUNC`.
  - `let _anchor : u64 = 42` produces `STT_OBJECT + STB_GLOBAL`.
  - The "magic name" entry-point detection for `_start` continues to fire (the auto-mark from m5-001).
  - `readelf -s` on a built object now shows `_start` with `Type: FUNC` and `Bind: GLOBAL`.
  - Regression test: building the existing `examples/02_functions.pdx` (which uses non-underscore names) produces an unchanged symbol table.
  - 3 integration tests covering the three identifier shapes (`_start`, `_anchor`, normal `add_one`).
- **Files:** `crates/paideia-as-elaborator/src/emit_walker.rs`, `crates/paideia-as-emitter-elf/src/symtab.rs`, `crates/paideia-as/tests/symtab_underscore_prefix.rs`.
- **Dependencies:** none.
- **Estimated size:** XS
- **Phase:** Phase 6 — PaideiaOS Phase-2 unblock.
- **Milestone:** `phase-6-parser-cleanups`.

---

## 5. Milestone m3 — Struct walker activation (records lower through build emit)

**Slug:** `phase-6-struct-walker`
**Issues:** 8
**Governing docs:** Phase-4 m7 `records-enums-phase4.md`; `crates/paideia-as-ir/src/record_layout.rs` (RecordLayoutTable + FieldAccessSideTable already present); `crates/paideia-as-elaborator/src/lower.rs` (RecordCons / FieldAccess IR nodes already lowered for `check`); `crates/paideia-as-encoder/src/encode.rs` (SIB-form codegen from Phase-3 m1-007).

Phase-4 m7-001 landed `struct Foo { x: u64, y: u64 }` parsing and the `RecordLayoutTable` / `FieldAccessSideTable` side-tables for type-checking. The `check` path is clean: `paideia-as check` accepts struct declarations, validates field types, enforces field-access exhaustiveness, and emits T0510..T0512 for record-layout / field-access errors. **The `build` path never consumes any of this** — `cmd_build` walks `EmitWalker` (Phase-5 m1) and `UnsafeWalker` (Phase-5 m3) only; neither populates `InstructionSideTable` for `IrKind::RecordCons` or `IrKind::FieldAccess`, so the emitter ships placeholder bytes for any function whose body references a struct field.

PaideiaOS Phase-2 P2-001 requires the descriptor struct to land:

```paideia
struct Capability {
  kind: u64,        // offset 0
  target: u64,      // offset 8
  rights: u64,      // offset 16
  generation: u64,  // offset 24 (total 32 bytes; m4 plan allows 24 if rights+gen fold)
}
```

P2-007 (`cap_verify`) reads `cap.generation` and `cap.kind`; P2-008 (`cap_has_rights`) reads `cap.rights`; P2-009 (`cap_mint`) writes all four fields. Without m3, none of these compile to real machine code — the build path simply ignores the field access and ships placeholder bytes.

m3 wires the `EmitWalker` to populate `InstructionSideTable` for `IrKind::FieldAccess` (load) and `IrKind::RecordCons` (store), reusing Phase-3 m1-007's SIB-form `mov`-with-displacement encoder. The walker emits `mov rax, [base + field_offset]` for reads and `mov [base + field_offset], rsrc` for writes; field offsets come from `RecordLayoutTable` (Phase-4 m7) via the existing `RecordTypeId` lookup.

The minimum surface in scope:

```paideia
let read_kind  : (*Capability) -> u64 = fn (p: *Capability) -> (*p).kind
let read_gen   : (*Capability) -> u64 = fn (p: *Capability) -> (*p).generation
let set_rights : (*Capability, u64) -> () = fn (p, r) -> unsafe {
  effects: {}, capabilities: {}, justification: "cap-rights write per P2-009",
  block: {
    mov [rdi + 16], rsi    // *p.rights = r ; encoded by m3-005
  }
}
```

Records-as-values (passing a `Capability` by value, returning one by value) is **not in scope** — Phase 2 capabilities are always handled via `*Capability` (linearity-encoded handle = u64; the descriptor lives in `.bss` and is addressed by pointer). The "by-value record" surface is Phase 6+ (P3 IPC sends records-as-messages).

---

### m3-001. ir + elaborator: per-struct `RecordLayout` finalisation in `EmitWalker`

- **Summary:** Today `RecordLayoutTable` (Phase-4 m7) holds the `RecordTypeId` per `IrKind::RecordCons` node but the `RecordLayout { fields: Vec<FieldLayout {offset, size}> }` is computed lazily during `check`. The build path needs the layout up-front, with each field's byte offset and total struct size pinned at build start so subsequent issues (m3-002 / m3-005) can use the offsets as encoder constants. m3-001 introduces `EmitPassState::finalise_record_layouts()`, called at the start of `EmitWalker::walk`, that walks every distinct `RecordTypeId` in the IR and stores a finalised `RecordLayout` per type in a new `FinalisedLayoutTable: HashMap<RecordTypeId, RecordLayout>` on `EmitPassState`.
- **Acceptance criteria:**
  - `EmitPassState` gains `record_layouts: HashMap<RecordTypeId, RecordLayout>` where `RecordLayout { size: u64, align: u8, fields: Vec<FieldLayout { offset: u64, size: u8 }> }`.
  - `finalise_record_layouts(types: &TypeArena)` walks every `RecordTypeId` referenced by any `IrKind::RecordCons` / `IrKind::FieldAccess` node in the IR, computes the layout (size = sum-of-field-sizes with natural alignment, no padding for Phase 6 — all fields are `u64`), and inserts the result.
  - Layouts are computed C-ABI-style: fields are laid out in declaration order, each field at its natural alignment offset. The 4-field `Capability` struct (4 × `u64`) lays out as `[0, 8, 16, 24]`, total size 32, align 8.
  - The walker rejects records containing fields of types other than `u64`, `u32`, `u8`, `*T` with a new diagnostic `T0513` ("Phase 6 record-layout supports u64/u32/u8/*T only") — keeps the layout calculus trivial; richer mixes are Phase 6+.
  - 5 unit tests: the cap descriptor (4 × u64); a single-field record; a record mixing u8 + u64 (4-byte and 8-byte fields with natural alignment); a record with a `*u8` field (8-byte on x86_64); rejection of a record containing a record (no nesting in Phase 6).
- **Files:** `crates/paideia-as-elaborator/src/emit_walker.rs`, `crates/paideia-as-ir/src/record_layout.rs`, `crates/paideia-as-diagnostics/catalog.toml` (`T0513`).
- **Dependencies:** none (consumes existing Phase-4 m7 record infrastructure).
- **Estimated size:** S
- **Phase:** Phase 6 — PaideiaOS Phase-2 unblock.
- **Milestone:** `phase-6-struct-walker`.

---

### m3-002. elaborator: `EmitWalker` lowers `IrKind::FieldAccess` for `(*p).field` shape

- **Summary:** When `EmitWalker` enters an `IrKind::FieldAccess` whose record-value child is an `IrKind::Deref(IrKind::Var(arg0))` (the `(*p).field` shape — the only one P2 uses), it emits `mov rax, [rdi + field_offset]` into `InstructionSideTable`. The field offset comes from m3-001's `record_layouts` table via the `FieldAccessSideTable::lookup(node).field_index`. RDI is the canonical first-argument register (System-V AMD64 ABI); generalising to other base registers is Phase 6+ when m3-003 + m3-004 land.
- **Acceptance criteria:**
  - For `(*p).kind` where `p: *Capability` is the lambda's first arg, the walker emits `Instruction { mnemonic: Mov, operands: [Reg(0), MemSib { base: 7, index: None, scale: X1, disp: 0 }] }` → `48 8B 07` (mov rax, [rdi]).
  - For `(*p).generation` (4th field, offset 24), emits `MemSib { ..., disp: 24 }` → `48 8B 47 18`.
  - For a u32 field (size 4), the encoder emits the 32-bit form `mov eax, [rdi + offset]` instead of the 64-bit form; the walker passes the field size into the operand width.
  - For a u8 field, emits `movzx rax, byte [rdi + offset]` (zero-extending to 64 bits per ABI return convention).
  - 4 unit tests covering each shape (`u64` / `u32` / `u8` / `*T` field reads); the `*T` field reads use the 64-bit form same as `u64`.
  - The walker errors with `T0514` ("field access via non-Deref(Var) shape not supported in Phase 6") for any FieldAccess whose record-child is not `Deref(Var(_))` — defers richer access patterns to Phase 6+.
  - **End-to-end fixture:** `tests/build-emit/cap_read_kind.pdx` containing `let read_kind : (*Capability) -> u64 = fn (p: *Capability) -> (*p).kind` builds to a `.text` containing the 3-byte `48 8B 07` plus a `c3` return → 4 bytes total.
- **Files:** `crates/paideia-as-elaborator/src/emit_walker.rs`, `crates/paideia-as-diagnostics/catalog.toml` (`T0514`), `tests/build-emit/cap_read_kind.pdx`, `crates/paideia-as/tests/build_emit_field_read.rs`.
- **Dependencies:** m3-001.
- **Estimated size:** S
- **Phase:** Phase 6 — PaideiaOS Phase-2 unblock.
- **Milestone:** `phase-6-struct-walker`.

---

### m3-003. elaborator: `EmitWalker` lowers `IrKind::Let(FieldAccess)` for in-block field bindings

- **Summary:** Cap-verify reads two fields back-to-back: `let g = (*p).generation; let k = (*p).kind`. m3-002 lowers each `FieldAccess` to a `mov rax, ...` — but two such lowerings in a row clobber RAX. m3-003 teaches the walker to assign distinct destination registers when the surrounding context is a `Let` binding inside a multi-statement body: the first read goes to RAX, the second to RCX, the third to RDX, the fourth to R8 (calling-convention scratch order). Up to 4 in-flight reads supported; the 5th raises a new diagnostic `E0901` ("register pressure exceeded in Phase 6 field-bind"). Phase 6+ introduces a proper register allocator.
- **Acceptance criteria:**
  - A 2-stmt body `let g = (*p).generation; let k = (*p).kind` emits `mov rax, [rdi + 24]; mov rcx, [rdi]` (the second read lands in RCX, not RAX).
  - A 4-stmt body assigns RAX, RCX, RDX, R8 in order.
  - A 5-stmt body fires `E0901` on the fifth `Let`.
  - `EmitPassState` gains `scratch_assignment: Vec<RegId>` reset at each function boundary (so cross-function RAX clobber stays untracked — each function owns its own scratch sequence).
  - 3 unit tests covering 1 / 4 / 5 in-flight reads.
- **Files:** `crates/paideia-as-elaborator/src/emit_walker.rs`, `crates/paideia-as-diagnostics/catalog.toml` (`E0901`).
- **Dependencies:** m3-002.
- **Estimated size:** S
- **Phase:** Phase 6 — PaideiaOS Phase-2 unblock.
- **Milestone:** `phase-6-struct-walker`.

---

### m3-004. elaborator: `EmitWalker` lowers `IrKind::RecordCons` for cap-mint shape

- **Summary:** When `EmitWalker` enters an `IrKind::RecordCons` whose fields are `[Var(arg0), Var(arg1), Var(arg2), Literal(0)]` (the cap-mint shape — three constructor args + a zero-generation literal), and the surrounding context binds it to a `let p : *Capability = ...` whose body is a separate allocation call, it emits `mov [rdi], rsi; mov [rdi + 8], rdx; mov [rdi + 16], rcx; mov [rdi + 24], 0` — four field stores into the buffer pointed to by RDI (the caller's allocation). Phase 6 limits `RecordCons` lowering to the "fill a caller-provided buffer" form; "allocate + return by value" stays Phase 6+ (needs the allocator surface from Phase-4 m10 wired into `build`).
- **Acceptance criteria:**
  - For a 4-field cap descriptor with all-u64 fields, the walker emits exactly 4 store instructions, in field-declaration order.
  - Each store uses `MemSib { base: RegId, index: None, scale: X1, disp: field_offset }`.
  - The source-register assignment follows System-V AMD64 ABI order: RSI, RDX, RCX, R8 for the 2nd..5th args (RDI is the buffer pointer).
  - Literal-valued fields (e.g., `generation: 0` at construction time) emit `mov [rdi + offset], 0` via the imm32-sign-extended form (`48 C7 47 18 00 00 00 00`).
  - Walker errors with `T0515` ("RecordCons must be bound by `let p : *T = construct_into(...)` form in Phase 6") for any RecordCons not in the supported shape.
  - **End-to-end fixture:** `tests/build-emit/cap_mint.pdx` containing the cap-mint function builds to a `.text` whose `objdump -d` shows the four `mov [rdi + N]` instructions in offset order.
  - 4 unit tests covering 1 / 2 / 3 / 4-field RecordCons shapes; rejection test for the "allocate-and-return" shape.
- **Files:** `crates/paideia-as-elaborator/src/emit_walker.rs`, `crates/paideia-as-diagnostics/catalog.toml` (`T0515`), `tests/build-emit/cap_mint.pdx`, `crates/paideia-as/tests/build_emit_record_cons.rs`.
- **Dependencies:** m3-001, m3-002.
- **Estimated size:** M
- **Phase:** Phase 6 — PaideiaOS Phase-2 unblock.
- **Milestone:** `phase-6-struct-walker`.

---

### m3-005. unsafe-walker + ir: field-access expression inside `unsafe { block: { ... } }` payload

- **Summary:** Phase-5 m3 wired the `UnsafeWalker` to parse register + immediate + memory-ref operands. PaideiaOS Phase-2 needs a fourth operand shape: `*p.field` (a field-projection operand) on the LHS or RHS of a `mov`. Concretely, P2-009 `cap_mint` writes `*p.rights = r` inside an unsafe block to bypass the typed `RecordCons` lowering — sometimes the descriptor is filled in-place by an interrupt handler that the typed surface can't yet express. m3-005 extends `parse_operand_from_ast` to recognise the `*ident.field` AST shape and emit `Operand::MemSib { base: <ident's RegId>, index: None, scale: X1, disp: <field offset> }`.
- **Acceptance criteria:**
  - `*p.rights = r` inside an unsafe block parses as a `mov [rdi + 16], rsi`-shaped instruction (with the field offset resolved at parse time via `RecordLayoutTable` lookup).
  - The operand parser uses the same `EmitPassState.record_layouts` table populated by m3-001.
  - For a struct field whose offset cannot be resolved (record type not finalised), emits new diagnostic `U1608` ("field offset not resolved; declare struct before use").
  - The reverse form `let v = *p.kind` inside an unsafe block (i.e., a `let`-binding inside the block — not the typed surface) is **not in scope** here — unsafe-block payload remains statement-only; the typed surface (m3-002) covers reads.
  - 4 unit tests covering write-via-pointer-field permutations for each of the 4 cap-descriptor fields.
  - End-to-end fixture: `tests/build-emit/cap_set_rights.pdx` builds; emits the expected `48 89 77 10` (mov [rdi + 16], rsi).
- **Files:** `crates/paideia-as-elaborator/src/unsafe_walker.rs` (operand parser), `crates/paideia-as-diagnostics/catalog.toml` (`U1608`), `tests/build-emit/cap_set_rights.pdx`, `crates/paideia-as/tests/build_emit_field_ptr_write.rs`.
- **Dependencies:** m3-001, m3-002.
- **Estimated size:** S
- **Phase:** Phase 6 — PaideiaOS Phase-2 unblock.
- **Milestone:** `phase-6-struct-walker`.

---

### m3-006. emitter-elf: record-layout debug info via `.note.paideia` (placeholder)

- **Summary:** Drop a minimal `.note.paideia` ELF note containing the JSON-serialised `record_layouts` table so downstream tools (PaideiaOS's `tools/inspect-caps.sh`, debugger pretty-printers) can resolve field offsets without re-parsing source. Phase 6 emits the note; DWARF integration is Phase 6+.
- **Acceptance criteria:**
  - Built ELF objects contain a `.note.paideia` section with `n_namesz = 8` (b"paideia\0"), `n_type = 0x50441600` (`PDX_LAYOUTS`), and the descriptor bytes = `serde_json::to_vec(&record_layouts)`.
  - `readelf -n <object>` shows the note with the expected name + type.
  - The section is `SHT_NOTE`, `SHF_ALLOC = 0` (not loaded into memory).
  - Round-trip test: a Rust test reads back the note from an emitted `.o` via the `object` crate and verifies the `Capability` struct's 4-field layout is present.
  - Section omitted entirely when `record_layouts` is empty (no struct declared).
- **Files:** `crates/paideia-as-emitter-elf/src/notes.rs` (new), `crates/paideia-as-emitter-elf/src/writer.rs`, `crates/paideia-as/tests/note_paideia_layouts.rs`.
- **Dependencies:** m3-001.
- **Estimated size:** S
- **Phase:** Phase 6 — PaideiaOS Phase-2 unblock.
- **Milestone:** `phase-6-struct-walker`.

---

### m3-007. cli: `cmd_build` runs `EmitWalker::finalise_record_layouts` before the per-construct walk

- **Summary:** Wire the m3-001 finalisation into the build pipeline. Runs at the start of the EmitWalker pass, after lowering and before per-node walk. The check subcommand path is **not** modified — `finalise_record_layouts` is build-only (consistent with the Phase-5 m1-005 / m3-005 discipline).
- **Acceptance criteria:**
  - `cmd_build.rs` calls `emit_walker.pass_state_mut().finalise_record_layouts(&lowering.types)` before `lowering.ir.walk(&mut emit_walker)`.
  - On an empty `.pdx` (no struct declared) the finalisation is a no-op (0 entries).
  - On a `.pdx` declaring 3 structs, the table has 3 entries.
  - Diagnostics from `finalise_record_layouts` (including T0513 from m3-001) route through the existing walker sink.
- **Files:** `crates/paideia-as/src/cmd_build.rs`.
- **Dependencies:** m3-001.
- **Estimated size:** XS
- **Phase:** Phase 6 — PaideiaOS Phase-2 unblock.
- **Milestone:** `phase-6-struct-walker`.

---

### m3-008. examples + corpus: struct-walker activation tests

- **Summary:** A new corpus under `tests/build-emit/struct/` exercising each m3 lowering shape end-to-end: cap-descriptor read / cap-descriptor write / cap-descriptor mint / cap-descriptor field-set inside unsafe block. Each `.pdx` fixture has a companion `.expected_bytes.txt` snapshot and an integration test under `crates/paideia-as/tests/build_emit_struct_corpus.rs`.
- **Acceptance criteria:**
  - `tests/build-emit/struct/cap_read_kind.pdx` (m3-002 fixture) — snapshot 4 bytes.
  - `tests/build-emit/struct/cap_read_generation.pdx` — snapshot 5 bytes.
  - `tests/build-emit/struct/cap_mint.pdx` (m3-004 fixture) — snapshot ~24 bytes.
  - `tests/build-emit/struct/cap_set_rights.pdx` (m3-005 fixture) — snapshot 5 bytes.
  - `tests/build-emit/struct/cap_verify_compound.pdx` — multi-stmt cap-verify: reads `(*p).generation`, reads `(*p).kind`, returns `kind` — snapshot ~10 bytes.
  - Integration test under `build_emit_struct_corpus.rs` builds each, asserts exit 0, snapshot-matches the byte sequence.
- **Files:** `tests/build-emit/struct/*.pdx`, `tests/build-emit/struct/*.expected_bytes.txt`, `crates/paideia-as/tests/build_emit_struct_corpus.rs`.
- **Dependencies:** m3-002, m3-004, m3-005, m3-007.
- **Estimated size:** S
- **Phase:** Phase 6 — PaideiaOS Phase-2 unblock.
- **Milestone:** `phase-6-struct-walker`.

---

## 6. Milestone m4 — Control-flow encoders (cmp / jcc / call sym inside unsafe blocks)

**Slug:** `phase-6-control-flow-encoders`
**Issues:** 6
**Governing docs:** Intel SDM Vol 2A §3.2 (CMP); Vol 2B §4.2 (Jcc); `crates/paideia-as-encoder/src/encode_instruction.rs:170` (existing `encode_instruction` dispatch); `crates/paideia-as-encoder/src/encode_instruction.rs:387` (`encode_call` already exists); `crates/paideia-as-elaborator/src/unsafe_walker.rs` (the surface that consumes the new mnemonics in source).

The Mnemonic enum already declares `Cmp` (10 base mnemonics) and `Jcc(Cond)` (Phase-3 m1 / Phase-4 m1 plumbing). What does not exist is:

1. **A real `Cmp` encoder.** `encode_cmp` is a stub today — calls return `Err(EncodeError::Unsupported("phase-5 m2-001"))` for any operand shape. PaideiaOS P2-007 cap-verify needs `cmp [rdi + 24], rcx` (compare cap.generation to caller-supplied) and `cmp rax, 0` (test verifier result).
2. **A real `Jcc` encoder with label resolution.** `encode_jcc` is the only encoder that needs to resolve a label within the same function: the source writes `cmp ...; jne fail_label; ...; fail_label: hlt`, the encoder writes a placeholder offset, and the emit-pass-2 patcher fills in the relative displacement. Phase 5 deferred this; Phase 6 lands it.
3. **`call sym` operand-shape dispatch inside unsafe blocks.** `encode_call` accepts `Operand::SymbolRef` (Phase-5 m5-002) and emits the `E8 <rel32>` form plus a `RelocSite`. What's missing is the **UnsafeWalker side**: today's m3-002 operand parser doesn't recognise a bare identifier in `call` operand position as a `SymbolRef` — it returns `OperandError::UnknownRegister`. The fix is a one-classifier addition in `parse_operand_from_ast`.

`call sym` is on the critical path: PaideiaOS P2-009 `cap_mint` calls `cap_alloc` (the slab allocator from P2-005) before populating fields; without inter-fn call inside unsafe blocks, cap-mint can't compose its prerequisites.

What is **not** in m4: loop-back labels (forward labels are needed for `jne fail` patterns; backward labels for spin loops are useful but P2 doesn't have spin loops — those land at P4 scheduler / P3 IPC). `Mnemonic::Loop` / `Mnemonic::Loope` / `Mnemonic::Loopne` stay deferred to Phase 6+. The `LOOP` instruction family is rarely used in modern code anyway; explicit `dec rcx; jnz back_label` is the idiomatic replacement once Phase 6+ ships backward labels.

---

### m4-001. encoder: real `Cmp` encoder for `cmp reg, reg`, `cmp [mem], reg`, `cmp reg, imm`

- **Summary:** Implement the three operand shapes of `cmp` that PaideiaOS P2 actually uses. Other operand shapes (`cmp reg, [mem]`, `cmp [mem], imm`) raise `EncodeError::Unsupported("cmp shape: ...")` — they're cheap to add when a later phase needs them.
- **Acceptance criteria:**
  - `cmp rax, rdi` (`[Reg(0), Reg(7)]`) → `48 39 F8` (3 bytes, opcode `39 /r`).
  - `cmp [rdi + 24], rcx` (`[MemSib { base: 7, ..., disp: 24 }, Reg(1)]`) → `48 39 4F 18` (4 bytes, opcode `39 /r`).
  - `cmp rax, 0` (`[Reg(0), Imm64(0)]`) → `48 83 F8 00` (4 bytes, opcode `83 /7 ib`, sign-extended imm8); for `rax, imm32`, opcode `81 /7 id`.
  - `cmp rax, 0x7FFF_FFFF_FFFF_FFFF` (out-of-imm32 range) → `EncodeError::Unsupported("cmp imm64 not supported; load into reg first")`.
  - Operand shape outside the three supported forms returns `EncodeError::Unsupported("cmp shape: ...")` with the shape name.
  - 12 round-trip unit tests via `iced-x86` covering reg/reg, mem/reg, reg/imm8, reg/imm32 across 3 register pairs.
  - SDM reference: Vol 2A §3.2 CMP.
- **Files:** `crates/paideia-as-encoder/src/encode.rs`, `crates/paideia-as-encoder/src/encode_instruction.rs`.
- **Dependencies:** none.
- **Estimated size:** S
- **Phase:** Phase 6 — PaideiaOS Phase-2 unblock.
- **Milestone:** `phase-6-control-flow-encoders`.

---

### m4-002. ir + unsafe-walker: label declaration + forward-label operand shape

- **Summary:** Extend the unsafe-block surface to accept label declarations (`fail_label:` as a statement) and label references (the identifier in `jne fail_label`). Add `IrKind::Label { name: String }` and `Operand::LabelRef { name: String }`. The UnsafeWalker collects label declarations into `EmitPassState.labels: HashMap<String, u64>` (placeholder offset = 0 at walk time) and label references into `EmitPassState.label_fixups: Vec<LabelFixup { byte_offset: u32, label: String, kind: FixupKind::Rel32 | FixupKind::Rel8 }>`.
- **Acceptance criteria:**
  - Source `fail_label: cli` parses; the parser emits two statements (the label decl + the `cli`).
  - Source `jne fail_label` parses as a Jcc instruction with `Operand::LabelRef { name: "fail_label" }`.
  - `IrKind::Label` is added to the IR enum with one field (`name: String`).
  - `Operand::LabelRef { name, addend }` is added to `paideia_as_ir::Operand`.
  - The walker collects label declarations into `EmitPassState.labels` keyed by name, with the byte offset of the instruction immediately following.
  - The walker collects label references into `EmitPassState.label_fixups`; the encoder writes a zero displacement and returns the byte offset in the placeholder via `EncodeOutput.label_fixups`.
  - Duplicate label declaration in the same function emits new diagnostic `U1609` ("label `X` declared twice").
  - Unknown label reference emits `U1610` ("undefined label `X`") at fixup-resolution time.
  - 6 unit tests covering the parse + walk paths.
- **Files:** `crates/paideia-as-parser/src/parse_unsafe.rs`, `crates/paideia-as-ir/src/node.rs`, `crates/paideia-as-ir/src/instruction.rs`, `crates/paideia-as-elaborator/src/unsafe_walker.rs`, `crates/paideia-as-diagnostics/catalog.toml` (`U1609`, `U1610`).
- **Dependencies:** none.
- **Estimated size:** S
- **Phase:** Phase 6 — PaideiaOS Phase-2 unblock.
- **Milestone:** `phase-6-control-flow-encoders`.

---

### m4-003. encoder: real `Jcc` encoder for forward labels (`rel32` form)

- **Summary:** Implement the conditional-jump encoder for forward labels (the only direction m4-002 supports). Each Jcc variant uses the 2-byte opcode form (`0F 8x <rel32>`), 6 bytes total. The encoder writes the opcode plus a zero `rel32` placeholder and returns a `LabelFixup` site for the patcher to fill in. Backward-jump support (where the offset can be computed at encode time) is **also** included since it's the same encoder — only the patch-vs-emit-time logic differs, and m4-002's `EmitPassState.labels` already supports the lookup.
- **Acceptance criteria:**
  - `je fail` (forward label) → `0F 84 00 00 00 00` (6 bytes; the `0F 84` is the JE opcode); the encoder returns a `LabelFixup { byte_offset: 2, label: "fail", kind: Rel32 }`.
  - All 16 Jcc variants in the m3-003 table (`je`, `jne`, `jl`, `jg`, `jle`, `jge`, `jb`, `jbe`, `ja`, `jae`, `jz`, `jnz`, `js`, `jns`, `jo`, `jno`) round-trip via `iced-x86`.
  - The unconditional `jmp` (Mnemonic::Jmp) with a `LabelRef` operand uses the `E9 <rel32>` form (5 bytes) and the same fixup mechanism.
  - For backward labels (declared earlier in the block), the encoder writes the actual rel32 directly instead of returning a fixup — branch-displacement is `target_offset - (current_offset + 6)`.
  - **End-to-end fixture:** `tests/build-emit/cap_verify_jcc.pdx` containing `cmp [rdi + 24], rcx; jne fail; mov rax, 1; ret; fail: mov rax, 0; ret` builds and `objdump -d` shows the JE displacement resolved to the `fail:` offset.
  - 18 unit tests: 16 Jcc + 1 Jmp + 1 backward-label sanity.
  - SDM reference: Vol 2B §4.2 Jcc.
- **Files:** `crates/paideia-as-encoder/src/encode.rs`, `crates/paideia-as-encoder/src/encode_instruction.rs`, `tests/build-emit/cap_verify_jcc.pdx`, `crates/paideia-as/tests/build_emit_jcc.rs`.
- **Dependencies:** m4-002.
- **Estimated size:** M
- **Phase:** Phase 6 — PaideiaOS Phase-2 unblock.
- **Milestone:** `phase-6-control-flow-encoders`.

---

### m4-004. cli: emit-pass-2 patcher applies label fixups + relocations after `.text` complete

- **Summary:** Today `cmd_build` does a single emit pass: instructions encode into the buffer and relocation sites are recorded as the encoder produces them. m4-004 adds an emit-pass-2 step that runs after the entire function's `.text` is written: it walks `EmitPassState.label_fixups`, looks up each label's resolved offset in `EmitPassState.labels`, computes the rel32 displacement, and patches the 4 bytes at `byte_offset`. Label fixups that reference an undeclared label emit `U1610`. The relocation pipeline (m4-004 of Phase 5) is unchanged — these are intra-function-local fixups, not cross-symbol relocations, so they don't enter `.rela.text`.
- **Acceptance criteria:**
  - `cmd_build::build_elf_object` runs a `patch_label_fixups()` step after the encoder loop completes.
  - For each `LabelFixup`, the patcher computes `displacement = label_offset - (fixup_byte_offset + 4)` and writes the i32 little-endian into the buffer.
  - For an unresolved label, emits `U1610` with the originating source span and aborts the build (exit 2 under the m1-004 strict mode).
  - Per-function label maps are scoped per function (declared at function entry; cleared at function exit) — labels do not leak across function boundaries.
  - Regression test: the m4-003 fixture builds cleanly and `objdump -d` shows the resolved JE displacement.
  - Integration with the m1-004 strict-mode flag: a function with an undeclared label fails under default flags; under `--encoder-warn`, the offending function is dropped from the emit (but the build exits 0).
- **Files:** `crates/paideia-as/src/cmd_build.rs`, `crates/paideia-as/tests/build_emit_label_patches.rs`.
- **Dependencies:** m4-002, m4-003.
- **Estimated size:** S
- **Phase:** Phase 6 — PaideiaOS Phase-2 unblock.
- **Milestone:** `phase-6-control-flow-encoders`.

---

### m4-005. unsafe-walker: bare-identifier operand in `call` position resolves to `SymbolRef`

- **Summary:** Phase-5 m5-002 added `Operand::SymbolRef { name, addend }` and the `call_sym` round-trip is unit-tested in `encode_instruction.rs::encode_call_symbol_ref_produces_reloc_site`. The gap is the unsafe-walker side: `parse_operand_from_ast` today only returns `Reg` / `Imm` / `MemSib` / (Phase 6 m3-005) `MemSib`-with-field-offset; a bare identifier in operand position errors with `OperandError::UnknownRegister`. m4-005 adds a classifier: if the mnemonic context is `call` / `jmp` (later: `lea` for label-as-address loads — Phase 6+) and the operand is a bare identifier not matching any register name, return `Operand::SymbolRef { name: ident, addend: 0 }`.
- **Acceptance criteria:**
  - `call cap_alloc` inside an unsafe block parses to `Instruction { mnemonic: Call, operands: [SymbolRef { name: "cap_alloc", addend: 0 }] }`.
  - The encoder dispatches to `encode_call` (already SymbolRef-aware from Phase 5), emits `E8 00 00 00 00`, returns a `RelocSite { byte_offset: 1, symbol: "cap_alloc", kind: R_X86_64_PLT32, addend: -4 }`.
  - `ld` resolves the call to the actual offset of `cap_alloc` in the local TU; cross-TU calls produce undefined-symbol entries per Phase-5 m5-004.
  - **Mnemonic context required:** `mov rax, cap_alloc` (a SymbolRef in non-call/jmp operand position) emits a Phase 6+ diagnostic `U1611` ("SymbolRef operand not supported for mnemonic `mov` in Phase 6"); the `lea rax, [cap_alloc]` form (load symbol address) is Phase 6+ once RIP-relative LEA lands.
  - **End-to-end fixture:** `tests/build-emit/cap_mint_calls_alloc.pdx` containing `let mint : (u64, u64, u64) -> *Capability = fn (k, t, r) -> unsafe { ..., block: { call cap_alloc; mov [rax], rdi; ret } }` builds and emits a relocation against `cap_alloc`.
  - 4 unit tests covering call + jmp + the two reject paths (unknown ident in non-call context; SymbolRef in mov).
- **Files:** `crates/paideia-as-elaborator/src/unsafe_walker.rs` (operand parser), `crates/paideia-as-diagnostics/catalog.toml` (`U1611`), `tests/build-emit/cap_mint_calls_alloc.pdx`, `crates/paideia-as/tests/build_emit_call_sym.rs`.
- **Dependencies:** none (consumes existing Phase-5 m5-002 SymbolRef infrastructure).
- **Estimated size:** S
- **Phase:** Phase 6 — PaideiaOS Phase-2 unblock.
- **Milestone:** `phase-6-control-flow-encoders`.

---

### m4-006. examples + corpus: control-flow corpus (cmp + jcc + call permutations)

- **Summary:** Build a corpus under `tests/build-emit/control_flow/` mirroring m3-008's pattern. Each fixture exercises one m4 capability; each is snapshot-tested for byte-exact emission. The corpus is the regression net for the cap-verify hot path which composes cmp + jne + call across two functions.
- **Acceptance criteria:**
  - `tests/build-emit/control_flow/cmp_reg_reg.pdx` — `cmp rax, rdi; ret` — snapshot 4 bytes.
  - `tests/build-emit/control_flow/cmp_mem_reg.pdx` — `cmp [rdi + 24], rcx; ret` — snapshot 5 bytes.
  - `tests/build-emit/control_flow/jne_forward.pdx` — `cmp rax, 0; jne fail; ret; fail: hlt` — snapshot ~12 bytes (cmp 4 + jne 6 + ret 1 + hlt 1).
  - `tests/build-emit/control_flow/jne_backward.pdx` — `loop_top: nop; cmp rax, 0; jne loop_top; ret` — snapshot ~9 bytes.
  - `tests/build-emit/control_flow/call_sym.pdx` (cross-fn call) — `let f = fn () -> unsafe { ..., block: { call g; ret } }; let g = fn () -> unsafe { ..., block: { ret } }` — snapshot ~7 bytes for `f`, 1 byte for `g`; reloc against `g` present.
  - `tests/build-emit/control_flow/cap_verify_compound.pdx` — full P2-007 hot path: load `(*p).generation`, cmp with caller-supplied, jne fail, load `(*p).kind`, return — snapshot ~16 bytes.
  - Integration test under `build_emit_control_flow_corpus.rs` builds each, asserts exit 0, snapshot-matches.
- **Files:** `tests/build-emit/control_flow/*.pdx`, `tests/build-emit/control_flow/*.expected_bytes.txt`, `crates/paideia-as/tests/build_emit_control_flow_corpus.rs`.
- **Dependencies:** m4-001, m4-003, m4-004, m4-005.
- **Estimated size:** S
- **Phase:** Phase 6 — PaideiaOS Phase-2 unblock.
- **Milestone:** `phase-6-control-flow-encoders`.

---

## 7. Milestone m5 — `.bss` arrays (zero-initialised data for cap table)

**Slug:** `phase-6-bss-arrays`
**Issues:** 5
**Governing docs:** Phase-5 m4-003 `DataSideTable` (already has `SectionKind::Rodata` / `SectionKind::Data` but **no** `Bss`); `crates/paideia-as-emitter-elf/src/sections.rs:17` (the section table already lists `.bss`); PaideiaOS `P2-002` (256-entry static cap table); PaideiaOS `pagetables.pdx` (3 × 4 KiB tables, also `.bss` consumers).

`.bss` is the third standard section the ELF emitter needs but cannot produce today. `let mut` doesn't parse, `[u64; 512]` parses (Phase-5 m4-001) but only when paired with an explicit initialiser (m4-002 array literal), and zero-initialised arrays via `[0; 512]` work but each emits 4 KiB of explicit zeros into `.rodata` — wasting binary space and missing the `SHT_NOBITS` discount that real `.bss` provides.

m5 introduces:

1. **`uninit` keyword** — a marker that the right-hand side of a `let` is zero-initialised (semantically: the buffer's bytes are unspecified, but the linker provides them zero per the System-V ABI for `.bss` sections).
2. **`let mut`** — the parser flag indicating mutability; required for `.bss` because `.bss` is read-write per the standard ABI. (Read-only zero buffers don't have a natural home — `.bss` is RW; `.rodata` is RO. Phase 6 takes the pragmatic choice: zero-init buffers go to `.bss`.)
3. **`SectionKind::Bss` variant** plus the emitter wiring that produces `SHT_NOBITS` sections (no payload bytes, but `sh_size` is the buffer's intended runtime size).
4. **`Operand::SymbolRef` resolution for `.bss` symbols** — already works (Phase-5 m5-003 generic SymbolTable), but the integration test covers the round-trip explicitly.

Out of scope: typed mutability checking (the `mut` flag is structural — type-system "you can write here" inference is Phase-4 borrow checker territory, and the build path doesn't re-validate it); custom alignment via `align(N)` annotations (Phase 6+); `static` keyword (Phase 6+, alternative spelling for top-level `let`).

---

### m5-001. parser + ast: `let mut` keyword + `uninit` rhs marker

- **Summary:** Extend the parser to accept `let mut x : T = uninit` and `let mut x : T = expr`. The `mut` token is consumed between `let` and the binding identifier; the binding gains a `mutable: bool` flag on the AST node. `uninit` is a new keyword reserved across the language; in expression position it parses to `ExprData::Uninit` and the elaborator constrains its use to RHS of `let mut x : T = uninit` (any other context is `P0220`).
- **Acceptance criteria:**
  - `let mut x : u64 = 0` parses; AST node has `mutable: true`.
  - `let mut arr : [u64; 512] = uninit` parses; the array-type from Phase-5 m4-001 is reused.
  - `let mut arr : [u64; 512] = [0; 512]` (explicit zero-fill) parses too — both forms accepted; m5-002 routes `uninit` to `.bss`, explicit-zero to `.data` with the value preserved.
  - `let x : u64 = uninit` (immutable + uninit) rejects with `P0220` ("uninit only valid for `let mut`").
  - `let mut x : u64 = uninit` (scalar + uninit) is accepted (lowers to a single 8-byte `.bss` entry).
  - `uninit` in any other position (e.g., `f(uninit)` or `1 + uninit`) emits `P0221` ("uninit not valid in this position").
  - 8 unit tests covering accept / reject permutations.
- **Files:** `crates/paideia-as-parser/src/parse_let.rs`, `crates/paideia-as-parser/src/parse_primary.rs`, `crates/paideia-as-ast/src/exprs.rs`, `crates/paideia-as-lexer/src/keywords.rs`, `crates/paideia-as-diagnostics/catalog.toml` (`P0220`, `P0221`).
- **Dependencies:** none.
- **Estimated size:** S
- **Phase:** Phase 6 — PaideiaOS Phase-2 unblock.
- **Milestone:** `phase-6-bss-arrays`.

---

### m5-002. ir + elaborator: `SectionKind::Bss` variant + `EmitWalker` routes `uninit` to `.bss`

- **Summary:** Add `SectionKind::Bss` to `paideia_as_ir::data::SectionKind`. `EmitWalker` extended: when a module-level `IrKind::Let` has `mutable: true` and its body is `IrKind::Uninit`, it inserts a `DataEntry { section: Bss, bytes: vec![], symbol_name, size_hint: <computed from type>, align: <natural> }`. The `DataEntry` schema gains a `size_hint: u64` field so `.bss` entries (with empty `bytes`) carry their intended size.
- **Acceptance criteria:**
  - `SectionKind` enum gains `Bss` variant.
  - `DataEntry` gains `size_hint: u64`; for `.rodata` / `.data` entries it equals `bytes.len()`; for `.bss` it is computed from the type (e.g., `[u64; 512]` → `8 * 512 = 4096`).
  - `EmitWalker` routes `let mut x : T = uninit` to `Bss` with `bytes: empty`, `size_hint: type_size(T)`.
  - `let mut x : T = literal_expr` (mutable + initialised) routes to `Data` (read-write data; was previously a Phase 6+ deferral).
  - `let x : T = literal_expr` (immutable + initialised) continues to route to `Rodata` per Phase-5 m4-003.
  - 6 unit tests covering the routing decisions: `uninit + mut → Bss`, `expr + mut → Data`, `expr + immut → Rodata`, `uninit + immut → P0220`, `uninit + mut + scalar → Bss (size 8)`, `uninit + mut + array → Bss (size N*elt)`.
- **Files:** `crates/paideia-as-ir/src/data.rs`, `crates/paideia-as-elaborator/src/emit_walker.rs`.
- **Dependencies:** m5-001.
- **Estimated size:** S
- **Phase:** Phase 6 — PaideiaOS Phase-2 unblock.
- **Milestone:** `phase-6-bss-arrays`.

---

### m5-003. emitter-elf: `.bss` section emission with `SHT_NOBITS`

- **Summary:** Extend `crates/paideia-as-emitter-elf/src/sections.rs` to emit a `.bss` section header with `sh_type = SHT_NOBITS` (8), `sh_flags = SHF_ALLOC | SHF_WRITE` (3), `sh_size = sum-of-bss-entry-sizes`, `sh_offset = current-file-position` (but no bytes are written for the section — `SHT_NOBITS` skips the file payload). Section symbols for `.bss` entries get the right `st_shndx` (the `.bss` section index) and `st_value` = offset within `.bss`.
- **Acceptance criteria:**
  - `readelf -S <object>` shows a `.bss` section with `Type: NOBITS`, `Flags: WA`, `Size: <sum>`, `Off: <where-it-would-be>` (but the file is no larger than it would be without the `.bss` payload).
  - `readelf -s <object>` shows each `.bss` symbol with `Ndx: <.bss-idx>` and `Value: <offset>`.
  - When linked via `ld`, the resulting executable's `.bss` is allocated zero-filled at runtime (verified via `readelf -l <executable>` showing the segment's `FileSize < MemSize`).
  - For an object with no `.bss` entries, the `.bss` section is omitted entirely (no zero-size NOBITS section).
  - 4 integration tests: (1) single u64 in `.bss`, (2) `[u64; 512]` in `.bss` — file size unchanged, `sh_size` = 4096, (3) three `.bss` symbols at expected offsets, (4) mixed `.rodata` + `.bss` object linked + run sanity (no segfault).
- **Files:** `crates/paideia-as-emitter-elf/src/sections.rs`, `crates/paideia-as-emitter-elf/src/writer.rs`, `crates/paideia-as/tests/bss_emission.rs`.
- **Dependencies:** m5-002.
- **Estimated size:** S
- **Phase:** Phase 6 — PaideiaOS Phase-2 unblock.
- **Milestone:** `phase-6-bss-arrays`.

---

### m5-004. emitter-elf: relocations against `.bss` symbols work end-to-end

- **Summary:** When an instruction's `Operand::SymbolRef("cap_table")` references a `.bss` symbol, the Phase-5 m4-004 relocation pipeline must produce a relocation against the right symbol-table entry (the one created by m5-003). Phase-5's generic SymbolTable already supports `.bss` symbols structurally, but the integration test that exercises the round trip does not yet exist; m5-004 closes the test gap and surfaces any bugs in the symbol-section-index linkage.
- **Acceptance criteria:**
  - A `.pdx` source declaring `let mut cap_table : [u64; 256 * 4] = uninit` (256 entries × 4 u64 fields = 8192 bytes) plus a function that does `lea rax, [cap_table + 32]` (or, until LEA-symbolref lands in Phase 6+, `mov rax, cap_table; add rax, 32`) builds + links + the resolved address points into `.bss`.
  - `objdump -d <linked-output>` shows the `lea`'s displacement resolved to the `.bss`-relative offset.
  - `readelf -r <object>` shows the relocation against the `.bss` symbol.
  - Test fixture: `tests/build-emit/cap_table_addr.pdx` — extracts the first 4 bytes of `.bss` content (which should be 0x00 since `.bss` is zero-init), confirms the cap_table symbol resolves to the right runtime address.
  - 3 integration tests: (1) reloc-against-bss-symbol exists, (2) link succeeds, (3) loaded executable's `.bss` is zero-initialised at runtime per ELF ABI.
- **Files:** `crates/paideia-as-emitter-elf/src/relocs.rs`, `tests/build-emit/cap_table_addr.pdx`, `crates/paideia-as/tests/build_emit_bss_reloc.rs`.
- **Dependencies:** m5-003, m4-005.
- **Estimated size:** S
- **Phase:** Phase 6 — PaideiaOS Phase-2 unblock.
- **Milestone:** `phase-6-bss-arrays`.

---

### m5-005. tests: PaideiaOS `pagetables.pdx` rebuilds with `.bss` arrays

- **Summary:** Re-enable the PaideiaOS `pagetables.pdx` file in the m1-006 regression suite. The Phase-1 stub declared only anchor qwords; with m5 shipped, the stub can be rewritten upstream to declare `let mut pml4 : [u64; 512] = uninit` plus PDPT + PD tables. m5-005 patches the PaideiaOS source (or coordinates a sibling PR in the PaideiaOS repo) and re-enables the `pagetables.pdx` build in the regression suite.
- **Acceptance criteria:**
  - PaideiaOS `src/kernel/boot/pagetables.pdx` is rewritten to use `let mut pml4 : [u64; 512] = uninit; let mut pdpt : [u64; 512] = uninit; let mut pd : [u64; 512] = uninit` (12 KiB total in `.bss`).
  - The paideia-as-side regression suite (`paideia_os_phase1_rebuild.rs` from m1-006) includes `pagetables.pdx` and asserts the build exits 0 + the resulting `.o` has a `.bss` section of size 12288 bytes (3 × 4096).
  - The PaideiaOS-side PR is filed and linked from the paideia-as Phase 6 commit; the cross-repo coordination protocol matches the Phase-5 m6-005 closure pattern.
  - The boot-side init code that zeros the tables (the existing `zero_bss.pdx`'s `rep_stosq` call) continues to work — `.bss` provides the zero bytes; the init code is now a no-op, but kept as a defensive pattern for early-boot determinism.
- **Files:** `crates/paideia-as/tests/paideia_os_phase1_rebuild.rs` (re-include), `PaideiaOS/src/kernel/boot/pagetables.pdx` (cross-repo PR).
- **Dependencies:** m5-002, m5-003, m1-006.
- **Estimated size:** XS
- **Phase:** Phase 6 — PaideiaOS Phase-2 unblock.
- **Milestone:** `phase-6-bss-arrays`.

---

## 8. Milestone m6 — End-to-end smoke (PaideiaOS Phase-2 unblock)

**Slug:** `phase-6-end-to-end-smoke`
**Issues:** 4
**Governing docs:** `PaideiaOS/.plans/paideia-os-osarch-plan.md` §P2 (capability system tracks); `design/capabilities/rights-catalog.md` (rights bitmap definitions); the Phase-5 m6 closure pattern.

m6 proves the chain works for PaideiaOS Phase 2. The minimal cap-system fixture exercises the four capabilities m3 / m4 / m5 unlock:

1. Declare `struct Capability` (m3 — record layout).
2. Declare `let mut cap_table : [u64; 1024] = uninit` (m5 — `.bss` array, 8 KiB for 256 caps × 4 u64s).
3. `let cap_alloc : () -> *Capability = fn () -> unsafe { ..., block: { ... } }` — returns a pointer into `cap_table` using `lea`-or-arithmetic.
4. `let cap_verify : (*Capability, u64) -> u64 = fn (p, gen) -> ...` — reads `(*p).generation`, compares (m4 `cmp`), branches (m4 `jne`), returns 1 or 0.
5. `let cap_mint : (u64, u64, u64) -> *Capability = fn (k, t, r) -> unsafe { ..., block: { call cap_alloc; ...field stores... ; ret } }` — uses the m4-005 `call sym` shape.

By the end of m6, this fixture:

1. Builds with `paideia-as build --emit elf64 cap_smoke.pdx -o cap_smoke.o`.
2. Links with `ld -T link.ld cap_smoke.o -o cap_smoke` (with a Phase-1-style harness providing `_start`).
3. Runs (smoke-only, no QEMU required — the harness is a userspace test ELF since cap-system code is OS-agnostic in isolation).
4. Returns the expected bit-pattern from `cap_verify(cap_mint(...), gen)`.

This is **the test that proves PaideiaOS Phase 2 is unblocked**.

---

### m6-001. fixtures: `tests/build-emit/cap_smoke.pdx` source

- **Summary:** Author the minimal `.pdx` source that exercises every m3 / m4 / m5 capability needed by PaideiaOS Phase 2. The source compiles to a userspace test ELF — no GDT / IDT / long-mode setup, just a `_start` that calls `cap_mint`, then `cap_verify`, then exits with the result.
- **Acceptance criteria:**
  - `tests/build-emit/cap_smoke.pdx` exists, < 80 lines including comments.
  - Declares `struct Capability { kind: u64, target: u64, rights: u64, generation: u64 }`.
  - Declares `let mut cap_table : [u64; 1024] = uninit` (8 KiB `.bss`).
  - Declares `let cap_alloc : () -> *Capability`, `let cap_verify : (*Capability, u64) -> u64`, `let cap_mint : (u64, u64, u64) -> *Capability`.
  - Declares `let _start : () -> () = fn () -> unsafe { ..., block: { call cap_mint; mov rdi, rax; mov rsi, 0; call cap_verify; mov rdi, rax; mov rax, 60; syscall } }` (exit(2) on Linux ABI; uses Mnemonic::Syscall — **added if missing as part of this issue**).
  - `paideia-as check tests/build-emit/cap_smoke.pdx` exits 0 with no diagnostics.
  - Companion `tests/build-emit/cap_smoke.expected_bytes.txt` records the expected `.text` byte sequence per function for snapshot comparison.
- **Files:** `tests/build-emit/cap_smoke.pdx`, `tests/build-emit/cap_smoke.expected_bytes.txt`, optionally `crates/paideia-as-encoder/src/encode.rs` (if `syscall` not yet encoded — adds `0F 05`).
- **Dependencies:** m3-008, m4-006, m5-004.
- **Estimated size:** S
- **Phase:** Phase 6 — PaideiaOS Phase-2 unblock.
- **Milestone:** `phase-6-end-to-end-smoke`.

---

### m6-002. fixtures: `tests/build-emit/cap_smoke.link.ld` + harness driver

- **Summary:** Linker script for the userspace test ELF (statically linked, no glibc) plus a shell driver that builds + links + runs + checks the process exit code. Uses Linux ABI `syscall` for exit so no runtime is needed. Smoke runs in `<200 ms` per invocation, fast enough for CI.
- **Acceptance criteria:**
  - `tests/build-emit/cap_smoke.link.ld` sets `OUTPUT_FORMAT(elf64-x86-64)`, `ENTRY(_start)`, `.text` at `0x400000`, `.bss` at `0x600000`.
  - `tools/run-cap-smoke.sh`: builds, links, runs, asserts exit code equals the expected `cap_verify` return value (1 for the happy path).
  - Linux-only (skipped on macOS/Windows via OS-detection).
  - 5-second timeout enforced.
- **Files:** `tests/build-emit/cap_smoke.link.ld`, `tools/run-cap-smoke.sh`.
- **Dependencies:** m6-001.
- **Estimated size:** XS
- **Phase:** Phase 6 — PaideiaOS Phase-2 unblock.
- **Milestone:** `phase-6-end-to-end-smoke`.

---

### m6-003. tests: byte-sequence + reloc-table assertion for `cap_smoke.pdx`

- **Summary:** A Rust integration test under `crates/paideia-as/tests/build_emit_cap_smoke.rs` that builds the fixture programmatically, extracts the per-function `.text` bytes, the `.bss` size, the symbol table, and the relocation entries, and asserts each matches a snapshot. This is the deterministic regression test that catches encoder + emitter regressions without needing the runtime smoke.
- **Acceptance criteria:**
  - Test invokes `paideia-as::cmd_build::run` on `cap_smoke.pdx`, asserts exit 0.
  - Reads the resulting `.o` via the `object` crate, asserts:
    - `.text` per-function byte snapshots match (5 functions: `_start`, `cap_alloc`, `cap_verify`, `cap_mint`, plus the harness).
    - `.bss` has `sh_size = 8192` (`cap_table`).
    - Symbol table contains `Capability` not (records have no runtime symbol), `cap_table` (STT_OBJECT in .bss), `cap_alloc` / `cap_verify` / `cap_mint` / `_start` (STT_FUNC, all global).
    - Relocations present: at least 3 (cap_mint → cap_alloc; cap_verify's `lea` against cap_table — or the equivalent `mov rax, cap_table; ...` sequence; _start → cap_mint).
  - Snapshot file `cap_smoke.expected_objdump.txt` matches `objdump -d` output line-for-line (minus addresses).
- **Files:** `crates/paideia-as/tests/build_emit_cap_smoke.rs`, `tests/build-emit/cap_smoke.expected_objdump.txt`.
- **Dependencies:** m6-001.
- **Estimated size:** S
- **Phase:** Phase 6 — PaideiaOS Phase-2 unblock.
- **Milestone:** `phase-6-end-to-end-smoke`.

---

### m6-004. tests: runtime smoke + PaideiaOS Phase-2 unblock marker

- **Summary:** A Rust integration test that shells out to `tools/run-cap-smoke.sh` and asserts the process exits 1 (the expected `cap_verify` success value). Skipped on non-Linux hosts. The closing commit message of this issue declares the PaideiaOS Phase-2 unblock explicitly, mirroring the Phase-5 m6-005 marker.
- **Acceptance criteria:**
  - Test file `crates/paideia-as/tests/cap_smoke_runtime.rs` exists.
  - On Linux, the test passes within 5 seconds (the fixture's runtime is < 1 ms; the budget covers build + link).
  - On non-Linux, the test is auto-skipped (prints "cap_smoke runtime test is Linux-only; skipping" and returns).
  - The Nix flake's `devShell` already provides `ld` and `bash`; no flake changes expected.
  - **PaideiaOS Phase-2 unblock declared:** the m6-004 commit message states "PaideiaOS Phase-2 (P2-001..P2-024) is now unblocked: paideia-as build emits struct field access, `.bss` arrays, `cmp`/`jcc`/`call sym` inside unsafe blocks, and a userspace cap-system smoke passes end-to-end." A cross-repo reference to the unblock is written into `PaideiaOS/.plans/issue-map.tsv` (entry: `phase-6-m6-004 unblocks P2-001`).
- **Files:** `crates/paideia-as/tests/cap_smoke_runtime.rs`, cross-repo update to `PaideiaOS/.plans/issue-map.tsv`.
- **Dependencies:** m6-002, m6-003.
- **Estimated size:** XS
- **Phase:** Phase 6 — PaideiaOS Phase-2 unblock.
- **Milestone:** `phase-6-end-to-end-smoke`.

---

## 9. Milestone m7 — Documentation + closure

**Slug:** `phase-6-docs-closure`
**Issues:** 4
**Governing docs:** the existing closure pattern from Phase 5 m7-001..004.

---

### m7-001. docs: `design/toolchain/phase-transition-6.md` retrospective

- **Summary:** Author the Phase 6 retrospective in the same shape as `phase-transition-5.md`. Sections: §0 scope (cross-repo unblock for PaideiaOS Phase 2); §1 carryover disposition from Phase 5 (none expected — Phase 5 closed cleanly); §2 honest list (what didn't ship: string literals, typed loops in build, `let mut` for non-`.bss`, record-by-value, generic record-field-access in non-Deref(Var) shape); §3 right calls (the operand-shape dispatcher in m1-001 paid for itself across m1-002/m1-003; the explicit "no records-as-values" boundary kept m3 to 8 issues); §4 changes (in hindsight); §5 Phase-7 carryover (self-hosting from `self-hosting-phase5-plan.md`; string-literal surface from this Phase 6 deferral list; `let mut` mutability checking in the borrow chain; record-by-value).
- **Acceptance criteria:**
  - `design/toolchain/phase-transition-6.md` exists, < 300 lines.
  - Contains the standard sections (§0..§5).
  - The Phase-7 carryover list contains self-hosting + the four named Phase 6+ deferrals (string lits, typed loops, mut+borrows, record-by-value).
  - The honest list documents which surface features still don't reach `build` (per §1 of this plan) and the rationale (Phase 2 doesn't consume them).
  - Cross-references `PaideiaOS/.plans/paideia-os-osarch-plan.md` §P2 as the unblock target.
- **Files:** `design/toolchain/phase-transition-6.md`.
- **Dependencies:** m6-004.
- **Estimated size:** XS
- **Phase:** Phase 6 — PaideiaOS Phase-2 unblock.
- **Milestone:** `phase-6-docs-closure`.

---

### m7-002. docs: STATUS.md Phase 6 closure section

- **Summary:** Prepend a Phase 6 closure section to `STATUS.md` (matching the Phase 5 m7-002 pattern). Lists each m1..m7 milestone, the issues that closed it, and the workspace-test count delta vs Phase 5 close (2416 tests).
- **Acceptance criteria:**
  - `STATUS.md` gains "Phase 6 milestone closure (m1–m7)" section above the Phase 5 section.
  - Each milestone has a one-line summary plus the list of issue IDs.
  - The "Workspace test totals" table grows a Phase-6-close row with the new test count.
  - The "Where to look next" section adds `design/toolchain/phase-transition-6.md` + the cap-smoke fixture path.
- **Files:** `STATUS.md`.
- **Dependencies:** m7-001.
- **Estimated size:** XS
- **Phase:** Phase 6 — PaideiaOS Phase-2 unblock.
- **Milestone:** `phase-6-docs-closure`.

---

### m7-003. release: v0.6.0 tag + CHANGELOG Phase 6 section

- **Summary:** Bump the workspace version from `0.5.0` to `0.6.0`; author the CHANGELOG section listing the operand-shape dispatch fix, the parser cleanups, the struct walker activation, the control-flow encoders, the `.bss` arrays surface, and the PaideiaOS Phase-2 unblock. Tag the commit as `v0.6.0`.
- **Acceptance criteria:**
  - `Cargo.toml` workspace `version = "0.6.0"`.
  - `CHANGELOG.md` gains a `## [0.6.0] — 2026-MM-DD` section listing new capabilities and deferred items (string lits → Phase 7, etc.).
  - `git tag v0.6.0 <closure-sha>` exists locally; pushed in the same commit window.
  - `cargo build --workspace` clean post-bump.
  - The Phase-6 issue map (`.plans/phase-6-issue-map.tsv`) maps each m1..m7 issue ID to its closing PR/commit.
- **Files:** `Cargo.toml`, `CHANGELOG.md`, `.plans/phase-6-issue-map.tsv`.
- **Dependencies:** m7-002.
- **Estimated size:** XS
- **Phase:** Phase 6 — PaideiaOS Phase-2 unblock.
- **Milestone:** `phase-6-docs-closure`.

---

### m7-004. examples + PaideiaOS-side rewrites: walk away from the `fn (x: ())` workaround

- **Summary:** Confirm each `examples/*.pdx` continues to `check` + `build` clean after the m2-001 parser change; spot-check the 3 build-clean examples from Phase 5 m7-004 (`01_hello.pdx`, `02_functions.pdx`, `15_unsafe.pdx`) for byte-identical output. Coordinate the PaideiaOS-side PR that strips the `fn (x: ())` workaround from every kernel stub. Bundle the PaideiaOS-side simplifications (kernel_main using `call uart_init`, uart_putc using `cmp`/`jne`/polling) into the same cross-repo coordination window — these are the consumer-side proofs of m4-003 / m4-005.
- **Acceptance criteria:**
  - `examples/01_hello.pdx`, `examples/02_functions.pdx`, `examples/15_unsafe.pdx` build clean post-Phase-6 with byte-identical output to Phase 5.
  - PaideiaOS-side cross-repo PR strips `fn (x: ())` workaround from every kernel stub (7 files: `entry.pdx`, `gdt.pdx`, `long_mode.pdx`, `uart.pdx`, `zero_bss.pdx`, `kernel_main.pdx`, `pagetables.pdx`); the resulting stubs re-build clean against the new paideia-as.
  - PaideiaOS-side: `kernel_main.pdx` is rewritten to use `call uart_init` + a 7-call orchestration sequence; the resulting `.text` is non-stub.
  - PaideiaOS-side: `uart_putc` is rewritten with the real polling loop (`cmp [rdi + 5], 0x20; je poll_top; out_al rax`); the resulting `.text` is non-stub.
  - `examples/README.md` gains a per-example status table delta vs Phase 5 (no examples regress; 0 examples newly build-clean — Phase 6 surface is consumed by PaideiaOS, not by the examples corpus).
  - `tests/examples-corpus.rs` continues to pass.
- **Files:** `examples/*.pdx` (no content changes expected), `examples/README.md`, `tests/examples-corpus.rs`; PaideiaOS-side: `PaideiaOS/src/kernel/boot/*.pdx` (cross-repo PR).
- **Dependencies:** m6-004, m2-001.
- **Estimated size:** S
- **Phase:** Phase 6 — PaideiaOS Phase-2 unblock.
- **Milestone:** `phase-6-docs-closure`.

---

## 10. Dependency graph (textual)

```text
  m1:  m1-001 → m1-002 → m1-004 → m1-006
                m1-001 → m1-003 → m1-004
       m1-005 (independent of m1-001..004) ─→ m1-006

  m2:  m2-001 (independent) — gates m7-004's PaideiaOS rewrites
       m2-002, m2-003, m2-004 (all independent leaves)

  m3:  m3-001 → m3-002 → m3-003
                m3-002 → m3-004
                m3-002 → m3-005
       m3-001 → m3-006
       m3-001 → m3-007
       (m3-002 + m3-004 + m3-005 + m3-007) → m3-008

  m4:  m4-001 (independent leaf — cmp encoder)
       m4-002 → m4-003 → m4-004 (label fixup chain)
       m4-005 (independent — consumes Phase-5 m5-002 SymbolRef)
       (m4-001 + m4-003 + m4-004 + m4-005) → m4-006

  m5:  m5-001 → m5-002 → m5-003
                          m5-003 → m5-004 (depends also on m4-005)
       (m5-002 + m5-003 + m1-006) → m5-005

  m6:  (m3-008 + m4-006 + m5-004) → m6-001 → m6-002 → m6-004
       m6-001 → m6-003 → m6-004

  m7:  m6-004 → m7-001 → m7-002 → m7-003
       (m6-004 + m2-001) → m7-004
```

**Critical path (16 issues):**

```text
m1-001 → m1-002 → m1-005 → m3-001 → m3-002 → m3-003 → m3-005 → m3-007 →
m5-001 → m5-002 → m5-004 → m6-001 → m6-003 → m6-004 → m7-001 → m7-003
```

At a sustainable cadence of ~2 issues/week (matching the Phase-5 baseline of 38 issues in 19 weeks), the wall-clock floor is **8 weeks**; parallel work on the non-critical sub-tracks (m1-003/004/006, m2 entirely, m3-004/006/008, m4 entirely, m5-005, m6-002, m7-002/004) compresses real-world delivery into **5–6 weeks** for a solo developer.

---

## 11. Parallel substacks

Three substacks can land concurrently with the critical path:

**Substack A — m4 (control-flow encoders):**
m4-001 (cmp) + m4-002→m4-003→m4-004 (label-fixup chain) + m4-005 (call sym) + m4-006 (corpus). Each is a leaf relative to m3. A second developer can carry this substack across the entire Phase 6 window — it lands by m4-006 ≈ week 4–5. PaideiaOS-side `uart_putc` polling loop unblocks here.

**Substack B — m2 (parser cleanups):**
m2-001..004, all leaves, no inter-dependencies. Trivial PRs (XS each). Can land in week 1 with the m1 fixes, or batched in a single sweep PR if preferred.

**Substack C — m3-006 + m1-006:**
The non-functional plumbing (note section + regression suite). m3-006 (`.note.paideia` layouts) blocks nothing in Phase 6 but is wanted before PaideiaOS Phase 2 consumes layouts via tooling. m1-006 (PaideiaOS rebuild regression) lands in week 2 after m1-005 + m1-002.

---

## 12. Phase 6 → Phase 7 carryover

Phase 7 inherits the original Phase 5 self-hosting plan (`design/toolchain/self-hosting-phase5-plan.md`, T1/T2/T3 tiers, 93k LoC, 21 crates) plus the four named Phase 6 deferrals:

1. **String-literal surface for `*u8` references.** `let s : *u8 = "PaideiaOS Phase 1\n"` → `.rodata` emission + pointer init. Consumer-driven by PaideiaOS Phase 3 (IPC tracing) and the banner-text completion.
2. **Typed `for` / `while` / `loop` in the build path.** Phase-4 m8 ships `LoopMetaTable` at the side-table level; activation in `EmitWalker` requires register-allocation per loop body. Consumer-driven by PaideiaOS Phase 4 (scheduler).
3. **`let mut` mutability checking integrated with the borrow checker.** Phase-4 m6 ships the borrow chain for immutable refs; the `let mut` flag in Phase 6 m5-001 is structural-only and not yet checked. Consumer-driven by PaideiaOS Phase 4 (scheduler) where mutability is rampant.
4. **Records-by-value (RecordCons returning by value, record args by value).** Phase 6 limits records to `*T`-addressed forms. PaideiaOS Phase 3 IPC messages and capability-pass-by-value to user-space gate on this.

Beyond these four, Phase 7 also subsumes any items that surface during PaideiaOS Phase 2 implementation. The pattern from Phase 5 → Phase 6 — file a paideia-as bug from PaideiaOS, plan the fix in the next phase — continues. The Phase 6 m1-005 strict-mode flag was the key Phase 5 lesson: silent fallback on encoder errors hid #734 for the entire Phase 1 boot work. Phase 6 makes the strict mode the default; Phase 7 inherits a louder failure surface, which should keep the file-bug-from-consumer loop short.

---

## 13. Unblock criterion

The single milestone that unblocks PaideiaOS Phase 2 (`PaideiaOS/.plans/paideia-os-osarch-plan.md` tasks `P2-001..P2-024`) is **m6 — End-to-end smoke**, specifically **m6-004**.

Why m6 and not earlier:

- m1 alone fixes the dispatch and the silent fallback but doesn't reach struct fields or control flow.
- m2 alone clears the parser papercuts but is cosmetic for unblock purposes.
- m3 alone wires record-field codegen but the verifier still can't branch (no `cmp`/`jne`).
- m4 alone covers control flow but record reads (`(*p).generation`) don't lower.
- m5 alone emits `.bss` but no code can address a `.bss` symbol with the right operand shape (m4-005's `SymbolRef`-in-call is needed for the slab allocator's address-of-table).

Only at m6-004 does the chain prove end-to-end: a cap-system fixture allocates from a `.bss` table, mints a descriptor, reads its fields, branches on a comparison, and returns the right exit code. **The commit message for m6-004 explicitly declares the PaideiaOS Phase 2 unblock**, and `PaideiaOS/.plans/issue-map.tsv` records the cross-repo dependency closure (`phase-6-m6-004 unblocks P2-001`).

P2-001..P2-024 then proceeds in PaideiaOS's own milestone cadence; the paideia-as side is done for Phase 2 consumers. PaideiaOS Phase 3 (IPC) and Phase 4 (scheduler) re-open paideia-as Phase 7 with the deferrals from §12.

---

## 14. Notes on what is deliberately deferred

The following are *not* in this plan, in keeping with the "stop the moment PaideiaOS Phase 2 unblocks" constraint:

- **Self-hosting** — Phase 7. `self-hosting-phase5-plan.md` remains the blueprint.
- **String literals as `*u8`** — Phase 6+, opens with PaideiaOS Phase 3 (IPC tracing) or banner completion. Banner aesthetics is not Phase 2 critical-path.
- **Typed loops in build** — Phase 6+, opens with PaideiaOS Phase 4 (scheduler).
- **`let mut` borrow chain** — Phase 6+, opens with PaideiaOS Phase 4 (mutable scheduler state).
- **Records-by-value** — Phase 6+, opens with PaideiaOS Phase 3 (IPC messages).
- **Generic / trait / enum activation in build** — Phase 6+ across multiple consumers; no single Phase 2 task demands them.
- **DWARF emission in ELF** — Phase 6+; `.note.paideia` (m3-006) provides a stop-gap for cap-system tooling.
- **RIP-relative LEA for symbol-as-address (`lea rax, [cap_table]`)** — Phase 6+. Phase 6 uses the `mov rax, cap_table` shape (sym in mov src operand) via m4-005-equivalent dispatch; cleaner LEA-symref form is a one-issue Phase 7 add-on.
- **Backward-label spin loops in `unsafe`** — m4-003 partially supports backward labels (the offset computation handles both directions) but `Mnemonic::Loop` / `Loope` / `Loopne` are not added; PaideiaOS Phase 2 has no spin loops. Adds in Phase 6+ when scheduler / IPC consume them.
- **`align(N)` annotation for `.bss` / `.data` symbols** — Phase 6+. Phase 6 uses natural alignment (8 for u64 fields, 4096 for `[u64; 512]` only because the symbol happens to land at a 4 KiB boundary — relies on the linker script). PaideiaOS page-table layout is alignment-sensitive and may need this earlier than Phase 7; flagged for re-evaluation if P2-002 work surfaces an alignment issue.
- **PE/COFF + PAX activation for new mnemonics** — they consume the same encoder bridge as ELF, so no new emitter work is needed for them to acquire the Phase 6 mnemonic surface; explicit testing is Phase 6+ (PE/PAX consumers are not on the Phase 2 path).

The 37 issues above are the smallest disciplined sequence that unblocks PaideiaOS Phase 2 without overshooting into Phase 7 territory.
