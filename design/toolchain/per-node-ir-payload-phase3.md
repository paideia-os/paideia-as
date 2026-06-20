# Per-node instruction payload (Phase 3 m2)

**Status:** Phase 3 m2 closure appendix.
**Scope:** Documents the Instruction payload schema, the side-table
convention, the encoder bridge, and the deferred per-mnemonic
extensions that ship in Phase 3 m3.

## 0. Origin

The original spec `design/toolchain/custom-assembler.md` lives upstream
in the `paideia-os/paideia-os` repository. This appendix is the local
companion the upstream §6.1 cross-reference will point at.

## 1. The schema (m2-001)

The kind-only IR (m1-002) carries no per-node x86_64 metadata —
mnemonic, operand list, encoding hint all live in a side-table
`InstructionSideTable` keyed by `IrNodeId`. This is the canonical
pattern from m3-007's `HandlerSideTable` and m1-006's
`LoadStoreSideTable`: keep `IrNodeData` ≤ 48 bytes (pinned by
`const_assert`); per-node metadata grows in dedicated tables.

The payload (`paideia-as-ir::Instruction`):

```rust
pub struct Instruction {
    pub mnemonic: Mnemonic,
    pub operands: SmallVec<[Operand; 3]>,
    pub encoding_hint: Option<EncodingHint>,
}
```

- `Mnemonic` enum: 10 variants — `Mov / Add / Sub / Cmp / Jcc(Cond) /
  Jmp / Call / Ret / RepMovsb / Lea`. Phase 3 m2 minimum coverage;
  the m9 opt-pass catalog drives the selection.
- `Cond` enum: 16 condition codes (`Eq / Ne / Lt / Le / Gt / Ge /
  Below / BelowOrEqual / Above / AboveOrEqual / Zero / NonZero / Sign
  / NotSign / Overflow / NotOverflow / Parity / NotParity`).
- `Operand` enum: 4 variants — `Reg(RegId) / Imm64(i64) /
  MemSib { base, index, scale, disp } / MemDisp { disp }`.
- `RegId` is `u8` (0..15 for RAX..R15); the encoder side owns the
  canonical register-name table.
- `Scale`: `X1 / X2 / X4 / X8` with `factor() → u32` and `from_factor(u32)
  → Option<Scale>`.
- `EncodingHint { opcode: u16, operand_size: u8 }` — phase-3-m2-001
  minimum (opcode + operand-size override).

## 2. Side-table convention (m2-001)

`InstructionSideTable` mirrors `LoadStoreSideTable` exactly:

```rust
pub struct InstructionSideTable {
    entries: HashMap<IrNodeId, Instruction>,
}
```

API: `new / insert / get / get_mut / remove / len / is_empty`. There
is intentionally no batch insert / no bulk read — the table is
populated incrementally by the elaborator's chokepoint (§3) and read
incrementally by opt passes (§5).

## 3. The elaborator chokepoint (m2-003)

`paideia-as-elaborator::populate::populate_instruction_table`:

```rust
pub struct PopulateContext<'a> {
    pub arena: &'a IrArena,
    pub load_store: &'a LoadStoreSideTable,
}

pub fn populate_instruction_table(
    ctx: &PopulateContext,
    table: &mut InstructionSideTable,
) -> usize
```

Walks every node in the arena, recognises `Load` / `Store` (m1-006),
and inserts the corresponding `Instruction` record.

Phase-3-m2-003 minimum recognition: `Load` → `Mov` (opcode `0x8B`);
`Store` → `Mov` (opcode `0x89`); everything else returns `false`
(skip — honest non-recognition).

Honest scaffolding: the placeholder `node_to_reg(_id)` returns `RDI`
for every node. Real SSA-style register allocation ships in m3 with
the opt-pass real-rewrite work.

## 4. The encoder bridge (m2-002)

`paideia-as-encoder::encode_instruction`:

```rust
pub fn encode_instruction(
    inst: &Instruction,
    buf: &mut CodeBuffer,
) -> Result<(), EncodeError>
```

