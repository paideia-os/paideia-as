# Slice D — RecordCons emit arm + caller-side pair-unpack gate lift

Issue: paideia-as#1554 (PAS-DEBT-B4-002)
Version bump: 0.36.70 → 0.36.71
Slice sequence: A (v0.36.68) → B (v0.36.69) → C (v0.36.70) → **D (this)** → retirements (B4-002 #1524, B4-003 #1525 as follow-ups)

## Scope closed

**Piece 1 — RecordCons emit arm**: `emit_visit_lambda.rs` gained an
`IrKind::RecordCons` body arm; `emit_walker/emit_core.rs::emit_callee_sret_splice`
gained an inline field-store loop (via
`emit_record_cons_field_stores_into_sret_buffer`) that populates the
source buffer between the `sub rsp, padded_size` allocation and the
aggregate-return helper's copy/load sequence. `emit_walker/walk.rs`'s
pre-pass gate grew a `Lambda → RecordCons` mark (paralleling the
existing `Lambda → EnumCons` mark) so the id-preorder flat pass at
line 624 doesn't re-fire the cap-mint `visit_record_cons` (which
would T0518 on non-4-u64 shapes).

**Piece 2 — pair-unpack gate lifted**: the Memory-only gate in
`return_record_cons_pass::populate_return_record_cons_slots` (`want_slot
= matches!(info.shape, PlacementShapeInner::Memory)`) is now
`!matches!(info.shape, PlacementShapeInner::Absent)`. Register-return
placements allocate a persistent `[RBP + disp]` slot; the Slice C
Piece-4 splice in `emit_call.rs` writes RAX/RDX/XMM0/XMM1 into it
after CALL.

**Piece 3 — Symbol return_record_layout for record-literal returns**:
already covered by Slice A's `populate_return_record_layouts` pass —
it walks `TypeFnPtr { ret: TypeRecord | TypeName-in-StructRegistry }`
and stamps `Symbol::return_record_layout`. No new pass needed for the
Slice D fixture shape.

**Piece 4 — end-to-end fixtures + tests**: two `.pdx` fixtures under
`tests/data/sret_slice_d/`; four unit tests in
`emit_walker_tests/sret_slice_d.rs`.

**Piece 5 — B4-002 / B4-003 retirements**: deferred. Rationale in the
Deferrals section below.

## Design decisions

### Where the field stores live

Two options were on the table:

**Option A** — Add a body-shape arm in `emit_visit_lambda` that
allocates its OWN buffer (`sub rsp, N`), populates it with field
stores, then hands the buffer disp off to `emit_ret` via a side-table.
`emit_callee_sret_splice` reads that disp and skips its own `sub rsp,
N` allocation.

**Option B (adopted)** — Keep the buffer allocation in
`emit_callee_sret_splice` (as Slice C had it). Add the field-store
loop INLINE in the splice, right after the `sub`. The RecordCons body
arm in `emit_visit_lambda` becomes minimal — it just calls `emit_ret`.

**Rationale for B**:
- The entire sret story (buffer allocation + population + copy) stays
  in one function (`emit_callee_sret_splice`). No side-table plumbing;
  no risk of a body arm allocating one buffer and the splice
  allocating another and them getting out of sync.
- Body arm remains small and stateless — mirrors the existing EnumCons
  arm's shape (arm-and-forget).
- The splice already has the `layout` in hand (from `Symbol::return_record_layout`)
  and can navigate to the RecordCons body via `arena.children(current_function)[0]`.
  No new information needs to flow through a side-table.

### Slot allocation for `@no_frame` callers

The pass in `return_record_cons_pass.rs` doesn't consult `let_meta.no_frame`
per caller Lambda — that would require a reverse map from Lambda IrNodeId
to the containing Let. Rather than add that lookup, the pass allocates a
slot for every record-returning App node regardless of frame status, and
the emit-side guards discard the slot when the caller is `@no_frame`:

