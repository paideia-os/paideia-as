# paideia-as Phase 5 — Build-emit activation plan (osarch)

**Author:** osarch agent
**Date:** 2026-06-20
**Repo:** `paideia-os/paideia-as` (workspace at `/home/snunez/Development/paideia-as/`)
**Scope:** Make `paideia-as build --emit elf64 some.pdx` produce machine code that QEMU can boot for a PaideiaOS Phase-1 kernel — instead of the placeholder `lea 0x1(%rdi), %rax ; ret` (5 bytes) the emitter ships today.
**Companion:** The original Phase 5 self-hosting plan in `design/toolchain/self-hosting-phase5-plan.md` is **deferred** to Phase 6+. PaideiaOS Phase-1 (P1-001..014) is the gating consumer of this Phase 5; m6 below is its unblocking milestone.

---

## 0. Why this plan exists

Phase 4 m14-001 retrospective recorded the gap explicitly:

> paideia-as build end-to-end for the new surface activates per-example as the elaborator chokepoints close. Today most examples pass-clean via `check` but `build` requires m1 walker-chain activation (per-pass).

That activation has not happened. Empirical verification on 2026-06-20 across three `.pdx` files — `add_one`, a 5-quadword `let : u64` constant, and an `unsafe { block: { lgdt [rdi] } }` block — all three produced byte-identical `48 8d 47 01 c3` (5 bytes). The user content never reaches the binary.

Root cause: `crates/paideia-as/src/cmd_build.rs::build_elf_object()` calls `lower_add_one(&mut buf)` unconditionally and ignores the lowered IR entirely. The `InstructionSideTable` (m2-001) and the populate path (m2-003) exist; the encoder bridge (m2-002) maps `Mnemonic → bytes`; but:

1. **No path consumes `InstructionSideTable` for ELF emission.** PE/COFF does (`emit_text_from_instructions`); ELF does not.
2. **The IR walker does not lower `IrKind::Let`, `IrKind::Lambda`, or `IrKind::Unsafe` body content into real `Instruction` entries.** Phase-4 m1-005/006 wired the walkers to populate `PositionIndex` and `NameResolutionTable`, but the IR→bytes side of the chain stops at the side-table population: lambda bodies, `let : T = literal` declarations, and `unsafe { block: { ... } }` payloads never become entries in `InstructionSideTable`.
3. **The `Mnemonic` enum is closed at 10 entries** (`Mov, Add, Sub, Cmp, Jcc(_), Jmp, Call, Ret, RepMovsb, Lea`). The PaideiaOS Phase-1 boot path needs another ~20: `lgdt, lidt, mov cr*, mov dr*, wrmsr, rdmsr, in, out, iret, iretq, sysret, swapgs, cpuid, cli, sti, hlt, int N`.
4. **The parsed instruction stream inside `unsafe { block: { ... } }` is held only as AST children** (see `crates/paideia-as-parser/src/parse_unsafe.rs`); the AST→IR lowerer maps the whole node to `IrKind::Unsafe` and discards the inner mnemonic sequence.
5. **`let x : u64 = 0x...` does not contribute to `.data`/`.rodata`.** Initialised static data has no surface activation today.
6. **The single emitted symbol is `"add_one"` hard-coded in `cmd_build.rs`.** No function-name extraction, no `_start` export, no cross-file calls.

Phase 5 closes these six gaps in milestone order. Self-hosting (the original Phase 5) is explicitly deferred.

---

## 1. Scope boundary (what is NOT in scope)

The user has bounded Phase 5 strictly. Stop the moment a `.pdx` source the size of PaideiaOS P1-001..010 can be assembled, linked, and QEMU-booted to the UART banner. Everything below is deferred to **Phase 6+**:

- **Self-hosting of any paideia-as crate** (`design/toolchain/self-hosting-phase5-plan.md` tiers T1/T2/T3 — 93k LoC, 21 crates). Moves to Phase 6.
- **Full walker-chain activation for the Phase-4 surface** (records / generics / borrowed-refs / stdlib types in `build`). Only what PaideiaOS Phase-1 lowers is in scope. Records and generics are not used in P1.
- **Optimisation passes in the build path** (peephole / DSE / unroll / encode-tight / macro-fusion / branch-hint / align / pool-constants). The m3-007 → m1-007..010 flip already shipped at the side-table level; their build-emit activation is Phase 6+.
- **DWARF emission in the ELF path.** The DWARF crate exists (Phase-1 PR 57); wiring it into `--emit elf64` is Phase 6+. PaideiaOS Phase-1 boot does not need source-level debugging.
- **PE/COFF and PAX activation.** PE already emits `InstructionSideTable` via `emit_text_from_instructions`; PAX consumes content hashes; both are correct-as-is for their phase. ELF parity with PE is the focus.
- **Effect-handler runtime materialisation.** `unsafe { effects: {...} }` annotations stay declared-not-checked through Phase 5; the PR-51 effect rewrite operates on `IrPerform` nodes only, which boot code does not use.
- **Linker integration.** The emitted `.o` must be linkable by GNU `ld` (already true for the `add_one` stub). Multi-object linking via `paideia-as-linker` stays out of `build`'s critical path; the PaideiaOS Makefile invokes `ld` directly.
- **Loops, generics, traits, records, enums, borrowed references in the build path.** PaideiaOS Phase-1 uses none. Their build-emit activation is Phase 6+.

---

## 2. Milestone index

Seven milestones, ~38 issues. Mean size **S**. No `L` tasks; the two largest (m2-encoder family, m3-unsafe-walker) are decomposed per-instruction or per-operand-shape.

| #  | Milestone slug                | Description                                                  | Issues | Critical path |
|----|-------------------------------|--------------------------------------------------------------|--------|---------------|
| m1 | `phase-5-elab-lowering`       | Per-construct IR lowering for `let`/`fn`/`unsafe`-payload.   | 5      | yes           |
| m2 | `phase-5-encoder-boot-isa`    | x86_64 encoder coverage for PaideiaOS Phase-1 boot ISA.      | 10     | partial       |
| m3 | `phase-5-unsafe-walker`       | Walker that consumes `unsafe { block: }` AST → IR → bytes.   | 5      | yes           |
| m4 | `phase-5-static-data`         | `.data` / `.rodata` emission for `let : T = literal` items.  | 4      | partial       |
| m5 | `phase-5-symbols-relocs`      | Symbol export + cross-file relocations through the linker.   | 5      | yes           |
| m6 | `phase-5-end-to-end-smoke`    | A `.pdx` source assembles, links, QEMU-boots, writes "x".    | 5      | yes (closure) |
| m7 | `phase-5-docs-closure`        | Retrospective, STATUS.md, v0.5.0 tag, examples updates.      | 4      | no            |
|    | **Σ**                         |                                                              | **38** |               |

**Critical path** (longest dependency chain through the milestones): m1-001 → m1-002 → m3-001 → m3-002 → m3-003 → m2-001 → m2-008 → m4-001 → m4-003 → m5-001 → m5-003 → m5-005 → m6-001 → m6-002 → m6-003 → m6-005 → m7-001 → m7-003 = **18 issues**.

**PaideiaOS Phase-1 unblock:** **m6 close (m6-005)**. PaideiaOS P1-001..010 are blocked on `paideia-as build --emit elf64` producing real machine code; m6's QEMU-bootable smoke is the proof that this works. m1..m5 are necessary stages; m6 is the gate.

The two parallelisable sub-tracks:

- **m2 (encoder boot ISA)** is mostly independent of m1/m3. Per-instruction issues can land in parallel with m1 once m1-001 has clarified the IR side-table shape. Only m2-001 sits on the critical path (it provides the dispatch entry point); m2-002..010 are leaf encoder work.
- **m4 (static data)** is independent of m3 (unsafe walker); both consume the same `InstructionSideTable` plumbing from m1. m4 and m3 can land in either order after m1-002.

---

## 3. Milestone m1 — Elaborator per-construct lowering

**Slug:** `phase-5-elab-lowering`
**Issues:** 5
**Governing docs:** `design/toolchain/walker-hookups-phase4.md` (m1 walker surface and populate-path pattern); `design/toolchain/per-node-ir-payload-phase3.md` (`InstructionSideTable` shape); `crates/paideia-as-elaborator/src/lower.rs` (current structural-only mapping table).

The walker chain from Phase-4 m1-005/006 ships unit-tested in `LinearityWalker`, `EffectRowWalker`, `CapWalker` and the four `*Walker` instances under `paideia-as-elaborator/src/borrow_walker.rs` etc. None of them populate `InstructionSideTable` for the three minimal constructs PaideiaOS Phase-1 needs: `let : u64 = literal`, `fn (...) -> expr`, and `unsafe { block: { ... } }`. m1 fills exactly this gap and no more.

The minimum surface this milestone covers, in increasing order of difficulty:

```text
let answer : u64 = 42                                      // m1-001
let add_one : (u64) -> u64 = fn (x : u64) -> x + 1        // m1-002, m1-003
let kernel_main_64 : () -> () !{sysreg} @{} =             // m1-004
  fn () -> unsafe {
    effects: { sysreg },
    capabilities: { },
    justification: "long-mode entry per Intel SDM Vol 3A §9.8.5",
    block: { /* opaque, lowered by m3 */ }
  }
```

The walker chain stays the existing one; what changes is that the walker now populates `InstructionSideTable` for those three node kinds. Phase-4 m1-005 introduced the convention of `WalkerPassState` + interior-mutable side-table writers (see `crates/paideia-as-elaborator/src/walker_pass_state.rs`); m1 issues here extend it.

---

### m1-001. elaborator: `EmitWalker` skeleton + `EmitPassState` side-table writer

