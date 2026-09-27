# PAS-DEBT-B3-007b (paideia-as#1543): MS x64 aggregate return placement

## Summary

Completes the x86-64 aggregate return-value ABI trilogy. Slice 1
(B3-007a, v0.36.53) landed the SysV classifier; B3-007c (v0.36.62,
Wave 33) landed the SysV placement + byte-exact instruction sequences.
This ticket lands the Microsoft x64 side: a purpose-built classifier
(binary Register/Memory, not eightbyte-split) and byte-exact instruction
sequences for the four MS placements (`None`, `IntSingle`, `XmmSingle`,
`Memory`). All new sequences are pinned by `#[cfg(test)]` byte-string
assertions that encode each emitted `Instruction` through
`paideia_as_encoder::encode_instruction` and compare against the Intel
SDM opcode tables.

End-to-end wiring at concrete call sites remains staged behind the
same upstream ticket the SysV side waits on (a return-record-layout
side-table on `Symbol`); see "Not done" below.

## MS x64 ABI rules pinned

### Classifier
- **Single scalar float/double** aggregate (one field, `is_float=true`,
  size ∈ {4, 8}, fills the aggregate at offset 0) → `Register(Xmm)`.
  This mirrors the plain scalar-float return path — a bare `f32`/`f64`
  return goes in `XMM0`, and MS treats a single-scalar-float wrapper
  struct equivalently.
- **Register-sized aggregate** (size ∈ {1, 2, 4, 8}, not a scalar
  float wrapper) → `Register(Int)`. This includes multi-field
  aggregates like `{ u32 a; u32 b }` and mixed float-pair aggregates
  like `{ f32 a; f32 b }`, which MS returns in `RAX` even though both
  fields are float — the MS ABI's XMM0 slot is reserved for a *single*
  scalar float/double, not for aggregates.
- **Anything else** (size ∉ {1, 2, 4, 8}: 0, 3, 5, 6, 7, > 8) →
  `Memory`. Unlike SysV, MS does **not** split an aggregate across two
  registers — anything over 8 bytes goes through sret.

### Placement selector
- `MsReturnPlacement::None` — void / zero-size aggregate return; used
  only from the `_from_layout` wrapper.
- `MsReturnPlacement::IntSingle` — `RAX`.
- `MsReturnPlacement::XmmSingle` — `XMM0`.
- `MsReturnPlacement::Memory` — hidden `RCX` sret pointer; callee
  copies into `[RCX]` and returns that same pointer in `RAX`.

### Callee side (before `RET`)
- `IntSingle` → `mov rax, [buf+disp]`.
- `XmmSingle` → `mov r10, [buf+disp]; movq xmm0, r10` (via the
  `MovqBitcast` shim reused from the SysV side — memory-form scalar
  SSE moves remain a deferred encoder extension).
- `Memory` → `mov r10, [src+k*8]; mov [rcx+k*8], r10` per qword,
  then `mov rax, rcx` (MS x64 requires the callee to return the sret
  buffer pointer in `RAX`).

### Caller side
- Around a Memory-returning callee: `lea rcx, [dest+disp]` before
  CALL. Real args then shift right by one to `RDX / R8 / R9 / stack`
  past the 32-byte MS shadow space.
- After a register-shaped return, write the placement's return
  register (RAX or XMM0) back into a caller-owned destination buffer
  via the mirror of the callee-side load sequence.

## Differences from SysV (pinned in tests)

1. Caller sret prelude uses `RCX` (implicit first MS arg) instead of
   `RDI` (implicit first SysV arg). Byte-exact test compares
   `48 8D 4D E0` (MS) against `48 8D 7D E0` (SysV) — same LEA form,
   only the destination register differs.
2. MS never splits an aggregate across two return registers. No MS
   analogue of SysV's `IntPair` / `IntSse` / `SseInt` / `SsePair`
   shapes; the classifier's return type is a single `MsAggregateClass`
   rather than a `Vec<AggregateClass>`.
