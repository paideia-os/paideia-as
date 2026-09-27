# PAS-DEBT-B4-002 (paideia-as#1524) — Wave 46 re-attempt

## Summary

Re-attempted retirement of the `cpuid_leaf_ad`/`cpuid_leaf_bc`
scalar-pair workaround now that #1554 Slices A-D (v0.36.68..v0.36.71)
have landed the reference record-return machinery.

**Verdict: RETIREMENT STILL DEFERRED.** Blockers 1-5 from the Wave 35
enumeration (`.plans/scratch/CHANGELOG-1524-b4002-cpuid-sret.md`) are
now resolved. Blocker 6 is partially resolved. Three NEW mismatches
between the Slice A-D reference machinery and the specific
composition shape `cpuid_leaf` needs remain — none is a "small enough
for this wave" extension.

Doc-only landing: `cpuidops.rs` docblock rewritten against v0.36.75
reality; version bumped 0.36.74 → 0.36.75; CHANGELOG entry added;
recipe pair unchanged.

## What #1554 Slices A-D actually landed

Slice A (v0.36.68) — parser accepts `record { … }` return position;
`return_record_layout_pass::populate_return_record_layouts` fills
`IrArena::return_record_layout_table` from item-level `pub let f :
(…) -> Record = fn (…) → …` bindings, stamped onto
`Symbol::return_record_layout` at walker Let-Lambda symbol
construction.

Slice B (v0.36.69) — `emit_call.rs` probes the callee Symbol's
`return_record_layout`; on `Memory` placement, splices the caller-
side sret prelude (`lea rdi, [rsp+disp]` or MS `lea rcx, …`),
shifts the SysV/MS arg-register selection right by one, and releases
the slot with a matching `add rsp, padded` after the CALL. Register-
return placements leave CALL emission byte-identical to the scalar
path.

Slice C (v0.36.70) — `return_record_cons_pass::populate_return_record_cons_slots`
allocates a caller-side persistent frame slot for each record-
returning App, sized off the callee layout. Per-caller totals land in
`caller_sret_frame_bump_table`, which the prologue emitter reserves
with a single `sub rsp, total`. `emit_walker/emit_core.rs::emit_ret`
grew `emit_callee_sret_splice`, which for `Memory` placements emits
`sub rsp, padded` + `sysv_callee_sret_store` (byte-copy from source
buffer into `[rdi+*]`), and for register-return placements emits
`sysv_callee_load_return_pair` (pack from source buffer into
RAX+RDX).

Slice D (v0.36.71) — `emit_visit_lambda.rs` grew a
`IrKind::RecordCons` body arm that calls `emit_ret`;
`emit_record_cons_field_stores_into_sret_buffer` populates the
source buffer inline by walking the RecordCons children in step with
the callee's `return_record_layout.fields`, emitting
`mov [rsp+offset], imm` (Literal) or `mov [rsp+offset], reg` (Var
via `local_bindings` — parameters). Also lifted the Slice C gate so
register-return placements now allocate a caller persistent slot for
RAX/RDX spill after CALL.

## What blockers are resolved

Referencing `.plans/scratch/CHANGELOG-1524-b4002-cpuid-sret.md`'s
six-blocker enumeration:

- **Blocker 1** (CpuidRegs record type not declared, unparseable at
  fn return position) — cleared. Named-struct + inline-record return
  types are both parsed by Slice A.
- **Blocker 2** (`Symbol::return_record_layout` side-table missing)
  — cleared. Slice A populates it; Slice B/C/D read it.
- **Blocker 3** (`emit_call.rs` has no aggregate-return branch)
  — cleared. Slice B wires the caller-side sret prelude / arg shift
  / slot release.
- **Blocker 4** (`emit_ret` has no aggregate-return branch)
  — cleared. Slice C wires `emit_callee_sret_splice` for both
  Memory and register-return placements.
- **Blocker 5** (`ArgConvention` has no sret variant) — cleared via
  the design decision to subsume sret detection into an emit-call-
  site probe of `Symbol::return_record_layout` rather than add a
  new ArgConvention variant (see `emit_call.rs` module docblock,
  "path (b)"). No ArgConvention change was needed for user-code
  Lambda callees.
- **Blocker 6** (record subsystem cannot unpack register-pair
  return) — partially cleared for register-return placements. The
  caller-side persistent frame slot in Slice C + Slice D's post-
  CALL RAX/RDX spill provides a memory materialisation from which
  field-addressed loads read individual eightbytes. Remaining
  limitation: the unpack path is coupled to caller-side wiring,
  not a general record-subsystem lowering — recipe-driven callees
  do not participate.

## What NEW gaps block cpuid_leaf specifically

**Gap A — Slice D's RecordCons body arm supports only `Literal`
and `Var` (parameter-forwarding) field values.**