- **Summary:** Introduce a new walker, `EmitWalker`, whose job is to populate `InstructionSideTable` for the three Phase-5 lowering shapes. Owns the entry into the emit-side of the pipeline; per-construct logic lands in m1-002..004.
- **Acceptance criteria:** task closed when
  - `crates/paideia-as-elaborator/src/emit_walker.rs` defines `pub struct EmitWalker { pass_state: EmitPassState }`.
  - `EmitPassState` exposes `instructions: &mut InstructionSideTable` plus `current_function: Option<IrNodeId>` and `current_offset: u64`.
  - `impl IrWalker for EmitWalker` provides `enter_node` / `exit_node` stubs that match the four populate-path patterns from `walker-hookups-phase4.md` §1.1–§1.4 (Call / Match / Handle / Branch).
  - The walker is exported from `paideia-as-elaborator/src/lib.rs` alongside `LinearityWalker`, `EffectRowWalker`, `CapWalker`.
  - One unit test in `emit_walker.rs` confirms walking an empty `IrArena` produces no side effects and zero diagnostics.
- **Files:** `crates/paideia-as-elaborator/src/emit_walker.rs`, `crates/paideia-as-elaborator/src/lib.rs`.
- **Dependencies:** none (uses existing Phase-4 m1 walker convention).
- **Estimated size:** S
- **Phase:** Phase 5 — Build-emit activation.
- **Milestone:** `phase-5-elab-lowering`.

---

### m1-002. elaborator: `EmitWalker` lowers `IrKind::Let(Literal)` for `let : u64 = imm`

- **Summary:** When the walker enters an `IrKind::Let` whose body is an `IrKind::Literal` of integer type, it emits the canonical `mov reg64, imm` `Instruction` into `InstructionSideTable` keyed by the let node's IrNodeId. Phase-5 simplification: the target register is always `RAX` for top-level `let` items (matching the calling-convention return register); module-level `let` is treated as "produces this value to RAX".
- **Acceptance criteria:** task closed when
  - The walker recognises the `Let → Literal` shape via `ir.arena[id].kind == IrKind::Let` and the first child's `kind == IrKind::Literal`.
  - For literals fitting in i32, emits `Instruction { mnemonic: Mov, operands: [Reg(RegId(0)), Imm64(value as i64)], encoding_hint: None }` (encoder bridge tightens to imm32 form per m2 §6 in encode_instruction.rs:154).
  - For literals > i32 range, emits the same with full 64-bit immediate.
  - `Instruction` is inserted via `InstructionSideTable::insert(let_node_id, instruction)`.
  - Unit test (in `emit_walker.rs::tests`) builds a synthetic IR `let answer : u64 = 42` and asserts the side-table has exactly one entry, encoder bridge encodes it to `48 c7 c0 2a 00 00 00`.
  - Unit test for `let magic : u64 = 0xCAFE_F00D_DEAD_BEEF` emits `48 b8 ef be ad de 0d f0 fe ca` (imm64 form).
- **Files:** `crates/paideia-as-elaborator/src/emit_walker.rs`.
- **Dependencies:** m1-001.
- **Estimated size:** S
- **Phase:** Phase 5 — Build-emit activation.
- **Milestone:** `phase-5-elab-lowering`.

---

### m1-003. elaborator: `EmitWalker` lowers `IrKind::Lambda` body for `fn (x) -> x + N`

- **Summary:** When the walker enters an `IrKind::Lambda` whose body matches the `Var + Literal` shape (the `add_one` exemplar from `examples/02_functions.pdx`), it emits `lea rax, [rdi + N] ; ret` into `InstructionSideTable`. Phase-5 simplification: only the single-parameter `fn (x : u64) -> x + N` shape is wired here. The multi-parameter / multi-statement cases are deferred to Phase 6+ (records / generics / borrowed-refs surface).
- **Acceptance criteria:** task closed when
  - The walker recognises the `Lambda → App(+, Var(arg0), Literal(n))` shape.
  - Emits two `Instruction` entries into `InstructionSideTable`:
    1. `Lea` with `[Reg(Rax), MemSib { base: Rdi, index: None, scale: X1, disp: n }]`.
    2. `Ret` with no operands.
  - For body `fn (x : u64) -> x` (identity), emits `mov rax, rdi ; ret` instead.
  - For body `fn (x : u64) -> x + x` (double), emits `lea rax, [rdi + rdi*1] ; ret` (uses SIB-form indexed load with `index = Some(rdi)`, `scale = X1`).
  - Three unit tests exercise the three shapes. Encoded bytes per shape: `add_one` → `48 8d 47 01 c3` (5 bytes); identity → `48 89 f8 c3` (4 bytes); double → `48 8d 04 3f c3` (5 bytes).
  - The walker records `(lambda_node_id → first_instruction_offset)` in `EmitPassState.function_offsets` so m5 symbol export can name the function entry.
- **Files:** `crates/paideia-as-elaborator/src/emit_walker.rs`, possibly minor adjustments to `crates/paideia-as-encoder/src/encode_instruction.rs` if the SIB-form lea exposes a missing operand shape.
- **Dependencies:** m1-002.
- **Estimated size:** S
- **Phase:** Phase 5 — Build-emit activation.
- **Milestone:** `phase-5-elab-lowering`.

---

### m1-004. elaborator: `EmitWalker` handles `IrKind::Unsafe` — delegate to UnsafeWalker (m3)

- **Summary:** When the walker enters an `IrKind::Unsafe`, it does not emit anything itself — instead, it records the node ID into `EmitPassState.pending_unsafe_blocks: Vec<IrNodeId>` so m3's `UnsafeWalker` (a dedicated walker per `walker-hookups-phase4.md` §1.3 handler-clause pattern) can resolve the block's parsed instruction stream from the AST in a follow-up pass. This separation keeps `EmitWalker` focused on typed-surface lowering; raw-instruction streams are a distinct concern.
- **Acceptance criteria:** task closed when
  - On `enter_node` for `IrKind::Unsafe`, the walker appends `node_id` to `EmitPassState.pending_unsafe_blocks`.
  - `EmitPassState::take_pending_unsafe()` drains and returns the vector for downstream consumption.
  - Unit test: walking an IR with two `IrKind::Unsafe` nodes records both IDs in declaration order.
  - The walker does not attempt to inspect the unsafe block's contents — that is m3's job.
- **Files:** `crates/paideia-as-elaborator/src/emit_walker.rs`.
- **Dependencies:** m1-001.
- **Estimated size:** XS
- **Phase:** Phase 5 — Build-emit activation.
- **Milestone:** `phase-5-elab-lowering`.

---

### m1-005. elaborator: chain `EmitWalker` into `cmd_build` and propagate diagnostics

- **Summary:** Activate `EmitWalker` in the `paideia-as build` pipeline alongside the existing `LinearityWalker` / `EffectRowWalker` / `CapWalker` chain in `crates/paideia-as/src/cmd_build.rs`. Diagnostics from `EmitWalker` route into the same `walker_sink: VecSink`. The walker runs after the existing three (so linearity / effect / cap diagnostics report before any emit-stage diagnostic).
- **Acceptance criteria:** task closed when
  - `cmd_build.rs` allocates an `EmitWalker` and `walks` it over `lowering.ir` with the existing `WalkerCtx` pattern (see lines 156–173 of `cmd_build.rs`).
  - The walker's populated `InstructionSideTable` survives into the emit step by being inserted into `lowering.ir.instructions_mut()` (the `IrArena` already owns one — the walker writes into it directly via `EmitPassState`).
  - On an empty `.pdx` source the chain produces zero diagnostics and an empty `InstructionSideTable`.
  - On `examples/01_hello.pdx` (4 `let` bindings) the table gets 4 entries.
  - On `examples/02_functions.pdx` (4 `let` bindings, each binding a `Lambda`) the table gets ~8 entries (one `mov`/`lea` + one `ret` per function).
  - The check subcommand path (`cmd_check.rs`) is **not** modified — `EmitWalker` is build-only.
- **Files:** `crates/paideia-as/src/cmd_build.rs`, `crates/paideia-as-elaborator/src/lib.rs` (re-export).
- **Dependencies:** m1-002, m1-003, m1-004.
- **Estimated size:** S
- **Phase:** Phase 5 — Build-emit activation.
- **Milestone:** `phase-5-elab-lowering`.

---

## 4. Milestone m2 — Encoder boot-ISA coverage

**Slug:** `phase-5-encoder-boot-isa`
**Issues:** 10
**Governing docs:** Intel SDM Vol 2A/2B (per-instruction encoding tables); `crates/paideia-as-encoder/src/encode.rs` (existing 1527-line encoder); `crates/paideia-as-encoder/src/encode_instruction.rs` (existing dispatch bridge, 664 lines).

The `Mnemonic` enum in `crates/paideia-as-ir/src/instruction.rs:18` covers 10 instructions today. PaideiaOS Phase-1 boot needs another set, listed below in dependency order. Each instruction gets its own issue with the SDM encoding table embedded as an acceptance criterion.

Mnemonics required by P1-001..010 (the kernel-banner critical path):

| P1 task     | Instructions needed                                                       |
|-------------|---------------------------------------------------------------------------|
| P1-001 GDT  | `lgdt [mem]`                                                              |
| P1-002 PT   | `mov reg, [mem]`, `mov [mem], reg` (already mostly there)                  |
| P1-003 long | `mov cr0/4, reg`, `mov reg, cr0/4`, `wrmsr`, `rdmsr`, far-`jmp`           |
| P1-005 bss  | `rep stosq` (sibling of `rep movsb`)                                       |
| P1-006 UART | `in al, dx`, `in ax, dx`, `out dx, al`, `out dx, ax`                       |
| P1-010 ban  | `hlt`, `cli`, `sti`, `nop`                                                |

P2+ adds `lidt`, `iret`, `iretq`, `sysret`, `swapgs`, `cpuid`, `mov dr*, reg`, `int N`. They are still in this milestone (cheaper to land the boot-ISA family together than to come back per-subsystem in Phase 6+).

---

### m2-001. ir + encoder: extend `Mnemonic` with privileged-ISA variants + bridge stub

