# PAS-DEBT-B4-002 (paideia-as#1524): cpuid_leaf record-return sret retirement

## Summary

Attempted retirement of the `cpuid_leaf` hand-rolled scalar-pair
workaround (`cpuid_leaf_ad` + `cpuid_leaf_bc` — two SysVRegs recipes
that pack two of CPUID's four output regs into u64s and force callers
that want all four to reissue CPUID twice) via the byte-exact
aggregate-return helpers landed in v0.36.62 (SysV, paideia-as#1544)
and v0.36.63 (MS x64, paideia-as#1543).

**Verdict: RETIREMENT BLOCKED.** The supply side is in place; the
demand side is missing the same six pieces of machinery flagged in
#1544's "End-to-end fixtures — deferred" section. No forced partial
retirement.

## Current shape (before this ticket)

- **Recipes.** `crates/paideia-as-elaborator/src/stdlib_lowering/cpuidops.rs`
  exposes two `SysVRegs` recipes, both scalar-return u64s:
  - `cpuid_leaf_ad(leaf: u32, subleaf: u32) -> u64` — returns
    `(EDX << 32) | EAX` packed into `RAX`.
  - `cpuid_leaf_bc(leaf: u32, subleaf: u32) -> u64` — returns
    `(ECX << 32) | EBX` packed into `RAX`.
- **Splice pattern.** Both recipes are inline splices (replace the
  CALL entirely). Each executes `push rbx; ... cpuid ... pop rbx;
  shl <hi>, 32; or rax, <hi>`. The pack-into-RAX shape is the
  workaround: SysV would actually return a 16-B aggregate in
  `RAX + RDX` (`IntPair`), but no lowering existed to consume that
  split return as a field-addressable record.
- **Caller cost.** A caller wanting all four registers calls both
  `_ad` and `_bc`, paying two CPUID executions.
- **Documented in.** `design/stdlib/cpuid-record-return.md` §2
  (scope note, paideia-as#1298); debt-catalog row
  `PAS-DEBT-B4-002` (`design/paideia-as-debt-catalog.md` §5.2).

## Target shape (blocked)

Per `design/stdlib/cpuid-record-return.md` §3 (recommended Option A —
caller-passed record pointer, sret shape):

```
; cpuid_leaf(leaf: u32, subleaf: u32) -> CpuidRegs
;   entry: RDI = caller-owned 16-B buffer ptr (record-layout aligned)
;          RSI = leaf, RDX = subleaf
    push    rbx
    mov     eax, esi
    mov     ecx, edx
    cpuid
    mov     dword ptr [rdi +  0], eax
    mov     dword ptr [rdi +  4], ebx
    mov     dword ptr [rdi +  8], ecx
    mov     dword ptr [rdi + 12], edx
    mov     rax, rdi
    pop     rbx
    ret
```

Note: the design doc's Option A shape is written for a
CpuidRegs-sized `Memory` sret (16-B under SysV would actually be
`IntPair` per the ABI classifier — see §2 of the design doc, which
explicitly acknowledges the tuple-in-registers rule but recommends
sret because the record subsystem has no register-pair-unpack path).
The 4×u32 store-through-RDI form still matches
`sysv_callee_sret_store(16, <src_base>, <src_disp>)` verbatim (three
`mov r10, [src+k]` / `mov [rdi+k], r10` qword pairs is a 16-B
special case — actually two pairs, since 16/8 = 2 qwords). The
helpers we would consume:

- **Callee side.** `aggregate_return::sysv_callee_sret_store(16,
  src_base, src_disp)` copies the 4-u32 buffer into `[RDI + k]` for
  `k in {0, 8}` and appends `mov rax, rdi`. For the register-return
  alternative (Option B): `aggregate_return::sysv_callee_load_return_pair(
  SysvReturnPlacement::IntPair, src_base, src_disp)` loads
  `[src+0]` into RAX and `[src+8]` into RDX.
- **Caller side.** `aggregate_return::sysv_caller_sret_prelude(
  dest_base, dest_disp)` emits `lea rdi, [dest+disp]` before the
  CALL. For Option B: `aggregate_return::sysv_caller_read_return_pair(
  SysvReturnPlacement::IntPair, dest_base, dest_disp)` after the
  CALL writes RAX + RDX back into the caller-owned buffer.
- **MS x64 mirrors.** `ms_caller_sret_prelude` (LEA into RCX),
  `ms_callee_sret_store` (copies 16 B into `[RCX+k]` for k in {0,8},
  then `mov rax, rcx`). MS classifies 16-B CpuidRegs as `Memory`
  (any aggregate > 8 B goes through sret in MS x64), so only the
  sret path applies there — no MS register-pair analogue.

## Demand-side blockers (six pieces of machinery)

These are the same blockers #1544's "End-to-end fixtures — deferred"
section flagged, now enumerated concretely for the cpuid_leaf case:

1. **CpuidRegs record type not declared.** `crates/paideia-as-stdlib/pdx/cpuid.pdx`
   currently declares only `trait CpuidOps { fn cpuid_leaf_ad(...);
   fn cpuid_leaf_bc(...); }`. Adding
   `record CpuidRegs { eax: u32, ebx: u32, ecx: u32, edx: u32 }` and
   `fn cpuid_leaf(leaf: u32, subleaf: u32) -> CpuidRegs !{sysreg}
   @{paideia.sysreg}` requires the parser to accept a record-typed
   return in `fn` signatures. Today `let f = fn (…) -> {u32; u32;
   u32; u32} …` is unparseable (per #1544 §"Not done"). Records ARE
   constructible via `RecordCons` and passable by pointer, but the
   return-position syntax is a gap.
2. **No return-record-layout side-table on `Symbol`.** The elaborator
   cannot look up whether a symbol's return type is an aggregate
   with a specific `RecordLayout`. The arg-side has this plumbing
   (see `paideia-as-elaborator/src/emit_call.rs`), but the
   return-side mirror is missing. Wire-up would add a
   `ret_record_layout: Option<RecordLayout>` field on `Symbol` and
   populate it during elaboration whenever the return type binds
   to a record.
3. **`emit_call.rs` has no aggregate-return branch.** Around a CALL
   whose callee returns an aggregate, the caller must:
   (a) look up the return-layout side-table;
   (b) call `sysv_return_placement_from_layout(&layout)` /
       `ms_return_placement_from_layout(&layout)` to derive placement;
   (c) if placement.needs_hidden_sret(), allocate a caller-owned
       stack slot of `layout.size` bytes at `layout.align` alignment,
       splice `aggregate_return::sysv_caller_sret_prelude(RBP, slot_disp)`
       or the MS mirror BEFORE the CALL, and shift the SysV/MS
       argument marshalling right by one (so caller args
       leaf/subleaf go to RSI/RDX/... instead of RDI/RSI/... in
       SysV, or RDX/R8/... instead of RCX/RDX/... in MS);
   (d) if placement is a register-return shape, splice
       `sysv_caller_read_return_pair(placement, ...)` /
       `ms_caller_read_return_reg(placement, ...)` AFTER the CALL to
       write RAX/RDX/XMM0/XMM1 back into a caller-owned buffer.
   None of this exists today. It is a substantial refactor of the
   SysV/MS arg-marshalling path in `emit_call.rs`.
4. **`emit_ret` has no aggregate-return branch.** In
   `emit_walker/emit_core.rs::emit_ret`, the callee must symmetrically:
   read the return-layout side-table for its own function symbol; if
   placement.needs_hidden_sret(), splice
   `sysv_callee_sret_store(layout.size, src_base, src_disp)` /
   `ms_callee_sret_store(...)` before the frame-pointer epilogue and
   RET; if placement is register-shaped, splice
   `sysv_callee_load_return_pair(placement, ...)` /
   `ms_callee_load_return_reg(placement, ...)` before the epilogue.
   Requires the callee to know where its return-value composite lives
   in its own frame (i.e., an integration point with the record-cons
   codepath that materialises the record on the callee stack in the
   first place).
5. **`ArgConvention` has no sret variant.** The current
   `ArgConvention::SysVRegs` (in `paideia-as-elaborator/src/stdlib_lowering/mod.rs`)
   hard-wires arg[0] to RDI. For an sret-shaped recipe, arg[0] must
   be the sret hidden pointer (RDI in SysV, RCX in MS) and real args
   shift right by one. Two paths:
   (a) Add `ArgConvention::SysVSret { real_arg_count: usize }` /
       `MsSret { real_arg_count: usize }` variants and teach
       `emit_call_args_and_call` to marshal real args starting from
       the arg-register list at index 1 rather than index 0.
   (b) Or push the sret detection out of `ArgConvention` entirely
       and into the emit_call site, making it a return-type-driven
       property rather than a recipe-declared property (this is the
       cleaner shape once the return-layout side-table exists —
       see blocker (2)).
   Path (b) subsumes (a) and is the recommended follow-up shape.
6. **Record subsystem cannot unpack a register-pair return** (blocks
   Option B specifically). SysV's `IntPair` placement returns two
   eightbytes in RAX + RDX; the record subsystem has no lowering
   that materialises `{eax, ebx, ecx, edx}` back from a
   `(RAX_low32, RAX_high32, RDX_low32, RDX_high32)` view. This is
   the "record-subsystem enhancement, not a boot-intrinsic
   enhancement" flagged in `design/stdlib/cpuid-record-return.md`
   §2 "Option B — deferred". Landing it would let a broad class of
   ≤16-B records return in registers uniformly. If (5) is resolved
   via path (b), (6) still blocks Option B specifically — Option A
   (sret via caller-owned buffer) sidesteps (6) because the record
   is materialised in memory on both sides.

Even if we tried to special-case cpuid_leaf as an inline `SysVRegs`
recipe (mirroring the ad/bc recipes' inline-splice shape), we would
hit blocker (5) directly: the recipe body needs RDI to hold the
sret buffer pointer, but SysVRegs puts arg[0] (leaf) in RDI.
Retirement is genuinely blocked, not merely inconvenient.

## Constraints observed

- **No aggregate_return helper modifications.** The helpers landed in
  #1543/#1544 are unchanged. This ticket only cites them.
- **No new intrinsic ABI.** `ArgConvention` still has only `Literal`
  and `SysVRegs` — no new variants. The sret-shaped variant is left
  to the follow-up ticket (blocker (5) path (a) or (b)).
- **Encoder pitfalls not applicable.** This ticket writes zero new
  instructions (documentation-only change). No `test rN,rN`, no
  `and r11, imm64`, no 2-op `imul r,imm`, no reserved-word labels
  affected.
- **SysV-first stance honoured.** The Option A / Option B analysis
  is SysV-first; MS x64 blockers mirror the SysV list with the
  RDI→RCX substitution.
- **Novel + clean design bias.** No POSIX / Unix inheritance
  touched. The sret shape is the paideia-as ABI (`design/toolchain/
  abi.md` §2.2) directly.

## Files changed

- `Cargo.toml` (line 85): `workspace.package.version` 0.36.63 → 0.36.64.
- `CHANGELOG.md` (top): "Wave 35: B4-002 cpuid_leaf retirement — BLOCKED
  on call-site wiring" entry with the six-blocker summary and follow-up
  ticket sketch.
- `crates/paideia-as-elaborator/src/stdlib_lowering/cpuidops.rs`
  (module docblock, ~30 new lines): "PAS-DEBT-B4-002 retirement status
  (v0.36.64, paideia-as#1524)" section enumerating the four landed
  helpers on each ABI's supply side, the six demand-side blockers,
  and the follow-up ticket sketch.
- `design/paideia-as-debt-catalog.md` §5.2 (`PAS-DEBT-B4-002` row):
  symptom column replaced — cites landed helpers explicitly, lists
  the six blockers inline, cross-references the follow-up ticket
  sketch by name and by scratch-changelog path.

No source-level code changes. No `.pdx` fixture reactivations
(the scalar-pair `cpuid_leaf_ad` / `cpuid_leaf_bc` recipes are the
current useful surface and remain untouched).

## Follow-up ticket sketch (ready for `gh issue create`)

**Title:** PAS-DEBT-B3-007c-followup: record-return type plumbing +
call-site wiring for SysV/MS aggregate returns

**Body:**

```
Consumes the byte-exact aggregate-return helpers landed in
v0.36.62 (SysV, #1544) and v0.36.63 (MS x64, #1543) at concrete
call sites. Retires debt items:

- PAS-DEBT-B4-002 (#1524) — cpuid_leaf full record return
  (design/stdlib/cpuid-record-return.md).
- PAS-DEBT-B4-003 — mldsa65_sign 3309-byte caller-buffer
  workaround (design/paideia-as-debt-catalog.md §5.2).

Scope (six pieces of machinery, one per blocker):

1. Parser + AST: accept record types at fn return position (`fn
   (…) -> { u32; u32; u32; u32 }` and named-record form `fn (…) ->
   CpuidRegs`). Land inside paideia-as-parser + paideia-as-ast.

2. Symbol side-table: add `ret_record_layout: Option<RecordLayout>`
   to Symbol (or the equivalent post-elaborator return-type descriptor).
   Populate during elaboration whenever the return type binds to a
   record type.

3. emit_call.rs: around every CALL whose callee returns an
   aggregate, look up the side-table, call
   sysv_return_placement_from_layout / ms_return_placement_from_layout,
   allocate the caller-owned buffer slot on the frame, splice
   the sret prelude helper before CALL (shifting SysV/MS arg regs
   right by one on Memory placement), and splice the caller-side
   read-back helper after CALL on register-return shapes.

4. emit_walker/emit_core.rs::emit_ret: mirror on the callee side —
   look up the same side-table, splice sysv_callee_sret_store /
   ms_callee_sret_store or the pair-load variants before the
   frame-pointer epilogue + RET.

5. ArgConvention: either add SysVSret / MsSret variants (path a),
   or refactor sret detection out of ArgConvention into an
   emit_call-site check driven by the return-layout side-table
   from (2) (path b, recommended — subsumes path a).

6. Record subsystem: add a lowering path that consumes a
   SysV IntPair register return (RAX + RDX with 4×u32 field-slice
   arithmetic) and materialises a field-addressable record temporary.
   Optional for cpuid_leaf if Option A (sret) is chosen — required
   for uniform ≤16-B aggregate register returns.

Deliverables per site being retired (post-machinery landing):

- CpuidRegs record type + `cpuid_leaf(leaf, subleaf) -> CpuidRegs`
  declaration in crates/paideia-as-stdlib/pdx/cpuid.pdx.
- New cpuid_leaf recipe in cpuidops.rs consuming
  aggregate_return::{sysv_callee_sret_store, ms_callee_sret_store}
  and the caller-side helpers via emit_call.
- Removal of PAS-DEBT-B4-002 row from paideia-as-debt-catalog.md.
- Retention of cpuid_leaf_ad / cpuid_leaf_bc (backward-compat surface
  per design/stdlib/cpuid-record-return.md §1: "Consumers written
  against the AD/BC pair keep working after that migration").

- mldsa65_sign sret conversion (parallel retirement, PAS-DEBT-B4-003)
  under the same machinery.

- .pdx end-to-end fixture per #1544 §"End-to-end fixtures — deferred":
  `let r: CpuidRegs = cpuid_leaf(0x1A, 0); assert r.eax != 0` on
  QEMU -cpu host, witnessing P-core class on Raptor Lake and
  non-zero XSAVE size.

Dependencies:
- Depends on: #1543 (MS x64 helpers landed v0.36.63),
              #1544 (SysV helpers landed v0.36.62).
- Blocks: #1524 (this ticket, B4-002),
          [B4-003 issue TBD] (mldsa65_sign sret).

Estimated size: L (parser + AST work is the tall pole; blockers
2–5 are contained inside the elaborator and emit_walker; blocker 6
is optional under Option A but recommended for uniformity).
```

## Not done (intentional)

- **No source-level code changes to cpuidops.rs recipes.** The
  ad/bc recipes remain the current useful surface. Removing them
  now would break downstream stdlib/pdx/cpuid.pdx-documented
  decoders (cpuid_hybrid_class_from_1a, cpuid_topology_shift_from_0b,
  etc.), all of which call into the ad/bc pair.
- **No CpuidRegs record type declaration.** Would require the
  parser + AST work (blocker 1) landing first.
- **No emit_call / emit_ret wiring.** Deferred to the follow-up
  ticket above.
- **No cargo check/build/test.** Per `feedback_no_background_builds.md`,
  builds are main-only.

## Risk / follow-ups

- The six-blocker enumeration in the debt-catalog row is long. If
  a future softarch attempts B4-002 retirement without reading the
  updated cpuidops.rs docblock, they will re-hit the same
  investigation. The docblock is the source of truth; the
  debt-catalog row cross-references it.
- The mldsa65_sign case (B4-003) has the same six blockers and
  should be batched into the follow-up ticket rather than filed
  separately.
- If the record-subsystem enhancement (blocker 6) is deprioritised,
  Option A (sret) still lands cpuid_leaf cleanly — but then all
  ≤16-B aggregate returns pay the sret buffer-allocation cost even
  when they could have gone through RAX + RDX. This is an
  acceptable interim shape; the register-pair path can land later
  as a pure optimisation.

Build not run — main should invoke `bash tools/build.sh` and re-invoke
me with the error tail if it fails.