3. Callee sret store writes to `[RCX]` (not `[RDI]`) and returns that
   pointer in `RAX` — byte-exact test compares
   `48 89 C8` (mov rax, rcx) against `48 89 F8` (mov rax, rdi).

## SSE scratch GPR reuse

The `MovqBitcast` GPR shim used for the `XmmSingle` placement reuses
the same `SSE_SCRATCH_GPR` (`R10`) the SysV side uses. `R10` is:
- Caller-saved (no epilogue-side save/restore).
- Disjoint from `RCX` (MS sret pointer / MS arg-0).
- Disjoint from `RDX / R8 / R9` (real MS args after sret).
- Disjoint from `RAX` (return-pointer / return-value register).

So the shim never aliases a live MS return-value or arg register, and
the choice reads uniformly across both ABIs.

## Files changed

- `crates/paideia-as-ir/src/abi.rs`
  - **Module docblock** (top of file): updated the TODO/DONE map.
    `PAS-DEBT-B3-007b` moves from TODO to DONE with a pointer to
    `MsAggregateClass` / `MsRegClass` / `classify_ms_aggregate` /
    `MsReturnPlacement` / `ms_return_placement` and to the elaborator's
    `aggregate_return` module.
  - **New MS x64 section** (immediately before `fn merge_classes`):
    - `MsAggregateClass` enum (2 variants: `Register(MsRegClass)`,
      `Memory`), `#[non_exhaustive]`.
    - `MsRegClass` enum (2 variants: `Int`, `Xmm`),
      `#[non_exhaustive]`.
    - `classify_ms_aggregate(&RecordLayout) -> MsAggregateClass` with
      the single-scalar-float special case.
    - `MsReturnPlacement` enum (4 variants: `None`, `IntSingle`,
      `XmmSingle`, `Memory`), `#[non_exhaustive]`.
    - `ms_return_placement(MsAggregateClass) -> MsReturnPlacement`
      reducer.
    - `ms_return_placement_from_layout(&RecordLayout) ->
      MsReturnPlacement` layout-driven entry point (handles
      `size == 0` → `None`).
    - `impl MsReturnPlacement` with `needs_hidden_sret()` and
      `uses_xmm()` predicates.
  - **15 new tests** in the existing `#[cfg(test)] mod tests` block:
    `classify_ms_single_u64_is_register_int`,
    `classify_ms_single_f64_is_register_xmm`,
    `classify_ms_single_f32_is_register_xmm`,
    `classify_ms_pair_u32_is_register_int`,
    `classify_ms_two_f32_in_one_reg_is_register_int`,
    `classify_ms_size_3_is_memory`,
    `classify_ms_size_16_is_memory`,
    `classify_ms_size_24_is_memory`,
    `ms_return_placement_int_maps_to_rax`,
    `ms_return_placement_xmm_maps_to_xmm0`,
    `ms_return_placement_memory_flags_hidden_sret`,
    `ms_return_placement_from_layout_zero_size_is_none`,
    `ms_return_placement_from_layout_composes_with_classifier`,
    `ms_placement_uses_xmm_only_for_xmm_single`,
    `ms_placement_needs_hidden_sret_only_for_memory`,
    `ms_return_placement_from_layout_scalar_f64_is_xmm_single`.