- `emit_call.rs::emit_call_args_and_call` computes `is_caller_no_frame`
  and clears `persistent_slot` when true → caller falls back to Slice
  B behaviour (transient sret for Memory; scalar-shape for register-
  return).
- `emit_visit_lambda.rs`'s sret prologue-bump block is wrapped in
  `if !is_no_frame` → the `sub rsp, N` never fires for a frameless
  caller.

This wastes a `caller_sret_slot_table` entry for @no_frame callers,
but the entry is 8 bytes and only allocated when the callee has a
`return_record_layout` — vanishingly rare in practice. The alternative
(threading no_frame status through the pass) requires either a
StructRegistry-style symbol → binding map or a caller_lambda-scoped
walk of `let_meta` — both larger than the savings.

### Field value shapes supported

The `emit_record_cons_field_stores_into_sret_buffer` helper handles:
- **Literal** → `mov [rsp + off], imm` (encoder narrows to `48 C7`
  imm32-sign-extended for typical values).
- **Var** → `arena.binding_names().get(child_id)` → name → `local_bindings.get(name)`
  → `RegId` → `mov [rsp + off], reg`.

Any other kind (nested App, arithmetic, EnumCons, FieldAccess) is
silently skipped — the sret store still fires, but that field's slot
reads back as raw stack. A follow-up `T0522 (unsupported RecordCons
field value shape)` diagnostic is deferred to when fixture pressure
demands richer value shapes (e.g. a body like `fn () -> Pair { a: 1
+ 2, b: other_fn() }`). The Slice D fixture shape
(`fn (x, y) -> Pair { a: x, b: y }` and `fn () -> Triple { a: 111,
b: 222, c: 333 }`) is fully covered by the Literal + Var arms.

## Deferrals (Piece 5)

Both B4-002 (`cpuid_leaf`, paideia-as#1524) and B4-003 (`mldsa65_sign`,
paideia-as#1525) require substantial recipe-framework revisions
orthogonal to Slice D's emit-side work:

### B4-002 — `cpuid_leaf` retirement

Current: two hand-rolled `SysVRegs` recipes
(`cpuid_leaf_ad` / `cpuid_leaf_bc`) return a u64 by packing two
CPUID registers via `shl 32; or`. Retirement replaces them with a
single `cpuid_leaf(leaf, subleaf) -> CpuidRegs { eax, ebx, ecx, edx }`
recipe.

Blockers:
1. `ArgConvention` has no "returns via sret / IntPair" variant. The
   current `SysVRegs` convention hard-wires "returns u64 in RAX". A
   new `ArgConvention::SysVReturnPair` (or a `ReturnConvention` cross-
   axis) needs to be introduced without regressing the ~30 existing
   SysVRegs recipes.
2. The recipe's splice contract with `emit_call.rs` differs: right
   now the splice inlines at the CALL site with a scalar-return
   assumption. With IntPair return, the splice needs to hand the
   RAX+RDX pair to the caller's persistent slot (which Slice D wires,
   but only via `Symbol::return_record_layout` — a recipe callee has
   no Symbol entry at all).
3. Callers of `CpuidOps::cpuid_leaf(0x1, 0)` need to be able to
   `.eax` project the result — the field-access lowering must resolve
   against the CpuidRegs struct type at typechecking time, not at
   recipe-splice time.

Estimate: 2-3 slices of work, coupled with recipe-framework revisions
that affect the MSR/TLB/barrier/refcount recipe families.

### B4-003 — `mldsa65_sign` retirement

Current: `mldsa65_sign(secret_key, msg, msg_len, out_sig)` takes a
3309-byte caller-allocated output buffer as its 4th parameter.
Retirement returns `MldsaSignature { bytes: [u8; 3309] }` and drops
the parameter.

Blockers:
1. Same `ReturnConvention` addition as B4-002. A 3309-byte return is
   always Memory-placed → sret via RDI.
