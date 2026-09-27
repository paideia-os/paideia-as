# CHANGELOG-1554 Slice C: record-cons pass + callee/caller sret finalisation

**Wave**: v0.36.70 (2026-09-27)
**Issue**: paideia-as#1554 (PAS-DEBT-B4-002)
**Predecessor**: v0.36.69 Slice B (caller-side sret prelude + arg-shift, transient slot)
**Successor**: Slice D (return-position record-cons body materialisation)

## Scope

Slice C closes the record-return machinery except for the source-
level record-cons body arm (deferred to Slice D). It ships:

  1. **Piece 2** — callee-side sret splice in `emit_ret`.
  2. **Piece 3** — persistent caller-side frame slot (replaces
     Slice B's transient `sub/lea/add` triplet with an RBP-relative
     LEA once the caller's prologue reserves the sret area).
  3. **Piece 4** — caller-side register-return pair-unpack, dormant
     in this wave (pass gates slot allocation on Memory-only per
     the Slice B IntPair regression contract).
  4. **Piece 1 scaffolding** — `return_record_cons_pass.rs` is the
     place where the future body-side record-cons materialisation
     hooks in; the pass populates two new arena side-tables today.

## What landed

### New IR side-tables (paideia-as-ir)

- `crates/paideia-as-ir/src/sret_frame_slots.rs` (new):
  - `CallerSretSlot { rbp_disp: i32, padded_size: u32 }`.
  - `CallerSretSlotTable` keyed by App IrNodeId.
  - `CallerSretFrameBumpTable` keyed by caller Lambda IrNodeId.
  - Full unit-test set (empty, insert-and-get, overwrite,
    bump-table).
- `crates/paideia-as-ir/src/arena.rs`: fields + read/write
  accessors mirroring the `return_record_layout_table` shape.
- `crates/paideia-as-ir/src/lib.rs`: module + re-export.
- `crates/paideia-as-ir/src/symbol.rs`: `SymbolTable::lookup_by_ir_node`
  (O(n) but n is small; documented rationale).

### New elaborator pass

- `crates/paideia-as-elaborator/src/return_record_cons_pass.rs`
  (new): walks every Lambda's descendant App nodes, allocates a
  caller-owned frame slot per record-returning-Memory call, sums
  the per-caller bump. Reads
  `return_record_layout_table` + `binding_names` + `let_meta`
  (all pre-walk), so it fires before `emit_walker::walk` consumes
  the tables.
  - 4 tests: single Memory call, IntPair no-slot regression,
    scalar-return regression, two-call packing.
- `crates/paideia-as-elaborator/src/lib.rs`: module + export.

### Emit-side wiring

- `crates/paideia-as-elaborator/src/emit_walker/emit_core.rs`:
  - `emit_callee_sret_splice` (new): looks up the current
    function's Symbol → `return_record_layout`, resolves SysV or
    MS placement, splices the appropriate aggregate-return helper
    (`sysv_callee_sret_store` / `sysv_callee_load_return_pair`,
    MS analogues) prefixed by a `sub rsp, padded_size` allocating
    the callee-local source buffer.
  - `emit_ret` docblock refresh: Slice C wiring documented.
- `crates/paideia-as-elaborator/src/emit_call.rs`:
  - `emit_function_call_with_app` (new entry point that threads
    an App IrNodeId hint through to `emit_call_args_and_call`).
  - `emit_function_call` / `emit_call_stmt` / `emit_call_expr`
    keep the old signature, forwarding `None` (Slice B fallback).
  - Persistent-slot branch in the caller-side sret block: when
    `app_id` names an entry in `CallerSretSlotTable`, emits a
    single `lea rdi, [rbp - disp]` (or `lea rcx, ...` for MS)
    instead of the transient `sub/lea` pair.
  - Post-CALL `add rsp, N` release skipped when the persistent
    slot fired.
  - Piece 4 splice after CALL for register-return callees whose
    slot is present (dormant this wave).
- `crates/paideia-as-elaborator/src/emit_visit_lambda.rs`:
  - Frame-bump `sub rsp, N` emitted right after the frame-pointer
    prologue when `CallerSretFrameBumpTable` has a value for the
    current Lambda.
  - Tail-call App arm uses `emit_function_call_with_app(Some(body_id))`
    so the caller-side sret path can resolve the persistent slot.

### Pipeline wiring

- `crates/paideia-as/src/cmd_build/walker_pipeline.rs`: invokes
  `populate_return_record_cons_slots(&mut lowering.ir)` right
  before `emit_walker.walk(&mut lowering.ir)`, after
  `call_sites` has been pre-populated and after the earlier
  `populate_return_record_layouts` pass filled the Slice A table.

## Piece 1 deferred to Slice D — rationale

The scope's Piece 1 asks for a return-position record-cons
materialisation pass that scans the AST/IR for return sites and
emits per-field stores into a callee-local slot. That requires an
`emit_visit_lambda` arm that recognises a Lambda body whose IR
kind is `IrKind::RecordCons` and drives the record-cons layout
through the same shape emit_store_record uses today (currently only
the module-level cap-mint shape). Adding that arm end-to-end in
this wave would balloon Slice C beyond a manageable review; the
Slice B fixtures (all `-> 0`-bodied) work as scaffolding proofs
that the ABI wiring is byte-shape-correct even without a real
source buffer.

Slice D task list (concrete):

  1. Add an `IrKind::RecordCons` arm in
     `emit_visit_lambda.rs`'s Lambda body dispatch that emits
     per-field stores into the callee-local source buffer (the
     same `[rsp + 0]`-anchored buffer `emit_callee_sret_splice`
     currently allocates).
  2. Move the source-buffer `sub rsp` from `emit_callee_sret_splice`
     to a per-Lambda prologue splice (mirroring the caller-side
     bump landed here) so the buffer is available for the whole
     body, not just the sret splice.
  3. Add a `ReturnRecordConsTable` side-table if the body needs
     a stable RBP-relative disp (e.g. for multi-return-site
     bodies with branches).
  4. Lift the pass gate in `return_record_cons_pass.rs` to allow
     register-return placements (currently `want_slot` is
     Memory-only) so Piece 4's post-CALL pair-unpack starts
     firing — Slice B's IntPair test's byte-identity assertion
     will need updating to reflect the pair-unpack presence.
  5. Enrich test coverage with real record-body fixtures (SysV
     IntPair-body, SysV Memory-body, MS Memory-body) driven
     through cmd_build and asserting the full emit-time byte
     sequence.

## Tests

Scaffolding-only per the constraint ("gate Slice C on scaffolding
tests only" when Piece 1 defers). The 4 tests in
`return_record_cons_pass.rs` pin:

  * `memory_callee_single_call_allocates_one_slot`
  * `intpair_callee_does_not_allocate_slot`
  * `scalar_returning_callee_produces_no_entries`
  * `two_memory_calls_pack_non_overlapping_slots`

Byte-exact end-to-end fixture tests for
`sret_16b_pair`/`sret_24b_memory`/`sret_ms_16b_memory` under
`tests/data/sret_slice_b/` are pinned by the parse+layout
smokes already landed in Slice B (their record bodies remain
`-> 0`; Slice D's real record-cons body will enable byte-exact
end-to-end assertions).

## B4-002 / B4-003 retirement status

Slice C completes the emit-side ABI wiring for record-returning
functions. Retirement of PAS-DEBT-B4-002 (paideia-as#1524 cpuid_leaf)
and B4-003 (paideia-as#1525 mldsa65_sign) requires ALSO:

  * Slice D's return-position record-cons body arm — without it,
    `cpuid_leaf`'s inline `record { eax, ebx, ecx, edx }` body
    won't lower correctly. The Slice C splice emits an sret store
    from an uninitialised buffer today.
  * MS x64 callee-side sret splice regression testing under a real
    fixture (`sret_ms_16b_memory.pdx`'s callee body currently
    stays `-> 0`).

So **B4-002 and B4-003 retirement is unblocked by Slice C's
scaffolding but not yet possible end-to-end** — the concrete
retirement PR lands with Slice D.

## No smoke run

Per the sub-agent contract: builds are main-only. Main should:

  * `bash tools/build.sh` on the updated tree
  * If clean: commit + push with `Bump tools/paideia-as →
    v0.36.70 (record-return Slice C: callee/caller sret + persistent
    frame slot)`.
  * If failing: re-invoke softarch with the error tail so the pass
    boundaries can be re-tuned.
