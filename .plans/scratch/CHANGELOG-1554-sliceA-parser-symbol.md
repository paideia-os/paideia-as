# PAS-DEBT-B4-002 Slice A (paideia-as#1554): record-return type plumbing

## Summary

Wires demand-side pieces (1) parser support for record types at fn
return position and (2) `Symbol::return_record_layout` side-table
field. Slice B (pieces 3–5: `emit_call.rs` aggregate-return branch,
`emit_ret` sret store, `ArgConvention::SysVSret`/`MsSret` variants)
and Slice C (piece 6: record subsystem unpack for register-return
placements) are separate waves.

Nothing about the historical scalar-return codepath moves. Every
call site of the existing `Symbol::new` / `new_with_visibility` /
`new_with_abi` continues to compile untouched — the new field
defaults to `None`, and Slice B/C code gates on
`Symbol::return_record_layout == Some(_)`.

## Verdict

Landed. Both plumbing pieces in place; the parser test corpus locks
in the shape; the elaborator populates the side-table; the emit
walker stamps the Symbol.

## Files changed

### paideia-as-ir crate

- `crates/paideia-as-ir/src/symbol.rs`:
  - Added `pub return_record_layout: Option<RecordLayout>` field on
    `Symbol`.
  - Extended `Symbol::new`, `Symbol::new_with_visibility`,
    `Symbol::new_with_abi` to default it to `None`.
  - New builder: `Symbol::with_return_record_layout(mut self, Option<RecordLayout>) -> Self`.
  - Switched `#[derive]` from `PartialEq/Eq/Hash` to hand-written
    impls that **exclude** the new field, so
    `SymbolTable::insert` still replaces in place after a populate
    pass. Documented reasoning inline.
  - 3 new tests: `return_record_layout_defaults_to_none`,
    `with_return_record_layout_attaches_and_clears`,
    `return_record_layout_not_part_of_identity`.
- `crates/paideia-as-ir/src/return_record_layout.rs` (new):
  - `ReturnRecordLayoutTable` via `impl_named_side_table!`, keyed by
    the item-level Let's `IrNodeId`.
  - Module docblock explains the "keyed by Let, not by Lambda"
    convention and the "absence ≠ scalar-return signal" contract Slice
    B/C consumers must honour.
  - 4 tests covering empty / insert-and-get / missing-key / overwrite.
- `crates/paideia-as-ir/src/arena.rs`:
  - Added `return_record_layout_table: ReturnRecordLayoutTable`
    field, initialised in both `Default` (via macro-derived
    `ReturnRecordLayoutTable::default()`) and `with_capacity` paths.
  - Two accessors: `return_record_layout_table()` /
    `return_record_layout_table_mut()`.
- `crates/paideia-as-ir/src/lib.rs`:
  - `pub mod return_record_layout;`
  - `pub use return_record_layout::ReturnRecordLayoutTable;`

### paideia-as-elaborator crate

- `crates/paideia-as-elaborator/src/return_record_layout_pass.rs` (new):
  - `populate_return_record_layouts(ast, ir, ast_to_ir, source_map, registry)`.
  - Walks item-level `ItemData::Let` nodes; gates on
    `value.kind == ExprLambda` and
    `ast.type_data(ty) == TypeData::FnPtr { ret, .. }`; peels `ret`
    into either an inline `TypeData::Record` (fields decoded via
    `struct_registry::decode_field_type`) or a `TypeData::Name` looked
    up in the `StructRegistry`.
  - Layout math mirrors `emit_pass_state::finalise_record_layouts`
    byte-identically: `field_align == field_size`, struct alignment is
    the max of field alignments, struct is tail-padded to that
    alignment. FnPtr fields inside an inline record are 8 B unsigned
    (mirrors `build_struct_registry`).
  - Silent-drop policy documented inline: unsupported field types,
    unresolved names, or a non-Lambda RHS with a fn-typed annotation
    leave the side-table entry absent; Symbol stays at scalar-return
    default. `T0552` still fires elsewhere for named-struct field
    types.
  - 5 unit tests for layout helpers + 2 integration tests that
    hand-build a minimal AST + IR + registry and prove the pass
    populates the side-table on `pub let cpuid : (u32, u32) ->
    CpuidRegs = fn …` and skips non-Lambda RHS bindings.
- `crates/paideia-as-elaborator/src/emit_walker/walk.rs`:
  - In the `IrKind::Let` arm inside `walk_inner`, after the ABI-aware
    `Symbol::new_with_abi(...)` and the pub-visibility override, read
    `arena.return_record_layout_table().get(node_id).cloned()` and
    stamp it onto `sym.return_record_layout` before
    `arena.symbols_mut().insert(sym)`. Keyed by `node_id` (the outer
    Let's IR id), not `symbol_ir_node` (the Lambda's id).