- **Summary:** Extend `paideia_as_ir::Mnemonic` with the privileged + system-ISA mnemonics needed by PaideiaOS Phase 1 onward: `Lgdt, Lidt, MovCr { write: bool }, MovDr { write: bool }, Wrmsr, Rdmsr, In { width: u8 }, Out { width: u8 }, Iret, Iretq, Sysret, Swapgs, Cpuid, Cli, Sti, Hlt, Int, Nop, RepStosq, FarJmp`. Add matching `Err(EncodeError::Unsupported)` arms in `encode_instruction.rs::encode_instruction`, so subsequent issues (m2-002..010) only need to fill the encoder body.
- **Acceptance criteria:**
  - `Mnemonic` has 20 new variants in declaration order matching the table above.
  - `derive(Clone, Copy, Debug, Eq, PartialEq, Hash)` carries through; `Mnemonic` size growth verified via `static_assertions::const_assert!(size_of::<Mnemonic>() <= 4)` (currently 2 bytes for `Jcc(Cond)`; bumping to a 1-byte payload on `MovCr` keeps size at ≤ 4).
  - `encode_instruction.rs::encode_instruction` dispatches each new variant to a per-mnemonic stub returning `Err(EncodeError::Unsupported("phase-5 m2-NNN"))` with a per-mnemonic marker so failed encodings name the open issue.
  - `iced-x86`-based round-trip test exists for at least `Nop` (the trivial case) to prove the per-mnemonic harness shape.
  - The 10 existing variants and their encoders keep working unchanged (existing tests pass).
- **Files:** `crates/paideia-as-ir/src/instruction.rs`, `crates/paideia-as-encoder/src/encode_instruction.rs`.
- **Dependencies:** none.
- **Estimated size:** S
- **Phase:** Phase 5 — Build-emit activation.
- **Milestone:** `phase-5-encoder-boot-isa`.

---

### m2-002. encoder: zero-operand control + sync instructions (`cli, sti, hlt, nop, swapgs, cpuid`)

- **Summary:** Encode the six zero-operand instructions PaideiaOS Phase-1 needs for control-flow gating and CPU synchronisation. All are 1- or 2-byte fixed encodings — the per-instruction work is small but they cluster naturally.
- **Acceptance criteria:**
  - `cli` → `FA` (1 byte). `sti` → `FB` (1 byte). `hlt` → `F4` (1 byte). `nop` → `90` (1 byte). `swapgs` → `0F 01 F8` (3 bytes). `cpuid` → `0F A2` (2 bytes).
  - Each encoder function lives in `crates/paideia-as-encoder/src/encode.rs` named `encode_cli`, `encode_sti`, etc. (or as a single `encode_zero_operand` helper with a per-mnemonic byte table).
  - Each is dispatched from `encode_instruction.rs::encode_instruction` via a per-`Mnemonic`-arm `inst.operands.is_empty()` guard (yields `EncodeError::OperandCount` if not).
  - Six round-trip unit tests via `iced-x86::Decoder` confirm each mnemonic round-trips to itself.
  - SDM reference: Vol 2A §3.2 (CLI), §4.3 (HLT), §4.3 (SWAPGS), §3.2 (CPUID).
- **Files:** `crates/paideia-as-encoder/src/encode.rs`, `crates/paideia-as-encoder/src/encode_instruction.rs`.
- **Dependencies:** m2-001.
- **Estimated size:** XS
- **Phase:** Phase 5 — Build-emit activation.
- **Milestone:** `phase-5-encoder-boot-isa`.

---

### m2-003. encoder: I/O port instructions (`in al/ax dx`, `out dx al/ax`)

- **Summary:** Encode the four `in`/`out` forms PaideiaOS Phase-1 needs for UART 16550 init (P1-006..010). Only the `dx`-addressed forms are required; the immediate-port forms (`in al, imm8`) are skipped (16550 lives at variable port addresses, the kernel must address it via DX).
- **Acceptance criteria:**
  - `in al, dx` → `EC` (1 byte). `in ax, dx` → `66 ED` (2 bytes). `in eax, dx` → `ED` (1 byte).
  - `out dx, al` → `EE` (1 byte). `out dx, ax` → `66 EF` (2 bytes). `out dx, eax` → `EF` (1 byte).
  - Operand check: `[Operand::Reg(Rax)]` only — the SDM encoding fixes the source/dest register as `al`/`ax`/`eax`/`rax` (no rax/qword form exists for `in`/`out`).
  - Width parameter on the `Mnemonic` (`In { width: 1|2|4 }`, `Out { width: 1|2|4 }`) selects the encoding.
  - Round-trip unit tests for all six forms via `iced-x86`.
  - SDM reference: Vol 2A §3.2 IN / OUT.
- **Files:** `crates/paideia-as-encoder/src/encode.rs`, `crates/paideia-as-encoder/src/encode_instruction.rs`.
- **Dependencies:** m2-001.
- **Estimated size:** S
- **Phase:** Phase 5 — Build-emit activation.
- **Milestone:** `phase-5-encoder-boot-isa`.

---

### m2-004. encoder: MSR access (`wrmsr, rdmsr`) + `Mnemonic::Int N`

