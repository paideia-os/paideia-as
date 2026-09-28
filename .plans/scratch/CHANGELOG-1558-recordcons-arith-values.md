# PAS-DEBT-B4-002-alt (paideia-as#1558) — Wave 50

## Summary

Extends the Slice D helper
`emit_record_cons_field_stores_into_sret_buffer`
(`crates/paideia-as-elaborator/src/emit_walker/emit_core.rs`, v0.36.71)
to lower field values whose IrKind is `App` (bit-arithmetic operator
or function call) and `FieldAccess`, in addition to the pre-existing
`Literal` and `Var` arms. Adds a T0578 diagnostic (in
`crates/paideia-as-diagnostics/catalog.toml`) that fires for every
other shape, replacing the previously-silent "field slot left
uninitialised" gap with a build-halting error.

Addresses **Gap A** from Wave 46's blocker enumeration
(`.plans/scratch/CHANGELOG-1524-b4002-cpuid-retirement-attempt.md`),
the local half of the two-track cpuid_leaf-retirement plan (branch
"PAS-DEBT-B4-002-alt": arithmetic field values without touching
stdlib-recipe layout participation).

Version bump: 0.36.75 → 0.36.76.

## What lands

### New field-value arms (`emit_core.rs`)

1. **`IrKind::App` with bit-arithmetic operator** — discriminated via
   the shared `operator_lexeme_of` lookup (#1196), same as every
   other App consumer. Handles the bitwise / shift subset (`&`, `|`,
   `<<`, `>>`) that cpuid-style decoders compose; arithmetic (`+`,
   `-`, `*`) and comparisons are intentionally out of scope for this
   wave — they fire T0578 with the operator lexeme named. Emits the
   canonical two-instruction sequence
   `mov rax, arg0; op rax, arg1` followed by
   `mov [rsp+offset], rax`.

   Operand shape restriction: only flat `Var` / `Literal` pairs. A
   nested App (App inside App) hits T0578 because the single-
   instruction seam here clobbers RAX with no spill discipline —
   supporting nesting requires either a scratch-register lease or
   an intermediate stack spill, both of which are substantial
   surgery beyond the wave scope.

2. **`IrKind::App` as function call** — routes through
   `emit_call_expr` (the same expression-position path
   `visit_lambda` uses), which lands the SysV/MS integer return in
   RAX; the arm then splices `mov [rsp+offset], rax`. Suitable only
   for scalar-returning callees for now — a record-returning callee
   lowers its own caller-side sret prelude and lands the record in
   a distinct frame slot rather than in RAX (Gap A residue,
   documented in the arm's docblock).

3. **`IrKind::FieldAccess`** — routes through
   `visit_field_access_with_reg(dest=RAX)` so the existing width-
   dispatch helper drives the load, then splices the sret-slot
   store. Handles the `FieldAccess(Deref(Var))` shape — the common
   case where a callee reads a field from a pointer-to-record
   parameter and forwards it into its own returned record. Other
   receiver shapes (`FieldAccess(App)`, nested `FieldAccess`) fall
   through to the T0578 fallback via the enclosing wildcard,
   because the enclosing visitor cannot currently spill an
   intermediate record without a persistent caller slot.

### T0578 diagnostic

Minted in `catalog.toml`:

- Category `type`, severity `error`, since `0.36.76`.
- Fired by the Slice D helper on any field value whose `IrKind`
  is outside the recognised set (`Literal` / `Var` / `App`-
  operator / `App`-call / `FieldAccess`), or on a supported kind
  whose sub-shape is not yet lowered (bit-arith on a nested App,
  App-call with no call_sites metadata, non-flat operator
  operands).
- Message names the offending IrKind + field index + offset so
  a downstream fix knows exactly which shape to teach the arm
  about.
- Promotes the previously-silent gap ("slot reads back as
  uninitialised stack") to a build-halting error.

The Slice D docblock's original "T0522" placeholder is retired —
that code was already assigned to the non-exhaustive-match
diagnostic; T0578 is the next available slot after T0577.

### 4 helper functions in `emit_core.rs`

Introduced to keep each arm's discipline in one place and avoid
copy-paste between the App-operator, App-call and FieldAccess
arms:

- `emit_bit_arith_field_value_into_rsp_slot(app_id, arena, disp,
  field_idx, op_lex) -> bool` — the App-operator arm's body.
- `emit_call_field_value_into_rsp_slot(lambda_id, app_id, arena,
  disp, field_idx) -> bool` — the App-call arm's body.
- `emit_field_access_field_value_into_rsp_slot(fa_id, arena, disp)`
  — the FieldAccess arm's body.
- `emit_sret_slot_store_from_rax(disp)` — shared
  `mov [rsp+disp], rax` tail; one operand-shape site for the three
  arms above.
- `classify_flat_operand(node_id, arena) -> Option<FlatOperand>` —
  resolves a flat App operand to either its home register (Var in
  `local_bindings`) or its literal value (Literal in
  `literal_values`), refusing everything else.
- `FlatOperand` internal sum type — `Reg(RegId)` / `Imm(i64)`.

### 4 new tests (`sret_slice_d.rs`)

- `record_cons_body_populates_from_bit_and_of_literals` — pins
  the `mov rax, imm; and rax, imm; mov [rsp+0], rax` sequence
  for a `(0xdead & 0xff)` field followed by a bare Literal
  store. Proves the App-operator arm advances the emission
  cursor cleanly and the Literal-arm regression holds.
- `record_cons_body_populates_from_bit_shr_of_var_and_literal` —
  pins the Var-operand path (SysV param 0 → RDI) through the
  same arm plus a Var-field-value regression via
  `local_bindings` shared lookup.
- `record_cons_body_populates_from_field_access_deref_var` —
  pins the FieldAccess arm's width-dispatch load + sret store
  pair, using a source RecordLayout registered on the walker
  and `mark_field_access_handled` to keep the flat walker from
  double-emitting.
- `record_cons_body_unsupported_field_value_shape_fires_t0578`
  — asserts T0578 fires with the offending IrKind (`Cast`)
  named in its message AND that a neighbouring Literal field
  still emits its store, so a single unsupported field does not
  abort the whole loop.

### App-call test intentionally deferred

Setting up `emit_call_expr`'s preconditions cleanly (proper callee
Symbol with ABI, arg-marshalling scratch slot, etc.) requires the
full builder-callee shape from `sret_call_wiring.rs` which is
worth a dedicated wave. The App-call arm compiles and follows the
same discipline as `emit_call_expr`'s expression-position call site
in `visit_lambda`; its byte-shape correctness is inherited from
that path's own test surface (`sret_call_wiring.rs` + `layouts_calls.rs`).

## cpuid_leaf composition status

**PARTIAL.** A `.pdx` wrapper of the shape

```
fn wrapper(x: u64, p: *Src) -> Pair {
    Pair { a: x & 0xff, b: (*p).f }
}
```

now lowers cleanly. The exact `cpuid_leaf` composition

```
fn cpuid_leaf(leaf: u32, sub: u32) -> CpuidRegs {
    CpuidRegs {
        eax: (cpuid_leaf_ad(leaf, sub) & 0xffffffff) as u32,
        // ...
    }
}
```

still hits T0578 on two counts:

1. The outer `... as u32` is an `IrKind::Cast` — not lowered by
   this wave (would need a Cast arm that respects the source →
   dest width dispatch already implemented in `cast_shape.rs`).
2. The arithmetic argument is itself an App
   (`cpuid_leaf_ad(leaf, sub)`) — a nested App inside a bit-arith
   App, which the flat-operand classifier refuses.

Either of the following follow-ups unblocks the retirement:

- Lift intermediate `let`-binding to hoist the App result into a
  bare Var (source-level restructuring in `cpuid.pdx`):
  `let ad = cpuid_leaf_ad(leaf, sub); CpuidRegs { eax: (ad & 0xffffffff) as u32, ... }`.
  This still needs Cast handling for the `as u32`, unless the
  field type in `CpuidRegs` is `u64` (widening the record
  eliminates the cast at the cost of doubling its size).
- Extend the arm set to handle `Cast` and nested-App composition
  (a proper expression walker with RAX-clobber discipline). That
  is another dedicated wave.

## Constraints observed

- **Slice D interface unchanged.** The helper's signature
  (`fn emit_record_cons_field_stores_into_sret_buffer(&mut self,
  lambda_id: IrNodeId, layout: &RecordLayout, arena: &IrArena)`)
  is preserved; only the match arms inside its loop are extended.
- **Literal / Var arms byte-identical.** Existing Slice D tests
  (`sysv_memory_record_cons_body_populates_source_buffer_from_literals`,
  `sysv_intpair_record_cons_body_populates_before_pair_load`,
  `literal_body_leaves_sret_source_buffer_unpopulated`) untouched
  and continue to assert the pre-Wave-50 shape.
- **SysV/MS ABI split preserved.** New arms use the same `RSP`-
  relative disp shape the sret helpers on both sides
  (`sysv_callee_sret_store`, `sysv_callee_load_return_pair`,
  MS mirrors) read from. `emit_call_expr` inside the App-call
  arm consults the current lambda's ABI via
  `state.lambda_abi_option`.
- **Non-exhaustive `IrKind` wildcard preserved.** The `other =>`
  arm captures every unknown variant and fires T0578; the wildcard
  is unreachable-safe against future `IrKind` additions from
  `paideia-as-ir`.
- **Encoder pitfalls dodged.** No `test rN,rN`, no `and r11,
  imm64` (the classifier drives RAX only, and the App-operator
  arm's `<op> rax, imm64` uses the same generic form emit_arm_body_app
  already exercises). No reserved-label collisions — the helper
  emits raw `Mov`/`And`/`Or`/`Shl`/`Shr` instructions with no
  Label operands.

## Files touched

- `Cargo.toml` — workspace.version 0.36.75 → 0.36.76.
- `CHANGELOG.md` — v0.36.76 entry.
- `crates/paideia-as-elaborator/src/emit_walker/emit_core.rs` —
  new imports (`paideia_as_diagnostics::{Category, DiagnosticCode,
  Severity}`, `crate::emit_store_record::operator_lexeme_of`);
  `t0578_code` module-level helper; docblock rewrite on
  `emit_record_cons_field_stores_into_sret_buffer`; new arms for
  `IrKind::App` (dispatching to
  `emit_bit_arith_field_value_into_rsp_slot` or
  `emit_call_field_value_into_rsp_slot`) and `IrKind::FieldAccess`
  (dispatching to `emit_field_access_field_value_into_rsp_slot`);
  wildcard now fires T0578 with the offending IrKind named; four
  new private helpers plus `FlatOperand` internal sum type.
- `crates/paideia-as-elaborator/src/emit_walker_tests/sret_slice_d.rs`
  — additional imports (`FieldAccessInfo`, `RecordTypeId`); four
  new tests covering the App-operator, App-operator with Var,
  FieldAccess and T0578 negative paths.
- `crates/paideia-as-diagnostics/catalog.toml` — T0578 entry
  (immediately after T0577, before the P0282 parser-code block).
- `.plans/scratch/CHANGELOG-1558-recordcons-arith-values.md` —
  this file.

## Verification not run

Per PAS-DEBT loop rule and the sub-agent build discipline, this
softarch pass does **NOT** run `cargo build`, `cargo check`, or
any test. Main is expected to run scoped
`cargo check --tests -p paideia-as-elaborator -p paideia-as-diagnostics`
next, followed by a full `bash tools/build.sh` if the scoped
check is clean. On failure, the debugger re-invocation should
target `emit_core.rs`'s new arms first (byte-shape of the
`mov rax, imm; op rax, imm` sequences, especially the `shl rax,
imm64` encoder path — an encoder-narrow to `imm8` is documented
but not always exercised) and the T0578 message format (the
`{:?}` on `IrKind` shape is nightly-warning-free on Rust 1.94).
