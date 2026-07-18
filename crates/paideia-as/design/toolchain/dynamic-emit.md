# Dynamic emit — the paideia-as runtime API

This document describes the public API for emitting x86_64 instructions at runtime.
It is the reference specification for JIT compilers, WASM engines, and self-hosting
scenarios that need to generate executable code on the fly.

## 1. Purpose

The paideia-as dynamic-emit surface exists to serve three primary consumers:

- **JIT compilation** — Languages or engines that compile to x86_64 at runtime.
- **WASM/VM execution** — paideia-os Phase 10 (userspace WASM jail) and other
  virtual machine implementations.
- **Self-hosting** — paideia-as v0.20 self-hosting bootstrap can re-emit the
  IR's instruction nodes without going through the full encoder pipeline.

Deliberately out of scope:

- **Linker functionality** — `emit_instruction` produces raw bytes, not relocatable
  object files (.o). Cross-module linking belongs to a linker, not the emitter.
- **DWARF / debug info** — Instruction emission is independent of debug-line tables.
- **REX optimization / peephole passes** — The encoder emits the correct bytes for
  a given `Instruction`; downstream tightening (e.g., rel8 vs rel32 size selection)
  is tracked in v0.21 milestones.

## 2. The three imports

Every consumer starts with the same minimal imports:

```rust
use paideia_as_emit::{emit_instruction, CodeBuffer, EmitError};
use paideia_as_runtime::{Instruction, InstrMode, Mnemonic, Operand, RegId};
use smallvec::smallvec;
```

- **`CodeBuffer`** — A growable byte vector that accumulates the emitted instruction stream.
- **`emit_instruction(buf, ins)`** — The single public entry point. Takes a buffer and
  an Instruction, emits bytes, and handles rollback on error.
- **`EmitError`** — Error taxonomy (operand count mismatch, shape mismatch, unresolved
  relocations, unsupported mnemonics).
- **`Instruction`** — A struct-literal record describing a single x86_64 instruction:
  mnemonic, operands, encoding mode (64-bit or 32-bit), and metadata.

## 3. The three-step recipe

All consumers follow this pattern:

### Step 1: Build an Instruction

```rust
fn instr(mnemonic: Mnemonic, operands: &[Operand]) -> Instruction {
    Instruction {
        mnemonic,
        operands: operands.iter().cloned().collect(),
        encoding_hint: None,
        byte_offset_in_text: None,
        mode: InstrMode::Mode64,
        emission_order: 0,
    }
}

let add = instr(Mnemonic::Add, &[
    Operand::Reg(RegId(0)),  // RAX
    Operand::Reg(RegId(3)),  // RBX
]);
```

The `Instruction` struct is intentionally verbose to expose all fields — no hidden
magic. Consumers build instances via struct literals or (as shown) a small local helper.
The fields are:

- `mnemonic: Mnemonic` — The CPU instruction (Add, Mov, Ret, Call, etc.).
- `operands: SmallVec<[Operand; 4]>` — Operands: registers, immediates, memory forms.
- `encoding_hint: Option<...>` — Advanced feature for disambiguating instruction forms;
  leave as `None` for standard emission.
- `byte_offset_in_text: Option<u32>` — Source location tracking; leave as `None`.
- `mode: InstrMode` — Execution mode (Mode64 for 64-bit, Mode32 for 32-bit).
- `emission_order: u32` — Ordering hint for the encoder; leave as `0`.

### Step 2: emit_instruction(&mut buf, ins)

```rust
let mut buf = CodeBuffer::new();
let result = emit_instruction(&mut buf, add)?;
```

On success, `buf.bytes` has been extended with the encoded bytes of the instruction.
On error, the buffer is rolled back to its pre-call state (no partial emissions).

### Step 3: buf.bytes or buf.as_slice()

```rust
let code: &[u8] = buf.as_slice();
// Now code is the raw x86_64 byte stream. Consumers then:
// - Copy to executable memory (JIT).
// - Sign + encrypt (paideia-os Phase 10).
// - Link into a .o file (self-hosting).
```

## 4. Handling symbol references — resolve_symbols

Not all instructions are self-contained. Instructions involving external symbols
(function calls, data references) require a symbol-resolution pass before emission.

**When you need resolve_symbols:**

```rust
let instruction_stream = vec![
    // ...
    instr(Mnemonic::Call, &[
        Operand::SymbolRef { name: "printf".into(), addend: 0 }
    ]),
    // ...
];

// Resolve symbols first (returns Ok if all symbols were in the table).
let symbol_table = SymbolTable::new();
symbol_table.define("printf", 0x401050);
let resolved = resolve_symbols(&instruction_stream, &symbol_table, &mut label_map)?;

// Now emit the resolved instructions (they have no SymbolRef operands left).
for ins in resolved {
    emit_instruction(&mut buf, ins)?;
}
```