- **Summary:** Encode the two-byte MSR-access forms needed by P1-003 (EFER.LME bit set via `wrmsr` with MSR index `0xC0000080`) and the software-interrupt form for IDT testing.
- **Acceptance criteria:**
  - `wrmsr` → `0F 30` (2 bytes). `rdmsr` → `0F 32` (2 bytes). `int N` → `CD <imm8>` (2 bytes) where `N: u8`.
  - `wrmsr`/`rdmsr` accept no operands (the MSR index lives in `ECX`, the value in `EDX:EAX`; this is the caller's discipline). `int` takes one `Operand::Imm64` whose value must fit in `u8` else `EncodeError::Unsupported("int operand > u8")`.
  - Round-trip unit tests via `iced-x86`.
  - SDM reference: Vol 2A §3.2 WRMSR / RDMSR / INT.
- **Files:** `crates/paideia-as-encoder/src/encode.rs`, `crates/paideia-as-encoder/src/encode_instruction.rs`.
- **Dependencies:** m2-001.
- **Estimated size:** XS
- **Phase:** Phase 5 — Build-emit activation.
- **Milestone:** `phase-5-encoder-boot-isa`.

---

### m2-005. encoder: control-register MOV (`mov cr*, reg`, `mov reg, cr*`)

- **Summary:** Encode the eight forms PaideiaOS Phase-1 needs for long-mode entry: `mov cr0, reg`, `mov cr2, reg`, `mov cr3, reg`, `mov cr4, reg` and the reverse forms. The CR registers are accessed via two-byte opcodes with a non-standard ModR/M shape.
- **Acceptance criteria:**
  - `mov cr0, rax` → `0F 22 C0`. `mov cr3, rax` → `0F 22 D8`. `mov cr4, rax` → `0F 22 E0`.
  - `mov rax, cr0` → `0F 20 C0`. `mov rax, cr3` → `0F 20 D8`. `mov rax, cr4` → `0F 20 E0`.
  - CR8 (TPR) accessed via `0F 22 /r` with REX.R=1: `mov cr8, rax` → `44 0F 22 C0`. Phase-5 supports CR0..CR4 + CR8 only (CR5/6/7 are reserved on x86_64).
  - Dispatch via `Mnemonic::MovCr { write: true }` (mov-to-cr) and `MovCr { write: false }` (mov-from-cr); operand shape `[Reg(cr_index), Reg(gpr)]` where `cr_index` is encoded as a `RegId(u8)` (0..4 + 8). The bridge resolves cr_index via a small lookup.
  - Operand validation: target/source must be a 64-bit GPR (not 8/16/32-bit). Any other shape returns `EncodeError::OperandShape`.
  - 12 unit tests (6 write forms + 6 read forms) round-trip via `iced-x86`.
  - SDM reference: Vol 2B §4.2 MOV (CR0–CR4, CR8 ↔ r64).
- **Files:** `crates/paideia-as-encoder/src/encode.rs`, `crates/paideia-as-encoder/src/encode_instruction.rs`.
- **Dependencies:** m2-001.
- **Estimated size:** S
- **Phase:** Phase 5 — Build-emit activation.
- **Milestone:** `phase-5-encoder-boot-isa`.

---

### m2-006. encoder: debug-register MOV (`mov dr*, reg`, `mov reg, dr*`)

- **Summary:** Encode the DR-register access forms for future debug-trap subsystems (P2/P3 onward). Mirrors m2-005 structure but with opcodes `0F 21` (read) and `0F 23` (write).
- **Acceptance criteria:**
  - `mov dr0, rax` → `0F 23 C0`. `mov dr7, rax` → `0F 23 F8`.
  - `mov rax, dr0` → `0F 21 C0`. `mov rax, dr7` → `0F 21 F8`.
  - DR0..DR7 supported (DR4/DR5 alias DR6/DR7 on most CPUs but Phase-5 encodes them per SDM regardless).
  - Operand validation parallel to m2-005.
  - 16 unit tests via `iced-x86`.
  - SDM reference: Vol 2B §4.2 MOV (DR0–DR7 ↔ r64).
- **Files:** `crates/paideia-as-encoder/src/encode.rs`, `crates/paideia-as-encoder/src/encode_instruction.rs`.
- **Dependencies:** m2-001.
- **Estimated size:** XS
- **Phase:** Phase 5 — Build-emit activation.
- **Milestone:** `phase-5-encoder-boot-isa`.

---

### m2-007. encoder: descriptor-table load (`lgdt [mem]`, `lidt [mem]`)

- **Summary:** Encode the two descriptor-table load forms. PaideiaOS P1-001 needs `lgdt`; P2-013 needs `lidt`. Both consume a memory operand of the form `[reg]` or `[rip + label]` (P5 doesn't yet wire RIP-relative; phase-5 supports `[reg + disp]` only — the boot code can place the GDT descriptor at a known address loaded into a register before `lgdt`).
- **Acceptance criteria:**
  - `lgdt [rdi]` → `0F 01 17` (3 bytes). Encoding: `0F 01 /2` with mod=00, reg=2, rm=7 (RDI).
  - `lgdt [rdi + 8]` → `0F 01 57 08` (4 bytes).
  - `lidt [rdi]` → `0F 01 1F` (encoding: `0F 01 /3`). `lidt [rdi + 16]` → `0F 01 5F 10`.
  - Operand shape: `[Operand::MemSib { base, index: None, scale: X1, disp }]`. Indexed forms (`[base + index*scale]`) return `EncodeError::Unsupported("lgdt/lidt indexed form")`.
  - Six unit tests covering both mnemonics with disp=0, disp=8, disp=-128.
  - Round-trip via `iced-x86`.
  - SDM reference: Vol 2A §3.2 LGDT / LIDT.
- **Files:** `crates/paideia-as-encoder/src/encode.rs`, `crates/paideia-as-encoder/src/encode_instruction.rs`.
- **Dependencies:** m2-001.
- **Estimated size:** XS
- **Phase:** Phase 5 — Build-emit activation.
- **Milestone:** `phase-5-encoder-boot-isa`.

---

### m2-008. encoder: interrupt-return + system-return (`iret, iretq, sysret`)

- **Summary:** Encode the three return-from-privileged-context forms. P6 (interrupt + exception) consumes `iretq`; P4 (scheduler) may consume `sysret` for fast user-return. `iret` (32-bit return) is included for completeness but not used in 64-bit kernel.
- **Acceptance criteria:**
  - `iret` → `CF` (1 byte). `iretq` → `48 CF` (2 bytes, REX.W prefix). `sysret` → `48 0F 07` (3 bytes, 64-bit form per SDM).
  - All three are zero-operand.
  - Three unit tests via `iced-x86`.
  - SDM reference: Vol 2A §3.2 IRET/IRETD/IRETQ, §4.3 SYSRET.
- **Files:** `crates/paideia-as-encoder/src/encode.rs`, `crates/paideia-as-encoder/src/encode_instruction.rs`.
- **Dependencies:** m2-001.
- **Estimated size:** XS
- **Phase:** Phase 5 — Build-emit activation.
- **Milestone:** `phase-5-encoder-boot-isa`.

---

### m2-009. encoder: `rep stosq` for `.bss` zeroing (P1-005)

- **Summary:** Encode the `REP STOSQ` form needed by `src/kernel/boot/zero_bss.pdx`. Mirror of the existing `rep movsb` encoding (`F3 A4`) but for the 8-byte store-from-RAX-to-RDI variant.
- **Acceptance criteria:**
  - `rep stosq` → `F3 48 AB` (3 bytes). The `48` is REX.W to select the 64-bit operand size.
  - Zero-operand mnemonic; operand list must be empty else `EncodeError::OperandCount`.
  - Round-trip via `iced-x86`.
  - SDM reference: Vol 2A §3.2 REP/STOS (REX.W form).
- **Files:** `crates/paideia-as-encoder/src/encode.rs`, `crates/paideia-as-encoder/src/encode_instruction.rs`.
- **Dependencies:** m2-001.
- **Estimated size:** XS
- **Phase:** Phase 5 — Build-emit activation.
- **Milestone:** `phase-5-encoder-boot-isa`.

---

### m2-010. encoder: far-`jmp m16:64` for the 32→64 mode transition (P1-003)

- **Summary:** Encode the far-jmp form that completes the long-mode entry sequence (after CR0.PG is set, the CPU executes in compatibility mode; far-jmp loads a 64-bit code segment from a `m16:64` memory operand and switches to true 64-bit mode). Encoded as `48 FF /5 [mem]`.
- **Acceptance criteria:**
  - `jmp far [rdi]` → `48 FF 2F` (3 bytes).
  - `jmp far [rip + offset32]` → `48 FF 2D <disp32>` (7 bytes). Phase-5 supports the RIP-relative form for this one mnemonic — the boot path places the far-jmp descriptor at a link-time constant.
  - Dispatch via `Mnemonic::FarJmp` with operand `[Operand::MemSib { base, index: None, scale: X1, disp }]` or a new `Operand::MemRipRel { disp: i32 }` (introduce if RIP-rel is otherwise missing from `Operand`; check `paideia-as-ir/src/instruction.rs:80` first).
  - Three unit tests: `[rdi]`, `[rdi + 8]`, `[rip + 0x1000]`.
  - Round-trip via `iced-x86`.
  - SDM reference: Vol 2A §3.2 JMP (Mem-Indirect Far).
- **Files:** `crates/paideia-as-encoder/src/encode.rs`, `crates/paideia-as-encoder/src/encode_instruction.rs`, possibly `crates/paideia-as-ir/src/instruction.rs` if `Operand::MemRipRel` does not exist.
- **Dependencies:** m2-001.
- **Estimated size:** S
- **Phase:** Phase 5 — Build-emit activation.
- **Milestone:** `phase-5-encoder-boot-isa`.

---

## 5. Milestone m3 — Unsafe-block payload walker

**Slug:** `phase-5-unsafe-walker`
**Issues:** 5
**Governing docs:** `crates/paideia-as-parser/src/parse_unsafe.rs` (the 960-line `unsafe { effects: …, capabilities: …, justification: …, block: { … } }` parser); `crates/paideia-as-parser/src/parse_memref.rs` (memory operand parser, 116 lines); `custom-assembler.md` §9.1 (audit catalog for unsafe blocks).

The parser already accepts arbitrary instruction streams inside `unsafe { block: { … } }`. The parsed AST shape is preserved as `ExprData::Unsafe { effects, capabilities, justification, block }` where `block` is an `ExprBlock` whose children are `StmtInstruction` nodes (see `parse_unsafe.rs:79`). The AST→IR lowerer in `lower.rs:159..166` collapses the whole thing to `IrKind::Unsafe` and the child statements to `IrKind::Action`, **losing the mnemonic structure**.

m3 introduces `UnsafeWalker`, a dedicated walker that re-traverses the AST for each pending unsafe block (collected by m1-004), parses register / memory / immediate operands per `StmtInstruction`, looks up the mnemonic in the encoder bridge's mnemonic table, and emits `Instruction` entries into `InstructionSideTable` keyed by the appropriate IR node IDs.

---

### m3-001. ir + ast: persist `StmtInstruction` mnemonic + operand AST shape through lowering

- **Summary:** Today the AST→IR lowerer maps `StmtInstruction` to `IrKind::Action`, dropping the mnemonic-and-operands payload. Introduce a new IR kind `IrKind::RawInstruction` that preserves a back-pointer to the originating AST node, so m3-002 can re-read the parsed instruction shape without re-parsing source. The AST itself already holds the data — only the bridge changes.
- **Acceptance criteria:**
  - `IrKind::RawInstruction` added to `paideia_as_ir::node::IrKind` enum.
  - `lower.rs` table updated: `NodeKind::StmtInstruction → IrKind::RawInstruction` (not `Action`).
  - The `LoweringResult::ast_to_ir` map suffices to round-trip from `IrNodeId` back to `NodeId` (already true per the bijection invariant in `lower.rs:79`).
  - One unit test: lowering a single `mov rax, 1` `StmtInstruction` produces one `IrKind::RawInstruction` whose mapped `NodeId` resolves via `ast_to_ir` back to the original AST node.
  - Existing tests that asserted `IrKind::Action` for `StmtInstruction` are updated.
- **Files:** `crates/paideia-as-ir/src/node.rs`, `crates/paideia-as-elaborator/src/lower.rs`.
- **Dependencies:** none.
- **Estimated size:** S
- **Phase:** Phase 5 — Build-emit activation.
- **Milestone:** `phase-5-unsafe-walker`.

---

### m3-002. elaborator: operand parser for the unsafe-block surface

- **Summary:** Build a `parse_operand_from_ast(ast: &AstArena, operand_node: NodeId) → Result<Operand, OperandError>` function that consumes an AST operand subtree (per `parse_memref.rs` and `parse_unsafe.rs::parse_operand`) and produces a `paideia_as_ir::Operand`. Three operand shapes are in scope for Phase 5: register names (`rax`, `rdi`, ..., `r15`, plus `cr0`..`cr8`, `dr0`..`dr7`), memory references (`[reg]`, `[reg + disp]`, `[reg + reg*scale + disp]`), and immediate literals (decimal, hex, char). Effect / capability operands stay declared-not-decoded.
- **Acceptance criteria:**
  - `parse_operand_from_ast` returns `Operand::Reg(RegId(0))` for AST representing `rax`; `RegId(7)` for `rdi`; `RegId(15)` for `r15`.
  - For control registers: returns a tagged variant or a sentinel `RegId(0x100 | cr_index)` that m2-005's bridge recognises; commit message documents the chosen encoding.
  - `[rdi + 8]` parses to `Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 8 }`.
  - `[rdi + rsi * 4]` parses to `Operand::MemSib { base: RegId(7), index: Some(RegId(6)), scale: Scale::X4, disp: 0 }`.
  - `0x12345678` parses to `Operand::Imm64(0x12345678)`.
  - Unknown register name returns `OperandError::UnknownRegister(name)` with the span.
  - 12 unit tests covering all operand shapes plus the 4 error paths.
- **Files:** `crates/paideia-as-elaborator/src/unsafe_walker.rs` (new), `crates/paideia-as-elaborator/src/lib.rs`.
- **Dependencies:** m3-001.
- **Estimated size:** S
- **Phase:** Phase 5 — Build-emit activation.
- **Milestone:** `phase-5-unsafe-walker`.

---

### m3-003. elaborator: mnemonic-name → `Mnemonic` enum resolver

- **Summary:** Build a `resolve_mnemonic(name: &str) → Option<Mnemonic>` function that maps source mnemonic strings (case-insensitive) to `paideia_as_ir::Mnemonic` enum values. The table covers the 10 existing mnemonics plus the 20 added in m2-001.
- **Acceptance criteria:**
  - `resolve_mnemonic("mov") == Some(Mnemonic::Mov)`. `resolve_mnemonic("MOV") == Some(Mnemonic::Mov)` (case-insensitive).
  - `resolve_mnemonic("je") == Some(Mnemonic::Jcc(Cond::Eq))`. All eight Jcc forms (`je, jne, jl, jge, jle, jg, jb, ja`) map to the appropriate `Cond`.
  - `resolve_mnemonic("lgdt") == Some(Mnemonic::Lgdt)`. Each m2-001 mnemonic maps to its enum value.
  - `resolve_mnemonic("rep_movsb") == Some(Mnemonic::RepMovsb)`. The underscore form is the canonical source spelling; `rep movsb` (two tokens) also resolves to the same mnemonic via the parser's instruction-statement handling (the parser joins `rep` + `movsb`).
  - `resolve_mnemonic("not_a_real_mnemonic") == None`.
  - Unknown mnemonic at use site emits new diagnostic `U1605` ("unknown mnemonic in unsafe block").
  - Table-driven implementation in a `MNEMONIC_TABLE: &[(&str, Mnemonic)]` `phf`-style static; 30+ entries.
  - One unit test per known mnemonic plus 3 negative tests.
- **Files:** `crates/paideia-as-elaborator/src/unsafe_walker.rs`, `crates/paideia-as-diagnostics/catalog.toml` (`U1605`).
- **Dependencies:** m2-001, m3-002.
- **Estimated size:** S
- **Phase:** Phase 5 — Build-emit activation.
- **Milestone:** `phase-5-unsafe-walker`.

---

### m3-004. elaborator: `UnsafeWalker` consumes pending blocks, emits `Instruction` entries

- **Summary:** Bring it together. `UnsafeWalker::run(arena: &AstArena, ir: &mut IrArena, pending: Vec<IrNodeId>)` iterates the pending IDs collected by m1-004, finds each block's child `StmtInstruction` nodes via `ast_to_ir` reverse lookup, parses each with m3-002 + m3-003, and inserts the resulting `Instruction` into `ir.instructions_mut()` keyed by the `StmtInstruction`'s `IrNodeId`.
- **Acceptance criteria:**
  - Walker entry point: `pub fn run(arena, ir, pending) -> Vec<Diagnostic>`.
  - For each pending unsafe block, walks its `block: ExprBlock` child sequence and emits one `Instruction` per `StmtInstruction`.
  - Unknown mnemonic at a use site: emits `U1605` with the mnemonic's span; the offending `StmtInstruction` produces no `Instruction` (skipped, walk continues).
  - Operand shape error (m3-002 returns `Err`): emits `U1606` ("malformed operand in unsafe block") with the operand span; instruction skipped.
  - For the `lgdt [rdi]` example from `examples/15_unsafe.pdx`'s extension: one `Instruction { mnemonic: Lgdt, operands: [MemSib { base: Rdi, ... }] }` lands in the side-table.
  - Three integration tests via synthetic `.pdx` fixtures under `crates/paideia-as-elaborator/tests/unsafe_walker/`.
- **Files:** `crates/paideia-as-elaborator/src/unsafe_walker.rs`, `crates/paideia-as-elaborator/tests/unsafe_walker/`, `crates/paideia-as-diagnostics/catalog.toml` (`U1606`).
- **Dependencies:** m3-001, m3-002, m3-003.
- **Estimated size:** S
- **Phase:** Phase 5 — Build-emit activation.
- **Milestone:** `phase-5-unsafe-walker`.

---

### m3-005. cli: `cmd_build` calls `UnsafeWalker::run` after `EmitWalker`

- **Summary:** Wire `UnsafeWalker::run` into the build pipeline. Runs after `EmitWalker` (m1-005) so the `pending_unsafe_blocks` vector is populated. Diagnostics from `UnsafeWalker` route into the same `walker_sink`.
- **Acceptance criteria:**
  - `cmd_build.rs` calls `EmitPassState::take_pending_unsafe()` then `UnsafeWalker::run(&arena, &mut lowering.ir, pending)`.
  - For a `.pdx` source with three `unsafe { block: { lgdt [rdi]; cli; hlt } }` blocks, the `InstructionSideTable` has 3 entries (one per block, three instructions each — 9 total).
  - End-to-end test in `crates/paideia-as/tests/`: parse + lower + emit-walk + unsafe-walk of a 3-instruction unsafe block produces an `InstructionSideTable` with `len() == 3` and the disassembled bytes match `0F 01 17 FA F4`.
  - The check subcommand path is **not** modified — `UnsafeWalker` is build-only (consistent with m1-005).
- **Files:** `crates/paideia-as/src/cmd_build.rs`, `crates/paideia-as/tests/build_unsafe.rs`.
- **Dependencies:** m1-005, m3-004.
- **Estimated size:** XS
- **Phase:** Phase 5 — Build-emit activation.
- **Milestone:** `phase-5-unsafe-walker`.

---

## 6. Milestone m4 — Initialised static data surface

**Slug:** `phase-5-static-data`
**Issues:** 4
**Governing docs:** `custom-assembler.md` §12.1 (ELF section roles); `crates/paideia-as-emitter-elf/src/sections.rs` (existing section-table infrastructure).

Today `let answer : u64 = 42` parses successfully and walks through `check` clean — but the constant value never makes it into any binary section. The PaideiaOS Phase-1 GDT (P1-001) is a ~40-byte data structure that needs to live in `.rodata` and be referenced by an `lgdt` instruction's memory operand. m4 makes that work.

The minimum surface this milestone covers:

```text
let gdt_descriptor : [u8; 16] = [
    0x27, 0x00,                  // limit = 0x27 (5 entries × 8 - 1)
    0x00, 0x10, 0x00, 0x00,      // base low (placeholder for gdt_table addr)
    0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00,      // pad
    0x00, 0x00,
]

let gdt_table : [u64; 5] = [
    0x0,                              // null
    0x00CF_9A00_0000_FFFF,            // code32
    0x00CF_9200_0000_FFFF,            // data32
    0x00AF_9A00_0000_FFFF,            // code64
    0x00AF_9200_0000_FFFF,            // data64
]
```

Notably: `[u8; N]` and `[u64; N]` type syntax must parse without P0100. The parser today fails on `[u8; N]` at the `;` (per the user's m11-006 verification report). m4-001 covers this.

---

### m4-001. parser: `[T; N]` fixed-array type parses without P0100

- **Summary:** Extend `parse_type.rs` to accept the `[T; N]` syntax for fixed-size array types. Today the `;` token triggers `P0100`. This issue extends `parse_type` to peek for `;` after the inner type and consume the array-length expression.
- **Acceptance criteria:**
  - `let bytes : [u8; 16] = [...]` parses without diagnostics.
  - `let table : [u64; 5] = [...]` parses without diagnostics.
  - Nested arrays `[[u8; 4]; 4]` parse (recursive).
  - The AST produces a new `Type::Array { element: NodeId, length: NodeId }` variant or extends `TypeApp` with the appropriate semantics; the choice is documented in the commit message.
  - The length expression is parsed as a primary expression (not just a literal), so `let x : [u8; SIZE] = …` will work once `SIZE` resolves at compile time (Phase 6+ feature; for now, only literal lengths must elaborate).
  - 6 unit tests covering `[u8; 0]`, `[u8; 16]`, `[u64; 5]`, `[[u8; 4]; 4]`, plus 2 reject tests for `[u8;]` (missing length) and `[u8; ]` (whitespace-only length).
- **Files:** `crates/paideia-as-parser/src/parse_type.rs`, `crates/paideia-as-ast/src/types.rs`.
- **Dependencies:** none.
- **Estimated size:** S
- **Phase:** Phase 5 — Build-emit activation.
- **Milestone:** `phase-5-static-data`.

---

### m4-002. parser: array literal `[expr, expr, ...]` initialisers

- **Summary:** Extend the parser to accept array-literal initialisers in expression position. `[1, 2, 3]` is currently rejected (the `[` token in expression position is treated as the start of an indexing expression, not a literal). Disambiguation: an array literal is a primary expression starting with `[` whose immediate followers are expressions separated by commas, terminated by `]`.
- **Acceptance criteria:**
  - `let xs : [u64; 3] = [1, 2, 3]` parses.
  - `let bytes : [u8; 5] = [0xCF, 0x9A, 0x00, 0x00, 0xFF]` parses.
  - Empty array literal `[]` requires explicit type annotation (no inference yet); without one, emits new diagnostic `P0210`.
  - Trailing comma `[1, 2, 3,]` is accepted.
  - AST has `ExprData::ArrayLit(Vec<NodeId>)`.
  - 6 unit tests covering the above shapes.
- **Files:** `crates/paideia-as-parser/src/parse_primary.rs` (or `parse_expr.rs`, wherever array-literal disambiguation lives), `crates/paideia-as-ast/src/exprs.rs`, `crates/paideia-as-diagnostics/catalog.toml` (`P0210`).
- **Dependencies:** m4-001.
- **Estimated size:** S
- **Phase:** Phase 5 — Build-emit activation.
- **Milestone:** `phase-5-static-data`.

---

### m4-003. emitter-elf: `.rodata` + `.data` section population from elaborator

- **Summary:** Introduce a `DataSideTable` (parallel to `InstructionSideTable`) keyed by `IrNodeId`, holding `DataEntry { section: SectionKind, bytes: Vec<u8>, symbol_name: String, align: u8 }`. `EmitWalker` (extended) populates it for module-level `let : T = literal` and `let : [T; N] = [literal_array]` items. `crates/paideia-as-emitter-elf/src/sections.rs` consumes it and writes the bytes into the appropriate sections.
- **Acceptance criteria:**
  - `DataSideTable` defined in `crates/paideia-as-ir/src/data.rs`; `IrArena::data() / data_mut()` accessors mirror the existing instruction table.
  - `EmitWalker` recognises module-level `IrKind::Let` whose body is `IrKind::Literal` (scalar) or `IrKind::ArrayLit` (array) and inserts a `DataEntry` into `DataSideTable`. Section: `.rodata` (default for `let`; `.data` for `let mut` once mutability lands — Phase 6+).
  - Bytes are little-endian-packed per type: `u8 = 1 byte`, `u64 = 8 bytes`, `[u8; N] = N bytes`, `[u64; N] = 8*N bytes`.
  - The symbol name defaults to the binding's source identifier (`gdt_descriptor`, `gdt_table`, …).
  - The ELF writer's `add_rodata_bytes` / `add_data_bytes` methods now exist (extend `sections.rs`) and the emit step calls them once per `DataEntry`.
  - 6 unit tests over the m4-002 example shapes; one integration test that builds a `.pdx` source declaring the 16-byte GDT descriptor and verifies `readelf -x .rodata <object>` shows the exact byte sequence.
- **Files:** `crates/paideia-as-ir/src/data.rs`, `crates/paideia-as-ir/src/arena.rs`, `crates/paideia-as-ir/src/lib.rs`, `crates/paideia-as-elaborator/src/emit_walker.rs`, `crates/paideia-as-emitter-elf/src/sections.rs`, `crates/paideia-as-emitter-elf/src/writer.rs`.
- **Dependencies:** m1-002, m4-002.
- **Estimated size:** S
- **Phase:** Phase 5 — Build-emit activation.
- **Milestone:** `phase-5-static-data`.

---

### m4-004. emitter-elf: relocation linking `.text` references to `.rodata` data symbols

- **Summary:** When a function body's `Instruction` references a data symbol by name (e.g., `lgdt [gdt_descriptor]` — phase-5 introduces this via the new symbol-operand shape from m5-002), the ELF emitter creates a `R_X86_64_PC32` relocation in `.rela.text` against the data symbol. `ld` then patches the displacement at link time.
- **Acceptance criteria:**
  - When `Operand::SymbolRef("gdt_descriptor")` appears in an instruction, the encoder bridge writes a placeholder zero displacement and the emitter inserts a `R_X86_64_PC32` relocation at the displacement's byte offset.
  - The relocation's symbol points to the `STT_OBJECT` symbol the data emitter created (m4-003).
  - `readelf -r <object>` shows the relocation.
  - `ld <object> -o <out>` produces an executable whose `objdump -d` shows the displacement resolved to the data symbol's final address.
  - Two integration tests: (1) a function calling `lgdt [gdt_descriptor]` where `gdt_descriptor` is declared in the same `.pdx` file; (2) two functions, one referencing data declared in the other (cross-function within one TU).
- **Files:** `crates/paideia-as-emitter-elf/src/relocs.rs`, `crates/paideia-as-emitter-elf/src/lower.rs`.
- **Dependencies:** m4-003, m5-002.
- **Estimated size:** S
- **Phase:** Phase 5 — Build-emit activation.
- **Milestone:** `phase-5-static-data`.

---

## 7. Milestone m5 — Symbol export + cross-file relocations

**Slug:** `phase-5-symbols-relocs`
**Issues:** 5
**Governing docs:** `custom-assembler.md` §12.1 (symbol table and relocation format); `crates/paideia-as-emitter-elf/src/symtab.rs` (existing 1-symbol stub); `crates/paideia-as-emitter-elf/src/relocs.rs`.

Today the ELF emitter has one hard-coded symbol (`add_one`). The PaideiaOS Phase-1 kernel needs:

1. **`_start` as the entry-point symbol.** Linker reads `ENTRY(_start)` from `link.ld`; without a `_start` symbol the link fails. Today the assembler doesn't name any symbol from the source.
2. **One symbol per top-level `let : T = …`.** `let kernel_main_64 : ...` becomes an `STT_FUNC` symbol; `let gdt_descriptor : [u8; 16] = …` becomes an `STT_OBJECT` symbol.
3. **Cross-file references.** `src/kernel/boot/long_mode.pdx` calls into `src/kernel/boot/gdt.pdx::gdt_load`. The call instruction produces an `R_X86_64_PC32` relocation against an undefined external symbol; `ld` resolves at link time.

---

### m5-001. ir: top-level binding symbol table

- **Summary:** Introduce a `SymbolTable` side-table (analogous to `InstructionSideTable`) holding `Symbol { name: String, kind: SymbolKind, ir_node: IrNodeId, global: bool }` where `SymbolKind ∈ { Function, Object, Undefined }`. `EmitWalker` populates entries when entering each module-level `IrKind::Let`. The entry's `global` flag is true by default; a future `private let` form (Phase 6+) would set it false.
- **Acceptance criteria:**
  - `SymbolTable` defined in `crates/paideia-as-ir/src/symbol.rs` with `insert`, `lookup_by_name`, `iter` methods.
  - `EmitWalker` calls `symbols.insert(Symbol { name: extract_name(let_node), kind: SymbolKind::Function (if Lambda body) else Object, ... })` on each module-level `IrKind::Let`.
  - The `_start` name is treated as a magic name: any binding named `_start` is auto-flagged `global: true` and marked as the entry-point.
  - 3 unit tests: `let foo : u64 = 42` produces one Object symbol; `let add_one : (u64) -> u64 = fn ...` produces one Function symbol; `let _start : () -> () = fn () -> ...` is marked as entry-point.
- **Files:** `crates/paideia-as-ir/src/symbol.rs`, `crates/paideia-as-ir/src/arena.rs`, `crates/paideia-as-ir/src/lib.rs`, `crates/paideia-as-elaborator/src/emit_walker.rs`.
- **Dependencies:** m1-005.
- **Estimated size:** S
- **Phase:** Phase 5 — Build-emit activation.
- **Milestone:** `phase-5-symbols-relocs`.

---

### m5-002. ir: `Operand::SymbolRef(String)` for unresolved symbol references in instructions

- **Summary:** Add a new `Operand` variant `SymbolRef { name: String, addend: i32 }` representing an unresolved symbol reference. m3-002's operand parser produces it when it sees an identifier in operand position that doesn't resolve to a register or immediate. The encoder writes a placeholder zero displacement; m4-004 / m5-003 emit the corresponding relocation.
- **Acceptance criteria:**
  - `Operand::SymbolRef { name: String, addend: i32 }` added to `paideia_as_ir::Operand`.
  - m3-002's `parse_operand_from_ast` returns `SymbolRef { name: "gdt_descriptor", addend: 0 }` for `gdt_descriptor`; returns `SymbolRef { name: "table", addend: 8 }` for `[table + 8]`.
  - Encoder bridge `encode_instruction.rs` recognises the `[SymbolRef(...)]` operand shape for `lgdt`, `lidt`, `lea`, `mov reg, [sym]` and writes a placeholder 4-byte displacement at the correct byte offset (the emitter consumes this offset via a new `RelocSite { byte_offset: u32, symbol: String, kind: RelocKind, addend: i32 }` returned alongside the bytes).
  - The encoder bridge's existing `encode_instruction` signature evolves to `Result<EncodeOutput, EncodeError>` where `EncodeOutput { reloc_sites: Vec<RelocSite> }`.
  - 4 unit tests covering `lea rax, [gdt_descriptor]`, `lgdt [gdt_descriptor]`, `mov rax, [gdt_descriptor + 8]`, `call kernel_main_64`.
- **Files:** `crates/paideia-as-ir/src/instruction.rs`, `crates/paideia-as-encoder/src/encode_instruction.rs`, `crates/paideia-as-encoder/src/encode.rs`.
- **Dependencies:** m2-001.
- **Estimated size:** S
- **Phase:** Phase 5 — Build-emit activation.
- **Milestone:** `phase-5-symbols-relocs`.

---

### m5-003. emitter-elf: real symbol-table emission from `SymbolTable`

- **Summary:** Replace the hard-coded `add_one` symbol in `cmd_build.rs::build_elf_object` with a `SymbolTable`-driven loop. Each `SymbolTable` entry becomes a `SymbolEntry`; `STT_FUNC` for functions (with `st_size` computed from the function's instruction-byte range), `STT_OBJECT` for data (with `st_size` from `DataEntry::bytes.len()`). The entry-point symbol (`_start`) is emitted with `STB_GLOBAL` binding.
- **Acceptance criteria:**
  - `build_elf_object()` no longer calls `lower_add_one` unconditionally — it iterates `SymbolTable::iter()` and emits one symbol per entry.
  - For each function symbol, the symbol's value is the byte offset where its first instruction was emitted (tracked via `EmitPassState.function_offsets` from m1-003).
  - For each data symbol, the symbol's value is the byte offset in `.rodata`/`.data`.
  - `readelf -s <object>` shows all expected symbols with correct types and sizes.
  - `ld <object>` succeeds when at least one symbol is named `_start` (or if a `--entry` flag overrides — but Phase 5 requires `_start`).
  - 3 integration tests: (1) `.pdx` with just `let _start : () -> () = fn () -> unsafe { ... hlt }` links cleanly; (2) `.pdx` with three functions all get exported symbols; (3) `.pdx` with mixed function + data symbols both emit correctly.
- **Files:** `crates/paideia-as/src/cmd_build.rs`, `crates/paideia-as-emitter-elf/src/symtab.rs`, `crates/paideia-as-emitter-elf/src/writer.rs`.
- **Dependencies:** m5-001, m1-005, m4-003.
- **Estimated size:** S
- **Phase:** Phase 5 — Build-emit activation.
- **Milestone:** `phase-5-symbols-relocs`.

---

### m5-004. emitter-elf: undefined-symbol entries for cross-file references

- **Summary:** When an instruction's `Operand::SymbolRef` names a symbol not present in the local `SymbolTable` (because it lives in a sibling `.pdx` file), the emitter creates an undefined-symbol entry (`SHN_UNDEF`) so `ld` resolves the reference at link time. The relocation then targets this undefined symbol.
- **Acceptance criteria:**
  - When emitting reloc sites, if `SymbolRef.name` is not in `SymbolTable`, the emitter calls `add_undefined_symbol(name)` returning a symbol index.
  - The relocation's `r_info` field uses the undefined-symbol index.
  - `readelf -s <object>` shows the undefined symbol with type `NOTYPE` and `SHN_UNDEF`.
  - Two `.o` files (compiled from sibling `.pdx` files) link via `ld a.o b.o -o linked` when `a.pdx` calls `gdt_load` and `b.pdx` defines `gdt_load`.
  - `objdump -d linked` shows the call's displacement resolved to the correct address.
  - Test fixtures under `crates/paideia-as/tests/cross_file/` cover this.
- **Files:** `crates/paideia-as-emitter-elf/src/symtab.rs`, `crates/paideia-as-emitter-elf/src/relocs.rs`.
- **Dependencies:** m5-002, m5-003.
- **Estimated size:** S
- **Phase:** Phase 5 — Build-emit activation.
- **Milestone:** `phase-5-symbols-relocs`.

---

### m5-005. cli: `cmd_build` writes the real `InstructionSideTable` body into `.text`

- **Summary:** Replace `build_elf_object`'s `lower_add_one(&mut buf)` placeholder call with a loop that iterates `lowering.ir.instructions()`, calls `encode_instruction` for each, accumulates bytes into the text-section buffer, and tracks per-instruction byte offsets for relocation patching. This is the final wiring step that turns the assembled IR into the binary's `.text`.
- **Acceptance criteria:**
  - `build_elf_object()` iterates `InstructionSideTable::iter()` in IR-node order; per instruction, calls `encode_instruction(&inst, &mut buf, &mut stats)` and records the byte offset before/after the call.
  - Per-function byte ranges are inferred from `EmitPassState.function_offsets` (start) and the next function's offset (end); the last function's end is `buf.len()`.
  - Per-instruction reloc sites returned from `encode_instruction` are accumulated into a `Vec<RelocSite>` for m4-004 / m5-004 consumption.
  - The function `lower_add_one` is **deleted** (or repurposed as a benchmark fixture in tests only); its 5-byte hard-coded output no longer flows through the build path.
  - On an empty `.pdx` (no top-level lets) the emitter writes an ELF with an empty `.text` section and zero symbols — and the build succeeds with exit 0 (a degenerate but valid object).
  - On `examples/02_functions.pdx` (4 functions), the resulting `.text` section contains the correct ~16-byte sequence (4 functions × ~4 bytes each).
  - `objdump -d <output>` matches a snapshot for each example.
- **Files:** `crates/paideia-as/src/cmd_build.rs`, `crates/paideia-as-emitter-elf/src/lower.rs`.
- **Dependencies:** m1-005, m2-001, m3-005, m4-003, m5-003, m5-004.
- **Estimated size:** S
- **Phase:** Phase 5 — Build-emit activation.
- **Milestone:** `phase-5-symbols-relocs`.

---

## 8. Milestone m6 — End-to-end smoke (PaideiaOS Phase-1 unblock)

**Slug:** `phase-5-end-to-end-smoke`
**Issues:** 5
**Governing docs:** `design/infrastructure/first-milestone.md` §1 (PaideiaOS smoke shape); `design/infrastructure/boot-path.md` §2 (long-mode entry); `crates/paideia-as/tests/data/` (existing test fixtures).

m6 is the closure milestone. By the end of m6, a `.pdx` source can:

1. Declare `let _start : () -> () = fn () -> unsafe { ... }`.
2. Use the m2 boot-ISA inside the unsafe block to write `'x'` to COM1 and `hlt`.
3. Be built with `paideia-as build --emit elf64 boot.pdx -o boot.o`.
4. Be linked with `ld -T link.ld boot.o -o kernel.elf`.
5. Be QEMU-booted via `qemu-system-x86_64 -kernel kernel.elf -serial mon:stdio -nographic`.
6. Print `x` over the serial console and halt.

This is **the test that proves PaideiaOS Phase-1 is unblocked**.

---

### m6-001. fixtures: `tests/build-emit/uart_smoke.pdx` source

- **Summary:** Author a minimal `.pdx` source that declares `_start` and uses an `unsafe` block to write a byte to COM1 (port `0x3F8`) and halt. The source uses only m2-covered instructions: `mov al, 0x78` (the ASCII for `'x'`), `mov dx, 0x3F8`, `out dx, al`, `hlt`. No GDT setup required (QEMU's `-kernel` flag arranges for long-mode with a flat memory map at entry).
- **Acceptance criteria:**
  - `tests/build-emit/uart_smoke.pdx` exists and contains the source.
  - `paideia-as check tests/build-emit/uart_smoke.pdx` exits 0 with no diagnostics.
  - The file is under 30 lines including comments.
  - A companion `tests/build-emit/uart_smoke.expected_bytes.txt` records the expected `.text` byte sequence for snapshot comparison.
- **Files:** `tests/build-emit/uart_smoke.pdx`, `tests/build-emit/uart_smoke.expected_bytes.txt`.
- **Dependencies:** m3-005, m5-003.
- **Estimated size:** XS
- **Phase:** Phase 5 — Build-emit activation.
- **Milestone:** `phase-5-end-to-end-smoke`.

---

### m6-002. fixtures: `tests/build-emit/link.ld` and `tools/run-smoke.sh` driver

- **Summary:** A minimal linker script that lays the kernel ELF at 1 MiB physical with `ENTRY(_start)`, and a shell script that invokes `paideia-as build`, `ld`, then `qemu-system-x86_64` and asserts the serial output contains `x`. The driver returns 0 on success, 1 on QEMU timeout or wrong output.
- **Acceptance criteria:**
  - `tests/build-emit/link.ld` sets `OUTPUT_FORMAT(elf64-x86-64)`, `ENTRY(_start)`, and a `.text` section at `0x100000`.
  - `tools/run-smoke.sh`: takes a `.pdx` source path, builds, links, runs QEMU for at most 5 seconds with `-no-reboot -no-shutdown -serial file:/tmp/qemu_serial.log`, then greps `/tmp/qemu_serial.log` for the expected output. Exit 0 / 1 accordingly.
  - Script handles QEMU not being installed gracefully (skipped with exit 77 = skip).
  - 5-second timeout enforced via `timeout 5 qemu-system-x86_64 ...`.
- **Files:** `tests/build-emit/link.ld`, `tools/run-smoke.sh`.
- **Dependencies:** m6-001.
- **Estimated size:** XS
- **Phase:** Phase 5 — Build-emit activation.
- **Milestone:** `phase-5-end-to-end-smoke`.

---

### m6-003. tests: byte-sequence assertion for `uart_smoke.pdx`

- **Summary:** A Rust integration test under `crates/paideia-as/tests/build_emit_smoke.rs` that builds `uart_smoke.pdx` programmatically (no shell), extracts the `.text` bytes via the `object` crate, and asserts they match the expected sequence from `uart_smoke.expected_bytes.txt` byte-for-byte. This catches encoder regressions without needing QEMU.
- **Acceptance criteria:**
  - Test invokes `paideia-as::cmd_build::run("tests/build-emit/uart_smoke.pdx", Some(tmp_path), "elf64")` and asserts exit code 0.
  - Reads the resulting `.o`, finds `.text` via the `object` crate (already a workspace dep), extracts bytes.
  - Asserts byte-for-byte match against the snapshot file. Mismatch prints both byte sequences side-by-side with a diff.
  - Also asserts `readelf -s` style: one `_start` symbol with `STB_GLOBAL` and the right `st_size`.
  - Test passes on every CI run (deterministic build per `det.rs::build_timestamp()`).
- **Files:** `crates/paideia-as/tests/build_emit_smoke.rs`.
- **Dependencies:** m6-001, m5-005.
- **Estimated size:** S
- **Phase:** Phase 5 — Build-emit activation.
- **Milestone:** `phase-5-end-to-end-smoke`.

---

### m6-004. tests: QEMU smoke under `cargo test --test qemu_smoke` (gated)

- **Summary:** A Rust integration test that shells out to `tools/run-smoke.sh` and asserts exit 0. Skipped (`return early with println!("skipped: no qemu")`) if `qemu-system-x86_64` is not on `PATH`. The test runs by default in CI when QEMU is installed.
- **Acceptance criteria:**
  - Test file `crates/paideia-as/tests/qemu_smoke.rs` exists.
  - On a host with QEMU installed, the test passes within 30 seconds.
  - On a host without QEMU, the test is auto-skipped (prints "qemu not found; skipping" and returns).
  - The Nix flake's `devShell` includes `qemu` so the test always runs locally.
  - Confirmed manually: invoking via `cargo test --test qemu_smoke` on a developer machine with QEMU shows the test pass.
- **Files:** `crates/paideia-as/tests/qemu_smoke.rs`, `flake.nix` (if `qemu` not already in dev shell — check first).
- **Dependencies:** m6-002, m6-003.
- **Estimated size:** XS
- **Phase:** Phase 5 — Build-emit activation.
- **Milestone:** `phase-5-end-to-end-smoke`.

---

### m6-005. cli + tests: `add_one` regression — `02_functions.pdx::add_one` byte-identical

- **Summary:** The Phase-1 PR 55 acceptance criterion is `fn x -> x + 1` lowers to `48 8d 47 01 c3`. m5-005 deletes the hardcoded `lower_add_one`; this issue confirms that the m1-003 walker chain reproduces the same 5 bytes for `examples/02_functions.pdx::add_one`. If a regression exists, this issue investigates and closes it.
- **Acceptance criteria:**
  - `cargo test --test build_emit_smoke -- add_one_byte_identical` passes.
  - The test invokes `cmd_build` on `examples/02_functions.pdx`, finds the `add_one` symbol in `.text`, extracts its 5 bytes, and asserts `vec![0x48, 0x8d, 0x47, 0x01, 0xc3]`.
  - The other three functions in the file (`add`, `double`, `identity`) also produce their expected byte sequences (3 additional assertions in the same test).
  - **PaideiaOS Phase-1 unblock declared:** the m6-005 commit message states "PaideiaOS Phase-1 (P1-001..010) is now unblocked: paideia-as build emits real machine code that QEMU can boot." A reference to the unblock is written into PaideiaOS's `.plans/issue-map.tsv` (entry: "phase-5-m6-005 unblocks P1-001").
- **Files:** `crates/paideia-as/tests/build_emit_smoke.rs`, `STATUS.md` (link to unblock declaration).
- **Dependencies:** m6-003, m5-005.
- **Estimated size:** XS
- **Phase:** Phase 5 — Build-emit activation.
- **Milestone:** `phase-5-end-to-end-smoke`.

---

## 9. Milestone m7 — Documentation + closure

**Slug:** `phase-5-docs-closure`
**Issues:** 4
**Governing docs:** the existing closure pattern from Phase 4 m14-001..004.

---

### m7-001. docs: `design/toolchain/phase-transition-5.md` retrospective

- **Summary:** Author a Phase 5 retrospective in the same shape as `phase-transition-4.md`: scope summary, per-milestone outcomes, carryover disposition (none expected since no Phase-4 → Phase-5 carryover items remained), what didn't ship, what we got right, what we'd change, Phase-5 → Phase-6 carryover (self-hosting list per `design/toolchain/self-hosting-phase5-plan.md`).
- **Acceptance criteria:**
  - `design/toolchain/phase-transition-5.md` exists, < 250 lines.
  - Contains the standard sections: §0 scope, §1 carryover disposition, §2 honest list, §3 right calls, §4 changes, §5 Phase-6 carryover.
  - The Phase-6 carryover list contains the original Phase-5 self-hosting plan items (T1/T2/T3 from `self-hosting-phase5-plan.md` §3).
  - The honest list documents which surface features still don't reach `build` (records, generics, traits, borrowed-refs, stdlib types) and the rationale (PaideiaOS Phase-1 doesn't need them; Phase 6+ will).
- **Files:** `design/toolchain/phase-transition-5.md`.
- **Dependencies:** m6-005.
- **Estimated size:** XS
- **Phase:** Phase 5 — Build-emit activation.
- **Milestone:** `phase-5-docs-closure`.

---

### m7-002. docs: STATUS.md Phase 5 closure section

- **Summary:** Prepend a Phase 5 closure section to `STATUS.md` (matching the Phase 4 m14-002 pattern from the existing file). Lists each m1..m7 milestone, the issues that closed it, and a workspace-test count delta vs Phase 4 close (2172 tests).
- **Acceptance criteria:**
  - `STATUS.md` gains a "Phase 5 milestone closure (m1–m7)" section above the Phase 4 section.
  - Each milestone has a one-line summary plus the list of issue IDs.
  - The "Workspace test totals" table grows a Phase-5-close row with the new test count.
  - The "Where to look next" section adds `design/toolchain/phase-transition-5.md`.
- **Files:** `STATUS.md`.
- **Dependencies:** m7-001.
- **Estimated size:** XS
- **Phase:** Phase 5 — Build-emit activation.
- **Milestone:** `phase-5-docs-closure`.

---

### m7-003. release: v0.5.0 tag + CHANGELOG Phase 5 section

- **Summary:** Bump the workspace version from 0.4.0 (m14-003) to 0.5.0; author the CHANGELOG section listing the build-emit activation, the encoder boot-ISA family, the unsafe walker, the static-data surface, and the symbol/relocation pipeline. Tag the commit as `v0.5.0`.
- **Acceptance criteria:**
  - `Cargo.toml` workspace `version = "0.5.0"`.
  - `CHANGELOG.md` gains a `## [0.5.0] — 2026-MM-DD` section listing the new capabilities and the deferred items (self-hosting → Phase 6+).
  - `git tag v0.5.0 <closure-sha>` exists locally; pushed in the same commit window.
  - `cargo build --workspace` clean post-bump.
  - The Phase-5 issue map (`.plans/issue-map.tsv` extension) maps each m1..m7 issue ID to its closing PR/commit.
- **Files:** `Cargo.toml`, `CHANGELOG.md`, `.plans/issue-map.tsv`.
- **Dependencies:** m7-002.
- **Estimated size:** XS
- **Phase:** Phase 5 — Build-emit activation.
- **Milestone:** `phase-5-docs-closure`.

---

### m7-004. examples: build-clean parity for the build-emit subset

- **Summary:** Walk through `examples/01_hello.pdx`, `examples/02_functions.pdx`, `examples/15_unsafe.pdx` and confirm each one not only `check`s but also `build`s without diagnostics — and produces a non-empty `.text` section. Examples that exercise out-of-Phase-5-scope surface (records / generics / borrowed-refs / stdlib types — `03`, `04`, `07`, `08`, `09`, `10`, `11`, `12`, `13`, `14`) get a single-line comment header stating "Phase 6+: `build` activation deferred — `check` passes today."
- **Acceptance criteria:**
  - `01_hello.pdx`, `02_functions.pdx`, `15_unsafe.pdx` each build to a valid `.o` whose `.text` is non-empty.
  - `examples/README.md` gains a per-example status table: column "check" (all pass), column "build" (3 pass, 17 deferred-with-rationale).
  - The 17 deferred examples each have their build-block reason recorded (one-line per example).
  - The `tests/examples-corpus.rs` regression test (extends or creates one) exercises `paideia-as build` on the 3 build-clean examples and asserts exit 0.
- **Files:** `examples/01_hello.pdx`, `examples/02_functions.pdx`, `examples/15_unsafe.pdx` (header comments only), `examples/README.md`, `tests/examples-corpus.rs`.
- **Dependencies:** m6-005.
- **Estimated size:** S
- **Phase:** Phase 5 — Build-emit activation.
- **Milestone:** `phase-5-docs-closure`.

---

## 10. Dependency graph (textual)

```text
  m1:  m1-001 → m1-002 → m1-003 → m1-005
                m1-001 → m1-004 ─→ m1-005

  m2:  m2-001 → m2-002 (zero-operand)
       m2-001 → m2-003 (in/out)
       m2-001 → m2-004 (wrmsr/rdmsr/int)
       m2-001 → m2-005 (mov cr*)
       m2-001 → m2-006 (mov dr*)
       m2-001 → m2-007 (lgdt/lidt)
       m2-001 → m2-008 (iret/iretq/sysret)
       m2-001 → m2-009 (rep stosq)
       m2-001 → m2-010 (far jmp)

  m3:  m3-001 → m3-002 → m3-003 (depends also on m2-001)
                m3-003 → m3-004
       (m1-005 + m3-004) → m3-005

  m4:  m4-001 → m4-002 → m4-003 (depends also on m1-002)
                          m4-003 → m4-004 (depends also on m5-002)

  m5:  (m1-005)             → m5-001
       (m2-001)              → m5-002
       (m5-001+m1-005+m4-003)→ m5-003
       (m5-002+m5-003)       → m5-004
       (m1-005+m2-001+m3-005+m4-003+m5-003+m5-004) → m5-005

  m6:  (m3-005+m5-003) → m6-001 → m6-002 → m6-004
       (m6-001+m5-005) → m6-003 → m6-005

  m7:  m6-005 → m7-001 → m7-002 → m7-003
       m6-005 → m7-004
```

**Critical path (18 issues):**

```text
m1-001 → m1-002 → m3-001 → m3-002 → m3-003 → m2-001 → m2-008 →
m4-001 → m4-003 → m5-001 → m5-003 → m5-005 → m6-001 → m6-002 →
m6-003 → m6-005 → m7-001 → m7-003
```

At a sustainable cadence of ~2 issues/week (matching the paideia-as-osarch-plan and PaideiaOS-osarch-plan baselines), the wall-clock floor is **9 weeks**; parallel work on the non-critical m2-002..010, m4-002, m4-004, m6-004, m7-002, m7-004 sub-tracks plus the m3 / m4 / m5 parallel branches compresses real-world delivery into 5–7 weeks for a solo developer.

---

## 11. PaideiaOS Phase-1 unblocking — explicit declaration

The single milestone that unblocks PaideiaOS Phase-1 (`PaideiaOS/.plans/paideia-os-osarch-plan.md` tasks P1-001..014) is **m6 — End-to-end smoke**, specifically **m6-005**.

Why m6 and not earlier:

- m1 alone produces an `InstructionSideTable` but doesn't reach the ELF.
- m2 alone covers the boot-ISA but those mnemonics never make it into the elaborator's emit path.
- m3 alone parses unsafe blocks into instructions but the symbols are not exported.
- m4 alone emits `.rodata` but without instruction references the data is unreachable.
- m5 alone emits symbols and relocations but the bytes in `.text` are still the placeholder until m5-005 deletes `lower_add_one`.

Only at m6-005 does the chain prove end-to-end: a `.pdx` source becomes a QEMU-bootable kernel. **The commit message for m6-005 explicitly declares the PaideiaOS Phase-1 unblock**, and `PaideiaOS/.plans/issue-map.tsv` records the cross-repo dependency closure ("phase-5-m6-005 unblocks P1-001").

P1-002..014 then proceeds in PaideiaOS's own milestone cadence; the paideia-as side is done for Phase 1.

---

## 12. Notes on what is deliberately deferred

The following are *not* in this plan, in keeping with the user's "stop the moment QEMU boots" constraint:

- **Self-hosting** (the original Phase 5 plan): paideia-as-lexer / parser / ast / diagnostics / types / effects / ir / elaborator / encoder / linker / dwarf / emitter-elf/pax/pe ports. **Becomes Phase 6+** per the existing `design/toolchain/self-hosting-phase5-plan.md` (the document stays as the Phase-6 blueprint).
- **Build-emit for records, enums, generics, traits, borrowed references, stdlib types, loops** — none of these appear in PaideiaOS Phase-1 boot code, so their walker-chain activation is Phase 6+.
- **DWARF emission in the ELF path.** The `paideia-as-dwarf` crate exists (Phase-1 PR 57); wiring `.debug_info` / `.debug_line` into `--emit elf64` is Phase 6+. PaideiaOS Phase-1 source-level debugging uses raw asm-reference disassembly in the meantime.
- **Optimisation passes in the build path.** The m3-007 / m1-007..010 flips ship at the side-table level today; activation in the build path is Phase 6+.
- **Effect-handler runtime materialisation.** `unsafe` blocks declare effects but they are not enforced at emit time. The Phase-1 PR 51 effect rewrite operates on `IrPerform`, which boot code does not use.
- **paideia-as-linker integration.** PaideiaOS Phase-1 uses GNU `ld` directly; the `paideia-as-linker` crate's activation is Phase 6+.
- **Real PE/COFF / PAX activation for the boot intrinsics.** PE/COFF already consumes `InstructionSideTable` via `emit_text_from_instructions`; once m1/m2/m3 land, PE will pick up the new mnemonics automatically (encoder bridge is shared). PAX consumes hashes and stays correct. No new work needed there for Phase 5 closure.
- **Macro expansion in the build path.** The Phase-1 PRs 46–49 shipped the macro pipeline; activation in `build` for non-trivial macros is Phase 6+. PaideiaOS Phase-1 uses no macros.
- **Multi-file build orchestration.** `paideia-as build a.pdx b.pdx -o linked.elf` is Phase 6+ (today: one `.pdx` per invocation; PaideiaOS uses a Makefile to glue them).

The 38 issues above are the smallest disciplined sequence that unblocks PaideiaOS Phase-1 without overshooting into Phase-6 territory.
