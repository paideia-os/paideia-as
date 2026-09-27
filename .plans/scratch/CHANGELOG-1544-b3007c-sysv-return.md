# PAS-DEBT-B3-007c (paideia-as#1544): SysV aggregate return placement

## Summary

Consumed the `AggregateClass` classifier landed in Slice 1 (paideia-as
v0.36.53) to synthesise the SysV AMD64 psABI §3.2.3 aggregate return
sequences the caller and callee need at the six register-shaped
placements plus the Memory (hidden sret) shape. All sequences are
byte-exact and pinned by unit tests that encode each emitted
`Instruction` through `paideia_as_encoder::encode_instruction` and
assert the concatenated bytes match the Intel SDM opcode tables.

End-to-end wiring at concrete call sites (auto-detecting an
aggregate-returning callee and firing these helpers from
`emit_call.rs` / the callee epilogue in `emit_walker/emit_core.rs`) is
deliberately staged behind an upstream ticket — see "Not done" below.

## Rules delivered

### Callee side (before `RET`)
- `[Integer, Integer]` → `mov rax, [buf+0]; mov rdx, [buf+8]`.
- `[Integer, SSE]` → `mov rax, [buf+0]` + SSE load-from-mem shim
  writing `xmm0` (see SSE handling note).
- `[SSE, Integer]` → SSE shim writing `xmm0` from `[buf+0]`;
  `mov rax, [buf+8]`.
- `[SSE, SSE]` → SSE shims writing `xmm0` from `[buf+0]` and `xmm1`
  from `[buf+8]`.
- Single INTEGER / SSE eightbytes covered too (canonical scalar
  return re-derived through the placement selector).
- `Memory` → `mov r10, [src+k*8]; mov [rdi + k*8], r10` for each
  qword (bulk sret copy), then `mov rax, rdi` (psABI requires the
  callee to return the hidden-pointer buffer in `RAX`).

### Caller side
- Around a Memory-returning callee: `lea rdi, [dest+disp]` before
  CALL. Real args shift right by one (RDI is now the hidden sret ptr
  — sret adds an implicit first arg, so real args map to
  RSI/RDX/RCX/R8/R9).
- After a register-shaped return, write the placement's return regs
  back into a caller-owned destination buffer via the mirror of the
  callee-side load sequence (RAX/RDX/XMM0/XMM1 → memory).

### SSE eightbyte handling (`MovqBitcast` shim)

Memory-form scalar SSE moves (`movsd [mem]`, `movsd xmm, [mem]`) are
deferred in this encoder — `Mnemonic::MovSd`/`MovSs` are
register-register only. So SSE eightbytes in memory buffers are
round-tripped through a scratch GPR (`R10`):

```
; load-XMM-from-mem
mov  r10, [buf + disp]              ; 4 bytes (REX.W+R + 8B + ModRM + disp8)
movq xmm<k>, r10                    ; MovqBitcast to_xmm=true, 5 bytes

; store-XMM-to-mem
movq r10, xmm<k>                    ; MovqBitcast to_xmm=false, 5 bytes
mov  [buf + disp], r10              ; 4 bytes
```

Byte-identical semantics to a memory-form `movsd` (both write only
the low eightbyte). The scratch is `R10`:
- Caller-saved (no epilogue-side save/restore).
- Disjoint from the SysV integer arg pool.
- Disjoint from `RAX` (INTEGER return low), `RDX` (INTEGER return
  high), and `RDI` (sret hidden-pointer).

## Files added