See `crates/paideia-as-runtime/src/resolve.rs` for the full `resolve_symbols` API
and its contract. For self-contained instructions (like our i32.add example),
skip this step entirely.

## 5. Error taxonomy

`EmitError` variants and what a consumer does with each:

| Variant | Meaning | Action |
|---------|---------|--------|
| `OperandCount` | Mnemonic arity doesn't match | Logic error; audit the Instruction construction. |
| `OperandShape` | Operand combination is invalid for this mnemonic | E.g., immediate as destination. Audit the Instruction. |
| `InvalidOperand` | An operand is structurally invalid | E.g., unresolved `Var` operand. Call `resolve_symbols` first. |
| `Unsupported` | This mnemonic/form is not in the encoder | Feature gate or v0.20 coverage gap. File an issue. |
| `UnresolvedRelocation` | Operand is `SymbolRef`, `LabelRef`, etc. | Call `resolve_symbols` before emitting. |

All errors result in **buffer rollback**: the buffer is left exactly as it was before
the failed `emit_instruction` call, so it's safe to retry or emit a different instruction.

## 6. Determinism guarantees

**v0.20 exit criterion**: Byte-identical emit across host platforms.

This is a load-bearing invariant for paideia-os Phase 10 (WASM jail signatures must
be reproducible). Consumers may assume:

1. **emit_instruction is a pure function** — Same input (Instruction) → same output bytes
   on every host (Linux x86_64, macOS x86_64, etc.).
2. **No floating-point arithmetic in the encoder** — All calculations are integer arithmetic.
3. **No syscalls or I/O** — Emission is deterministic independent of system state.

This makes the emitter suitable for property-based testing: fuzz an `Instruction`,
verify that emit is deterministic, and that the bytes round-trip through the decoder.

## 7. Worked example — the WASM i32.add lowering

See `crates/paideia-as-emit/examples/wasm_add.rs` for a complete, runnable example.

The load-bearing part (repeated by every downstream consumer):

```rust
// Decode WASM opcodes
fn decode_body(bytes: &[u8]) -> Result<Vec<WasmOp>, &'static str> { ... }

// Lowercase table: WASM → x86_64 Instruction
fn lower(op: &WasmOp, stack_depth: usize) -> Vec<Instruction> {
    match op {
        WasmOp::LocalGet(idx) => vec![instr(Mnemonic::Mov, &[ /* */ ])],
        WasmOp::I32Add => vec![instr(Mnemonic::Add, &[ /* */ ])],
        WasmOp::End => vec![instr(Mnemonic::Ret, &[])],
    }
}

// Emit driver
fn emit_body(ops: &[WasmOp]) -> Result<Vec<u8>, EmitError> {
    let mut buf = CodeBuffer::new();
    for op in ops {
        for ins in lower(op, depth) {
            emit_instruction(&mut buf, ins)?;
        }
    }
    Ok(buf.bytes)
}
```

The pattern:

1. Decode source opcodes (WASM, IR, bytecode, etc.) into your domain model.
2. Map each opcode to zero or more x86_64 Instructions (the lowering table).
3. Loop: `emit_instruction` each Instruction into a shared CodeBuffer.

This pattern scales: adding new opcodes (i32.sub, i32.mul, etc.) is a one-liner
in the lowering table.

## 8. Where this cannot help you (yet)

Known gaps slated for future milestones:

- **JIT-page allocation** — `emit_instruction` produces bytes; the consumer owns
  mmap/mprotect/ExecutableMemory. paideia-os Phase 10 will stabilize a kernel-side
  page-allocation policy, then expose an alloc callback here.

- **Cross-block relocation** — Instructions within a single emit pass cannot call
  out to functions in other blocks without going through `resolve_symbols`.
  Inter-procedural jumps need symbol/label resolution infrastructure (in progress).

- **REX-space optimization** — The encoder emits correct bytes but does not apply
  tightening passes (e.g., `mov r64, imm64` → `mov r32, imm32` when the value
  fits). v0.21 will add this.

- **Operand form selection** — When a mnemonic has multiple encodings (e.g.,
  `add r64, r64` vs. `add r64, imm32`), the encoder picks based on the operand
  types. Disambiguation is automatic, but if you need a specific form, use
  `encoding_hint`.

---

For questions or to report an issue with the runtime API, see
`crates/paideia-as-emit/src/lib.rs` and the associated tests in
`crates/paideia-as-emit/tests/emit_api.rs`.