- `crates/paideia-as-elaborator/src/aggregate_return.rs`
  - **Module docstring** rewritten to cover both SysV and MS x64
    sides, keeping the SysV-specific sections intact.
  - **Import**: `use paideia_as_ir::abi::{MsReturnPlacement,
    SysvReturnPlacement};` (SysV import preserved).
  - **New MS section** (between `sysv_callee_sret_epilogue_with_ret`
    and the tests module) with 6 public free functions:
    - `ms_caller_sret_prelude(dest_base, dest_disp) -> Vec<Instruction>`
    - `ms_caller_read_return_reg(placement, dest_base, dest_disp) -> Vec<Instruction>`
    - `ms_callee_load_return_reg(placement, buf_base, buf_disp) -> Vec<Instruction>`
    - `ms_callee_sret_store(size_bytes, src_base, src_disp) -> Vec<Instruction>`
    - `ms_callee_return_reg_epilogue_with_ret(placement, buf_base, buf_disp) -> Vec<Instruction>`
    - `ms_callee_sret_epilogue_with_ret(size_bytes, src_base, src_disp) -> Vec<Instruction>`
    All helpers reuse the existing private primitives
    (`mov_reg_from_mem`, `mov_mem_from_reg`, `mov_reg_from_reg`,
    `lea_reg_from_mem`, `load_xmm_from_mem`, `store_xmm_to_mem`) —
    zero code duplication with the SysV side.
  - **10 new byte-exact tests** appended to the existing `mod tests`
    block:
    - `caller_ms_sret_prelude_lea_rcx_rbp_minus_32_bytes_exact`
      (`48 8D 4D E0`)
    - `callee_ms_int_single_load_bytes_exact`
      (`48 8B 45 F8 C3`)
    - `callee_ms_xmm_single_load_bytes_exact`
      (`4C 8B 55 F8 66 49 0F 6E C2 C3`)
    - `caller_ms_int_single_readback_bytes_exact`
      (`48 89 45 F8`)
    - `caller_ms_xmm_single_readback_bytes_exact`
      (`66 49 0F 7E C2 4C 89 55 F8`)
    - `callee_ms_sret_24byte_epilogue_bytes_exact` — 27-byte sequence
      with the 3 qword copies + `mov rax, rcx` + `ret`
    - `ms_sret_store_zero_size_panics` (panic guard)
    - `ms_sret_store_non_multiple_of_8_panics` (panic guard)
    - `ms_none_placement_produces_empty_sequences`
    - `ms_memory_placement_bypasses_register_helpers`
    - `ms_classifier_to_placement_to_sret_epilogue_24byte` (end-to-end
      classifier → placement → sequence chain, same 27-byte string as
      the direct-placement test)

- `Cargo.toml`: `workspace.package.version` 0.36.62 → 0.36.63.
- `CHANGELOG.md`: new entry at top — "Wave 34: B3-007b MS x64 aggregate
  return placement".

## Byte-exact test list

Elaborator-side (`aggregate_return.rs`):

| Test | Expected bytes |
|------|----------------|
| `caller_ms_sret_prelude_lea_rcx_rbp_minus_32_bytes_exact` | `48 8D 4D E0` (4 B) |
| `callee_ms_int_single_load_bytes_exact` | `48 8B 45 F8 C3` (5 B) |
| `callee_ms_xmm_single_load_bytes_exact` | `4C 8B 55 F8 66 49 0F 6E C2 C3` (10 B) |
| `caller_ms_int_single_readback_bytes_exact` | `48 89 45 F8` (4 B) |
| `caller_ms_xmm_single_readback_bytes_exact` | `66 49 0F 7E C2 4C 89 55 F8` (9 B) |
| `callee_ms_sret_24byte_epilogue_bytes_exact` | 27-byte sequence (see docstring) |
| `ms_classifier_to_placement_to_sret_epilogue_24byte` | identical 27 B via layout-driven chain |

## Call-site wiring status