- `crates/paideia-as-elaborator/src/lib.rs`:
  - `pub mod return_record_layout_pass;`
  - `pub use return_record_layout_pass::populate_return_record_layouts;`

### paideia-as-parser crate

- `crates/paideia-as-parser/src/parse_type/tests.rs`: 4 new lock-in
  tests appended after the fn-ptr error tests:
  - `parse_fn_ptr_return_inline_record_cpuid_shape` — the 4×u32
    CpuidRegs shape that drove B4-002.
  - `parse_fn_ptr_return_named_struct` — `(u32) -> CpuidRegs`; parse
    yields a `TypeName` on the ret slot, elaborator resolves at
    `populate_return_record_layouts` time.
  - `parse_fn_ptr_return_empty_record` — `() -> record {}`; locks in
    the empty-record return shape.
  - `parse_fn_ptr_return_record_with_effects` — record return composed
    with an effect row (`!{sysreg}`) — proves the effect-row trailer
    survives the record ret.

  These are pure lock-ins: `parse_type_paren` was already routing
  through `parse_type` at the arrow's right side, and `parse_type` was
  already dispatching `KwRecord => parse_type_record` via
  `parse_type_unquantified`. The record-in-return-position path
  worked before this wave; the tests defend it against silent regression.

### paideia-as (build driver)

- `crates/paideia-as/src/cmd_build/mod.rs`: invokes
  `paideia_as_elaborator::populate_return_record_layouts` between
  `populate_lambda_param_enum_types` and the walker pipeline. Ordering
  chosen so the side-table is populated before the walker's Symbol
  construction reads from it.

### Workspace

- `Cargo.toml` (line 86): `workspace.package.version` 0.36.67 → 0.36.68.
- `CHANGELOG.md`: prepended a v0.36.68 entry summarising the six
  pieces of demand-side machinery, this slice's split (pieces 1 & 2
  landed; pieces 3–6 deferred to Slice B / C), and the file
  inventory.

## Parser production shape

Both shapes accepted:

1. **Anonymous inline record** — `(u32, u32) -> record { eax: u32, ebx: u32, ecx: u32, edx: u32 }`.
   Parse tree: `TypeFnPtr { params: [u32, u32], ret: TypeRecord {
   fields: [(eax, u32), (ebx, u32), (ecx, u32), (edx, u32)] } }`.
2. **Named struct in return position** — `(u32) -> CpuidRegs`.
   Parse tree: `TypeFnPtr { params: [u32], ret: TypeName { name:
   Ident("CpuidRegs"), args: [] } }`. The elaborator resolves the
   name against the `StructRegistry` when computing the layout.

Both compose with the existing FnPtr suffix grammar: effect rows
(`!{...}`) and capability sets (`@{...}`) attach to the FnPtr as
before, unaffected by the ret being a record.

## Symbol field placement + constructors updated

Field: `Symbol::return_record_layout: Option<RecordLayout>`.

Constructors touched (all default `None`):
- `Symbol::new(name, kind, ir_node)` — the auto-global convenience.
- `Symbol::new_with_visibility(name, kind, ir_node, visibility)`.
- `Symbol::new_with_abi(name, kind, ir_node, abi)`.

Builder added: `Symbol::with_return_record_layout(Option<RecordLayout>)`.

Identity: hand-written `PartialEq / Eq / Hash` intentionally exclude
the layout. Rationale documented inline: an insert-then-populate flow
must still collide on `SymbolTable::insert`'s by-name index instead
of producing a shadow entry.

## Test count

New tests:
- Parser: 4 (`parse_fn_ptr_return_{inline_record_cpuid_shape,
  named_struct, empty_record, record_with_effects}`).
- Symbol field: 3.
- Side-table: 4.
- Elaborator pass: 5 unit + 2 integration = 7.
- **Total new tests: 18.**

## Version bump confirmation

`Cargo.toml` line 86: `version = "0.36.67"` → `version = "0.36.68"`.
`CHANGELOG.md`: v0.36.68 entry prepended, dated 2026-09-27.

## What's needed for Slice B

Slice B lands `emit_call.rs` + `emit_ret` + `ArgConvention` variants
(pieces 3, 4, 5 of the six-blocker enumeration):

- **`emit_call.rs`** (piece 3): around every CALL whose callee's
  `Symbol::return_record_layout` is `Some(layout)`, call
  `sysv_return_placement_from_layout(&layout)` /
  `ms_return_placement_from_layout(&layout)`. When placement is
  `Memory` (needs hidden sret), allocate a caller-owned stack slot of
  `layout.size` bytes at `layout.align` alignment, splice
  `aggregate_return::sysv_caller_sret_prelude(RBP, slot_disp)` /
  `ms_caller_sret_prelude(...)` BEFORE the CALL, and shift the
  SysV/MS argument marshalling right by one (leaf → RSI/RDX in SysV
  instead of RDI/RSI). On register-return placement, splice
  `sysv_caller_read_return_pair(...)` /
  `ms_caller_read_return_reg(...)` AFTER the CALL to write RAX/RDX /
  XMM0/XMM1 back into the caller-owned buffer.