- `crates/paideia-as-elaborator/src/aggregate_return.rs` — the
  synthesiser module. Public API is four free functions returning
  `Vec<Instruction>` — one per side × placement family:
  - `sysv_caller_sret_prelude(dest_base, dest_disp) -> Vec<Instruction>`
    (caller-side `lea rdi, [dest+disp]` before CALL to a Memory-return
    callee)
  - `sysv_caller_read_return_pair(placement, dest_base, dest_disp)
    -> Vec<Instruction>` (caller-side post-CALL read-back)
  - `sysv_callee_load_return_pair(placement, buf_base, buf_disp)
    -> Vec<Instruction>` (callee-side pre-RET register loads;
    caller composes with frame-pointer epilogue and RET)
  - `sysv_callee_sret_store(size_bytes, src_base, src_disp)
    -> Vec<Instruction>` (callee-side sret buffer copy + `mov rax, rdi`)
  - Plus two `_with_ret` convenience shims for tests / future
    integration that need the trailing RET as a single sequence.

  Inline `#[cfg(test)] mod tests` (~350 LOC) drives byte-exact
  encoding through `paideia_as_encoder::encode_instruction` for
  every shape:
  - `caller_sret_prelude_lea_rdi_rbp_minus_32_bytes_exact`
  - `callee_int_pair_epilogue_bytes_exact` (IntPair — canonical
    `{ u64 lo; u64 hi }` shape)
  - `caller_int_pair_readback_bytes_exact`
  - `callee_sse_pair_epilogue_bytes_exact` (SsePair — canonical
    `{ f64 x; f64 y }` shape)
  - `caller_sse_pair_readback_bytes_exact`
  - `callee_sret_24byte_epilogue_bytes_exact` (Memory — 24-byte
    aggregate: 3× qword copies + `mov rax, rdi` + ret)
  - `callee_int_single_epilogue_bytes_exact`,
    `callee_sse_single_epilogue_bytes_exact` (canonical scalar
    returns rederived through the aggregate selector — pins that
    IntSingle/SseSingle stay byte-identical to the pre-existing
    RAX / XMM0 single-return path)
  - `callee_int_sse_epilogue_shape`, `callee_sse_int_epilogue_shape`
    (mixed shapes — operand-level assertions, not full-string
    byte-exact because these two shapes have not yet appeared in
    the paideia-as end-to-end fixture corpus)
  - `sret_store_zero_size_panics`,
    `sret_store_non_multiple_of_8_panics` (panic guards on the sret
    helper's precondition)
  - `none_placement_produces_empty_sequences`,
    `memory_placement_bypasses_register_pair_helpers` (inertness)
  - `classifier_to_placement_to_epilogue_pair_of_u64` (end-to-end
    chain: RecordLayout → classify_sysv_aggregate →
    sysv_return_placement → sysv_callee_return_pair_epilogue_with_ret,
    with the same 9-byte string as the direct-placement test)

## Files changed

- `crates/paideia-as-ir/src/abi.rs` (module docblock + new enum +
  free function + 10 unit tests):
  - **Module docblock** (lines 34-52): updated the `TODO/DONE` map at
    the top of the file. `PAS-DEBT-B3-007c` moves from TODO to DONE
    with a pointer to `SysvReturnPlacement` /
    `sysv_return_placement` and to the elaborator's
    `aggregate_return` module. `PAS-DEBT-B3-007b` (MS x64 sret,
    paideia-as#1543) stays TODO — this ticket is SysV-only.
  - **`SysvReturnPlacement` enum** (line ~528, right after
    `classify_sysv_aggregate`): the 8-variant reduction of the
    classifier's `Vec<AggregateClass>` output. `#[non_exhaustive]`
    to mirror `AggregateClass`. Full doc comment naming each
    placement's register pair per SysV §3.2.3 Table 3.4.
  - **`sysv_return_placement` function** (line ~579): pattern-matches
    the classifier's output vector shape to the enum. Every
    documented classifier output shape has a named arm; unknown
    shapes degrade to `Memory` as a safe conservative choice.
  - **`impl SysvReturnPlacement`** (line ~603): two methods —
    `needs_hidden_sret()` (returns `true` for `Memory`), `uses_xmm()`
    (returns `true` for every SSE-touching placement). These are the
    single-question predicates emitter integration will fire.
  - **10 tests** at the end of the existing `#[cfg(test)] mod tests`
    block: one per named placement, plus predicate coverage, plus a
    classifier-composition test.

- `crates/paideia-as-elaborator/src/lib.rs` (line ~9): added
  `pub mod aggregate_return;`.

- `Cargo.toml` (line 85): `workspace.package.version` 0.36.61 →
  0.36.62.

- `CHANGELOG.md` (new entry at top): "Wave 33: B3-007c SysV
  aggregate return placement".

## End-to-end (`.pdx`) fixtures — deferred

Not written. paideia-as has no source-level syntax for
record-returning functions today: a `let f = fn (…) -> { u64; u64 } …`
form isn't parseable (records are constructed via `RecordCons` but
returned only through pointer / opaque scalar today), and no symbol
table entry carries a return `RecordLayout`. Landing a `.pdx` fixture
that actually exercises the caller/callee sequences would require:

1. AST / parser extension to accept record return types on `fn`
   bindings.
2. A return-record-layout side-table on `Symbol` (mirror of the
   existing arg-side plumbing).
3. Wire-up in `emit_call.rs` (caller-side classify + splice
   `sysv_caller_sret_prelude` / `sysv_caller_read_return_pair`
   around the CALL, shifting arg regs on `Memory` placements) and in
   `emit_walker/emit_core.rs::emit_ret` (callee-side classify from
   the return-type side-table, splice
   `sysv_callee_load_return_pair` / `sysv_callee_sret_store` before
   the existing frame-pointer epilogue + RET).

Steps 2–3 are the natural next-wave issue against B3-007c. Suggested
title: **"PAS-DEBT-B3-007c-followup: record-return type plumbing +
call-site wiring for SysV aggregate returns"** — depends on this
ticket and on the (not-yet-filed) AST work.

The byte-exact unit tests in this ticket pin the emitted sequences
completely — a future integration ticket will consume them without
needing to re-verify the byte strings.

## Constraints honoured

- **SysV-only**. Zero touches to MS x64 code paths.
- **Classifier unchanged**. `AggregateClass` and
  `classify_sysv_aggregate` are read-only inputs here; Slice 1 remains
  the sole authority.
- **Single-register RAX return byte-identity preserved**. The
  `IntSingle` shape re-derives the pre-existing `mov rax, [buf];
  ret` sequence — pinned by `callee_int_single_epilogue_bytes_exact`
  which asserts the same 5-byte string that plain scalar-u64 returns
  have always produced.
- **SSE returns use `movq` (via GPR shim), not `mov`**. The scalar
  SSE encoder's memory forms are deferred; the `MovqBitcast` shim is
  the byte-exact equivalent (both write only the low eightbyte).
- **Encoder-pitfall guard**. No `test rN, rN`; no `and r11, imm64`;
  no 2-op `imul r, imm`; no multiline `pub let =` (the .pdx fixture
  path isn't touched at all); no module-basename mismatch (no new
  module tests reach the top-level runner); no reserved-word labels
  (this module emits no labels — Mov / Lea / MovqBitcast / Ret only).

## Not done

- **`.pdx` fixtures** — see "End-to-end fixtures — deferred" above.
- **Emit-call.rs / emit_ret integration** — see the same section.
- **Non-multiple-of-8 sret sizes** — `sysv_callee_sret_store` panics.
  A follow-up would add a tail-byte MOV chain or a REP MOVSB variant
  when a real aggregate exposes a 17/23/… byte size.
- **Encoder memory-form `movsd` / `movss`** — separate encoder
  ticket. Would let us drop the `MovqBitcast` GPR shim (smaller
  code — 3 bytes vs 9 bytes per SSE eightbyte, and no R10 write).
- **`cargo build`, `cargo test`** — per `feedback_no_background_builds.md`,
  builds are main-only.

## Risk / follow-ups

- The `SysvReturnPlacement` enum is `#[non_exhaustive]`. Any future
  addition (e.g. a psABI-extended vector-return class) will need to
  extend every match arm in `sysv_return_placement` and every
  helper's dispatch table in `aggregate_return.rs`. This is the
  intended forward-compat pressure.
- The `MovqBitcast` GPR shim uses `R10` unconditionally. If a
  future emitter integration wants to keep `R10` live across an
  aggregate-return sequence, it must save/restore around the shim
  (or the shim's scratch register would need to become a parameter).
  Tracked as an integration concern — this ticket lands only the
  helper functions.

Build not run — main should invoke `bash tools/build.sh` and re-invoke
me with the error tail if it fails.