2. The 4-parameter caller signature becomes 3-parameter after
   retirement. Downstream `paideia-os#2509` (aspace_map r11-dest) has
   the same "one caller-visible parameter is really a scratch-reg
   binding" pattern; both are best retired together to avoid two
   rounds of caller updates.
3. `MldsaSignature` is a huge struct (3309 bytes) — the caller-side
   sret slot allocation in `return_record_cons_pass` currently rounds
   up to a 16-byte multiple. 3309 → 3312 → 3312-byte stack slot per
   caller. Fine as long as the caller has a real frame and no
   recursion depth blows the redzone.

Estimate: 1 slice for the callee-side recipe conversion + caller-side
call-site updates; blocked on B4-002's `ReturnConvention` axis.

## Cross-repo status

`paideia-os` (the consumer) doesn't currently use record returns
through the paideia-as machinery — CPUID access is via hand-rolled
`unsafe` blocks that read RAX/RDX/RCX directly. The Slice D
machinery is exercised only by `paideia-as`'s own fixtures and unit
tests. paideia-os pickup arrives with B4-002 retirement.

## Test coverage summary

`emit_walker_tests/sret_slice_d.rs` (5 new tests):
1. `sysv_memory_record_cons_body_populates_source_buffer_from_literals` —
   asserts 3 literal stores at offsets 0/8/16 after `sub rsp, 32`.
2. `sysv_intpair_record_cons_body_populates_before_pair_load` —
   asserts 2 literal stores + 2 register loads.
3. `literal_body_leaves_sret_source_buffer_unpopulated` — pins that
   Slice D's Piece 1 is RecordCons-opt-in.
4. `sysv_intpair_caller_with_frame_emits_persistent_slot_and_pair_unpack` —
   caller with real frame gets `sub rsp, 16` prologue + `mov [rbp-16], rax`
   / `mov [rbp-8], rdx` after CALL.
5. `intpair_no_frame_caller_falls_back_to_slice_b_byte_identity` —
   `@no_frame` caller preserves 4-instruction scalar-shape emission.

`return_record_cons_pass::tests` (1 changed test):
- `intpair_callee_does_not_allocate_slot` → renamed to
  `intpair_callee_allocates_persistent_slot_in_slice_d` with inverted
  expectations (slot present at RBP-16 with padded_size 16).

Pinned unchanged (regression guards):
- `sret_call_wiring::sysv_memory_24b_emits_sret_sub_lea_arg_shift_add`
- `sret_call_wiring::sysv_intpair_16b_leaves_call_emission_byte_identical_to_scalar`
- `sret_call_wiring::scalar_return_regression_no_sret_wiring_when_layout_absent`
- `sret_slice_c::sysv_memory_callee_emit_ret_splices_sret_store`
- `sret_slice_c::sysv_intpair_callee_emit_ret_splices_pair_load`
- `sret_slice_c::ms_memory_callee_emit_ret_splices_sret_store_via_rcx`
- `sret_slice_c::scalar_return_callee_no_sret_splice`
- `sret_slice_c::caller_persistent_slot_emits_lea_rbp_relative_no_transient_sub_add`
- `sret_slice_c::slice_b_fallback_when_no_caller_slot`

## Issue #1554 closability

After Slice D lands (and the build is green), issue #1554 is
**fully closable** at the machinery level:
- Parser ✓ (Slice A)
- Symbol side-table ✓ (Slice A)
- Caller-side sret wiring ✓ (Slice B)
- Callee-side sret splice ✓ (Slice C)
- Persistent caller-side frame slot ✓ (Slice C)
- Caller-side pair-unpack ✓ (Slice D Piece 2)
- Callee-side RecordCons materialisation ✓ (Slice D Piece 1)

The two follow-up retirements (B4-002 #1524, B4-003 #1525) are the
consumer-side pickup: they exercise the machinery but their design
work (`ReturnConvention` recipe-framework axis) is a distinct scope.