See `emit_walker/emit_core.rs::emit_record_cons_field_stores_into_sret_buffer`
(v0.36.71): the match arm handles `IrKind::Literal` (mov [rsp+off],
imm) and `IrKind::Var` (mov [rsp+off], reg from `local_bindings`
parameter binding); any other kind is silently skipped, leaving the
slot uninitialised until a future T0522 diagnostic fires. Explicit
docblock note:

> for the Slice D fixture surface (Cpuid { eax: 1, ebx: 2, ecx: 3,
> edx: 4 }-style literal-populated records, plus the
> parameter-forwarding `fn (x, y) -> Pair { a: x, b: y }`) the
> literal + var arms cover everything.

A `.pdx` wrapper that composed `cpuid_leaf_ad`/`bc` calls into a
record would have field values of the shape
`(cpuid_leaf_ad(leaf, sub) & 0xFFFFFFFF) as u32` — an `App` subtree
plus arithmetic (`And` on `Literal(0xFFFFFFFF)` plus a bit-cast).
The App / arithmetic children silently miss the store, so each
CpuidRegs field would read back as uninitialised stack.

Even a two-step compose via `let ad = cpuid_leaf_ad(…); let bc =
cpuid_leaf_bc(…); CpuidRegs { eax: ad, … }` does not help: the
`u64` → `u32` cast on each field is still an arithmetic subtree,
not a `Var` node. And if we widened the record to `CpuidPairs { ad:
u64, bc: u64 }` matching the current pair shape, we get zero
ergonomic improvement — callers still do the shift + mask + cast at
every use site.

**Gap B — a raw-assembly body is clobbered by Slice C's
unconditional `emit_callee_sret_splice`.**

The alternative to a RecordCons body is a hand-written
`fn (leaf: u32, subleaf: u32) -> unsafe { block: { push rbx; cpuid;
mov [rdi+0], eax; …; pop rbx; mov rax, rdi; ret } }`. But
`emit_ret` fires `emit_callee_sret_splice` unconditionally whenever
the current Lambda has `return_record_layout` populated and is not
`@no_frame`. The splice would emit `sub rsp, padded` + copy-from-
uninitialised-source-buffer + `mov rax, rdi` AFTER the hand-written
stores, clobbering them.

The `@no_frame` opt-out (the only current gate on the splice) is a
semantic overload: it means "no frame-pointer prologue", not "skip
the record-return splice". Using it as an ad-hoc splice suppressor
would leak Slice C's implementation detail into the .pdx surface
and is not the discipline the rest of the stdlib follows.

**Gap C — stdlib-lowering recipes do not participate in
`Symbol::return_record_layout`.**

`stdlib_lowering::lower_stdlib_method` intercepts calls by
`(trait_name, method_name)` pair. Trait-method resolution builds a
Symbol for the substituted callee, but
`populate_return_record_layouts` (Slice A) only walks item-level
`Let` bindings — it does not inspect trait declarations. So a
`fn cpuid_leaf(leaf: u32, sub: u32) -> CpuidRegs` trait method
would not receive a `Symbol::return_record_layout` entry.

Consequences:
- `emit_call.rs`'s Slice B probe returns `Absent` for recipe-driven
  callees, so no caller-side sret prelude / arg shift / slot
  allocation fires. The recipe must marshal args at their SysV
  positions (RDI = leaf, RSI = subleaf) and produce its return in
  RAX+RDX per the register-return convention.
- No caller-side persistent frame slot is allocated, so field
  accesses `r.eax`, `r.ebx`, etc. have nowhere to lower from —
  the record subsystem sees a scalar return.

Wiring recipe-driven callees into the layout side-table requires
either (a) extending `populate_return_record_layouts` to walk
trait declarations and stamp a Symbol per trait method, or (b) a
parallel recipe-side layout table + emit_call probe. Both are
substantive elaborator surgery, well beyond a "single follow-up
wave" scope.

## Path A analysis

**INFEASIBLE for this wave.** Gap A + Gap B eliminate the two
possible shapes for a `.pdx` `cpuid_leaf` wrapper:
- RecordCons body: Gap A silently drops call+arithmetic fields.
- Raw-asm body: Gap B clobbers hand-written stores.

Even a hybrid where `let ad = cpuid_leaf_ad(…)` binds locals and
then `CpuidRegs { eax: ad_low32, edx: ad_high32, … }` uses those
locals directly fails Gap A — the extraction from `ad` is
arithmetic (shift + mask + cast), not a bare `Var`.

## Path B analysis

**MINIMUM-SCOPE PATH B NOT LANDING.** A minimum-scope Path B would
add `ArgConvention::SysVSret { layout: RecordLayout }` opt-in per
recipe + emit_call.rs branch that:

1. On seeing a recipe with `SysVSret`, allocate a caller-side frame
   slot (recipe-side layout drives the size + align).
2. Splice the sret prelude (`lea rdi, [rsp+disp]`) BEFORE arg-
   marshalling.