The MS x64 emit-call *arg-marshalling* path already exists — see
`crates/paideia-as-elaborator/src/emit_call.rs` around lines 236, 273,
296, 373, 528, 542, and 1156 (all guarded by
`callee_abi == CallingConvention::Ms`). It already handles the 4-arg
register pool, the 32-byte shadow space, and the odd-count alignment
pad (`MS_CALL_STACK_BUMP_ODD_PAD`, #1192).

The *aggregate-return* path (both the caller-side sret prelude +
readback and the callee-side sret store) is **not yet wired** to any
call site. Wiring gates on the same upstream work the SysV side waits
on:
1. AST / parser extension to accept record return types on `fn`
   bindings (not yet parseable — `let f = fn (…) -> { u64; u64 } …`
   is rejected today).
2. A return-record-layout side-table on `Symbol` (mirror of the
   existing arg-side plumbing).
3. Wire-up in `emit_call.rs` (classify + splice
   `ms_caller_sret_prelude` / `ms_caller_read_return_reg` around the
   CALL under the `callee_abi == CallingConvention::Ms` branch,
   shifting arg regs on `Memory` placements) and in
   `emit_walker/emit_core.rs::emit_ret` (splice
   `ms_callee_load_return_reg` / `ms_callee_sret_store` before the
   frame-pointer epilogue + RET when the callee's return is a
   `#[abi("ms")]`-tagged aggregate).

Steps 2–3 are the natural next-wave issue against the trilogy. The
byte-exact unit tests in this ticket + B3-007c pin the emitted
sequences completely — a future integration ticket consumes them
without re-verifying the byte strings.

## Constraints honoured

- **MS-only additions**. Zero touches to SysV helpers, primitives, or
  tests. SysV byte-identity preserved by construction (no changes to
  `sysv_*` functions or to the shared low-level primitives).
- **Classifier for SysV unchanged**. `AggregateClass` /
  `classify_sysv_aggregate` / `SysvReturnPlacement` are read-only
  inputs / untouched neighbours in `abi.rs`.
- **No encoder changes**. The `MovqBitcast` shim already exists from
  B3-007c; reused verbatim for MS `XmmSingle`.
- **Encoder-pitfall guard**. No `test rN, rN`; no `and r11, imm64`;
  no 2-op `imul r, imm`; no multiline `pub let =` (the .pdx fixture
  path is not touched at all); no module-basename mismatch; no
  reserved-word labels (this module emits no labels — Mov / Lea /
  MovqBitcast / Ret only).
- **R11 avoided**. Scratch is `R10` throughout (matches B3-007c);
  R11 is the encoder's scratch and remains untouched.

## Not done

- **`.pdx` fixtures** — see call-site wiring status above.
- **Emit-call.rs / emit_ret integration for MS aggregate returns** —
  see the same section.
- **Non-multiple-of-8 sret sizes** — `ms_callee_sret_store` panics
  (mirrors the SysV side). A follow-up would add a tail-byte MOV chain
  or a REP MOVSB variant when a real MS aggregate exposes a
  17/23/… byte size.
- **Encoder memory-form `movsd` / `movss`** — separate encoder ticket
  shared with B3-007c. Would let both ABIs drop the `MovqBitcast` GPR
  shim (smaller code — 3 bytes vs 9 bytes per SSE eightbyte, and no
  R10 write).
- **`cargo build`, `cargo test`** — per
  `feedback_no_background_builds.md`, builds are main-only.

## Risk / follow-ups

- The `MsReturnPlacement` and `MsAggregateClass` enums are
  `#[non_exhaustive]`. Any future addition (e.g. MS `__m128` vector
  returns via XMM0, or a psABI-extended packed-struct class) will
  need to extend every match arm in `ms_return_placement`,
  `ms_return_placement_from_layout`, and every helper's dispatch table
  in `aggregate_return.rs`. This is the intended forward-compat
  pressure.
- MS x64 has a subtle rule for `__m128`/`__m64` returns via XMM0 that
  this ticket does not model — the classifier treats only paideia's
  scalar `f32`/`f64` fields as `Xmm`. Paideia has no `__m128` type
  today; when SIMD lands, extend `classify_ms_aggregate` (add an
  `is_vector` field to `FieldLayout`) and add a matching test.
- The `single scalar float` special case in `classify_ms_aggregate`
  requires the field to fill the aggregate at offset 0 with no
  padding. A pathological layout with a f32 field at offset 4 and
  size 4 in an 8-byte aggregate would fall through to
  `Register(Int)`, matching what a C compiler would do for
  `struct S { char pad[4]; float x; };` (returned in RAX because it's
  not a bare scalar float).

Build not run — main should invoke `bash tools/build.sh` and re-invoke
me with the error tail if it fails.