- **`emit_ret`** (piece 4): in
  `emit_walker/emit_core.rs::emit_ret`, look up the current function
  symbol's `return_record_layout`; on `Some(layout)`, splice
  `sysv_callee_sret_store(layout.size, src_base, src_disp)` /
  `ms_callee_sret_store(...)` before the frame-pointer epilogue and
  RET (for Memory placement), or the pair-load variants for
  register-return placements. Requires the callee to know where its
  return-value composite lives in its own frame — an integration
  point with the record-cons codepath that materialises the record
  temporary in the first place.
- **`ArgConvention`** (piece 5): the debt-catalog recommends path (b)
  — refactor sret detection out of `ArgConvention` entirely and into
  the emit_call site, driven by `Symbol::return_record_layout`.
  Path (a) — adding `SysVSret { real_arg_count: usize }` / `MsSret {
  real_arg_count: usize }` variants and teaching
  `emit_call_args_and_call` to marshal real args starting from
  arg-register index 1 — is the smaller step; path (b) subsumes it
  once the return-layout side-table (this slice) exists.

## What's needed for Slice C

Slice C lands piece 6: the record subsystem must unpack a SysV
`IntPair` register return (RAX + RDX with 4×u32 field-slice
arithmetic) into a field-addressable record temporary. Optional for
cpuid_leaf if Option A (sret) is chosen — required for uniform ≤16-B
aggregate register returns.

## Constraints observed

- **No source-level code changes to existing callers of the Symbol
  constructors.** All three constructors keep their signatures; the
  new field defaults to `None`.
- **No changes to the historical scalar-return codepath.** Slice B/C
  gates on `Symbol::return_record_layout == Some(_)`; absent field ⇒
  scalar-return codepath preserved verbatim.
- **No modifications to aggregate_return helpers.** The SysV
  (v0.36.62) and MS x64 (v0.36.63) supply helpers are unchanged;
  this slice only threads the demand-side descriptor.
- **No encoder pitfalls applicable.** This slice writes zero new
  instructions (parser + AST + IR side-table + elaborator pass +
  walker Symbol-stamp only). No `test rN,rN`, no `and r11, imm64`,
  no 2-op `imul r,imm`, no reserved-word labels affected.
- **SysV-first stance honoured.** The pass extracts a raw
  `RecordLayout`; ABI-specific placement classification happens at
  the emit_call / emit_ret call site in Slice B, where the SysV
  classifier runs first and the MS x64 classifier mirrors it.
- **Novel + clean design bias.** No POSIX / Unix inheritance touched.
  The sret shape is the paideia-as ABI (`design/toolchain/abi.md`
  §2.2) directly.

## Not done (intentional)

- **No `emit_call.rs` aggregate-return branch.** Deferred to Slice B.
- **No `emit_ret` sret store / pair-load branch.** Deferred to Slice B.
- **No `ArgConvention::SysVSret` / `MsSret` variants.** Deferred to
  Slice B; recommended shape is path (b) — refactor out of
  ArgConvention rather than add variants.
- **No record subsystem pair-unpack lowering.** Deferred to Slice C.
- **No .pdx end-to-end fixture.** Blocked until Slice B lands the
  demand-side splice; the field alone doesn't change any emitted
  bytes.
- **No `cargo check`, `cargo build`, or `cargo test`.** Per
  `feedback_no_background_builds.md`, builds are main-only.

## Risk / follow-ups

- The elaborator's `populate_return_record_layouts` pass is
  silent-drop by design (see module docblock). If a caller declares
  `pub let cpuid : (…) -> CpuidRegs = fn …` but `CpuidRegs` is not in
  the `StructRegistry` (e.g. the struct declaration was elided or
  the type name was mistyped), the Symbol keeps `return_record_layout
  = None` and Slice B's emit_call code will fall back to the
  scalar-return path — the caller then gets a silently-wrong return
  layout at the CALL site. Slice B should key the diagnostic
  ("return type is a record but no layout available") on the
  annotation-shape mismatch rather than expect the pass to fire the
  diagnostic, since `T0552` (unsupported field type) is already the
  right diagnostic for the struct-registry side.
- The pass keys on the outer Let's IR node id, not the Lambda's, so
  a future refactor that changes `symbol_ir_node = rhs_id` in
  `walk.rs` (Lambda's id for functions) must NOT also change the
  side-table key. The walker keeps the two ids distinct: `node_id`
  keys the side-table lookup, `symbol_ir_node` is stashed on the
  Symbol itself. Documented inline.

Build not run — main should invoke `bash tools/build.sh` and re-invoke
me with the error tail if it fails.