3. Shift SysV arg-register selection right by one so real args land
   in RSI (leaf) + RDX (subleaf).
4. Emit CALL to the recipe target (unchanged).
5. Suppress `emit_callee_sret_splice` for recipe-driven callees so
   the recipe's own emit_ret does not receive the duplicate copy
   sequence (new callee-side flag).

Estimated one dedicated wave (Path B is not truly minimum because
Gap C is a distinct axis: recipe-driven callees need a layout side-
table entry OR emit_call must consult recipe metadata directly).
Composition testing against the existing 40+ `SysVRegs` recipes to
prove byte-shape invariance would be non-trivial.

Landing Path B alongside the Slice A-D reference machinery that just
shipped risks entangling two evolving axes (user-code sret vs
recipe sret) that should be independently reviewed.

## Deferral

Retirement remains blocked pending a dedicated follow-up ticket:

- **PAS-DEBT-B4-002-followup** — stdlib-recipe participation in
  `return_record_layout` side-table + per-recipe Slice C splice
  opt-out (addresses Gaps B + C), OR
- **PAS-DEBT-B4-002-alt** — extend Slice D's RecordCons body arm to
  handle App / arithmetic field values (addresses Gap A).

Either one alone unblocks the record-returning `cpuid_leaf`. The
first is the more general fix (unblocks all recipe-driven aggregate
returns, including a future consolidation of the crypto FFI thunks
that currently return status codes via RAX rather than typed
records); the second is the more local fix (unblocks any
composition of existing scalar-returning intrinsics into a record).

## paideia-os caller impact

**Zero.** `src/kernel/core/smp/topology.pdx` (the only in-tree
CPUID consumer, R18-M6-001/002 hybrid tagging + topology walk) uses
raw `cpuid` inside `unsafe { block: { … } }`, NOT the typed
`cpuid_leaf_ad`/`cpuid_leaf_bc` intrinsics. See the
`topology.pdx:32-41` "Encoder-gap posture" note explicitly reserving
the typed intrinsic for a future migration.

The typed AD/BC pair has no in-tree callers. Its retirement is a
cleanup, not an ergonomic prerequisite for downstream code — no
callsite churn blocks or benefits from this deferral.

## B4-003 (mldsa65_sign) status

**Not similarly retirable.** `MlDsa65::sign` (paideia-as#1525,
recipe in `stdlib_lowering/mldsaops.rs`) uses **Choice A**:
caller-allocated `MLDSA65_SIG_BYTES`-byte output buffer passed in
RCX + `i64` status return in RAX. This is the intended long-term
extern-C ABI shape shared by every other crypto FFI thunk
(`argon2id_derive`, `chacha20_poly1305_seal`/`open`,
`ml_kem_768_*`, `mldsa65_verify`).

`mldsaops.rs` docblock, §"Calling convention — sign":

> (A) the caller passes a pointer to a caller-allocated
> `MLDSA65_SIG_BYTES`-byte buffer and the thunk writes into it,
> returning a status code in RAX; (B) the callee returns a
> `{ bytes: [u8; 3309] }` record via an sret slot. (A) is chosen
> because it matches how every other extern-C crypto thunk in this
> codebase already works.

B4-003 is not a record-return workaround. It does not benefit from
Slice A-D and does not benefit from either Path A or Path B above.
Its retirement — if any is called for — is a separate design
question about crypto-primitive error surfacing, not sret marshalling.

## Files touched

- `Cargo.toml` — workspace.version 0.36.74 → 0.36.75.
- `CHANGELOG.md` — v0.36.75 entry.
- `crates/paideia-as-elaborator/src/stdlib_lowering/cpuidops.rs` —
  docblock rewritten against v0.36.75 reality (retirement-status
  section replaced; Gap A/B/C analysis added; Path B minimum-scope
  estimate added; B4-003 non-similarity noted).
- `.plans/scratch/CHANGELOG-1524-b4002-cpuid-retirement-attempt.md`
  — this file.

## Constraints observed

- **No aggregate_return helper modifications.** The Slice-C consumer
  helpers (`sysv_callee_sret_store`, `sysv_callee_load_return_pair`,
  MS mirrors) are unchanged.
- **No Slice A-D machinery modifications.** `emit_call.rs`,
  `emit_walker/emit_core.rs`, `return_record_layout_pass.rs`,
  `return_record_cons_pass.rs`, `emit_visit_lambda.rs` untouched.
- **No SysVRegs recipe changes.** All 40+ existing recipes emit
  byte-identical instruction streams.
- **No new intrinsic ABI.** `ArgConvention` still has only `Literal`
  and `SysVRegs`.
- **Non-exhaustive match reminder.** No `match` arms touched in
  this landing; `SysvReturnPlacement` / `MsReturnPlacement` wildcards
  in the referenced Slice C code path continue to compile.
- **Encoder pitfalls not triggered.** Doc-only landing; no
  instruction emission changed.