A single dispatch entry that turns an `Instruction` into bytes by
delegating to per-mnemonic encoders in `encode.rs`. Per-mnemonic
shims cover the common operand shapes for all 10 mnemonics; less-
common shapes (memory destinations, RIP-relative, etc.) return
`EncodeError::Unsupported`.

Round-trip tests via `iced-x86` (added to `[dev-dependencies]`)
disassemble each emitted sequence and assert the mnemonic + operand
match.

## 5. Opt-pass helper signatures (m2-004)

The m9 helpers' public signatures speak the m2-001 vocabulary now:

```rust
schedule_block(&InstructionSideTable, &[IrNodeId]) -> Vec<usize>
dse_block(&InstructionSideTable, &[IrNodeId]) -> Vec<usize>
tco_blocker(&InstructionSideTable, IrNodeId) -> Option<TcoBlocker>
is_unroll_safe(&InstructionSideTable, IrNodeId, u32) -> bool
```

The phase-2 helper bodies are preserved as `*_impl` internal
functions so their extensive existing test coverage stays green. The
new helpers stub to conservative defaults today (identity
permutation; eligible; not safe). Per-mnemonic analysis body ports
happen in Phase 3 m3 (opt-pass real-rewrites), exactly when the
populate path actually threads through the compile flow.

## 6. Regression corpus (m2-005)

`tests/ir-payload/` is a workspace member with 8 fixtures pinning
the populate contract:

- **Active (4 tests across 2 fixtures)**: `leaf_load.rs`,
  `leaf_store.rs` — synthetic IR construction + assertion of the
  populated `Instruction` record (mnemonic + opcode +
  operand_size).
- **Ignored (12 tests across 6 fixtures)**: `conditional_branch.rs`,
  `tail_call.rs`, `indexed_accumulator.rs`, `rep_movsb.rs`,
  `per_byte_scan.rs`, `multi_call_body.rs` — each carries an
  `#[ignore]` skip reason naming the populate-side path that needs
  to land for activation (the m3-002 / m3-005 / etc. wires).

## 7. Deferred to Phase 3 m3

m3 is "opt-pass real-rewrites." It ships:

- Per-mnemonic populate-path expansion (Call / Jmp / Jcc / Add /
  Sub / Cmp / Lea / RepMovsb beyond the m2-003 Load/Store seed).
- Per-mnemonic body ports for the m9 helpers: schedule_block_impl
  → mnemonic-driven; dse_block_impl → MemOp extraction from
  `Operand::MemSib`; tco_blocker_impl → real call-site
  introspection; is_unroll_safe_impl → real trip-count metadata.
- The full m9 "would-fire" → real-rewrite flip across all 11 passes
  (O1500..O1512).
- Activation of the 6 `#[ignore]`'d fixtures in `tests/ir-payload/`.

## 8. Forward links

- Upstream `custom-assembler.md` §6.1 cross-reference: follow-up PR
  on `paideia-os/paideia-os`.
- m3 milestone (`phase3-m3-opt-pass-real-rewrites`): per-mnemonic
  populate expansion + body ports + 6 fixture activations.
- Future expansion of the mnemonic catalog (PUSH / POP / XOR / TEST
  / SHL/SHR / MOVZX / MOVSX / IMUL / IDIV / CMOV / SETcc): a follow-
  up beyond Phase 3 m2.

## 9. Phase-4 cross-link

The per-mnemonic populate-path expansion deferred above lands at
**Phase 4 m1-001..004** (walker hookups):

- m1-001 — Call-node introspection (`CallSideTable` + intrinsic-call
  recognition in the populate path).
- m1-002 — Match arm walker surface (per-arm scope; S0904 fires).
- m1-003 — Handle clause walker surface (HandlerSideTable populates).
- m1-004 — Branch (if/else) walker surface (per-branch scope;
  m3-005 recursion check gates lift).

The 4-pass m3-007 would-fire flip (macro-fusion, branch-hint, align,
pool-constants) lands at **Phase 4 m1-007..010**, leaving 9/10 m3
passes as real-rewrite + 1 (unroll) awaiting m3-006 body-duplication
closure.

See `design/toolchain/walker-hookups-phase4.md` for the full Phase 4
m1 closure narrative.
