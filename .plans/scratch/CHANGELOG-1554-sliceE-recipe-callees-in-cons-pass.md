# paideia-as#1554 Slice E — recipe callees in `populate_return_record_cons_slots`

**Version:** 0.36.78 → 0.36.79
**Wave shape:** small, targeted (~50 lines of pass logic + 3 tests).
**Closes Wave-54 gap for:** paideia-as#1524.

## Problem

Wave 54 (v0.36.78) landed the `CpuidOps::cpuid_leaf` record-returning
recipe machinery. The recipe registers a 16 B `CpuidRegs` layout via
`enumerate_record_return_recipes()`, and `emit_call.rs`'s Slice B site
probe fires the caller-side sret prelude (`lea rdi, [sret_slot]`) for
App sites calling it. But `populate_return_record_cons_slots` — the
Slice C pass that allocates the persistent caller-frame slot — only
consumes `IrArena::return_record_layout_table`, which is keyed by
user-code Let `IrNodeId`. A stdlib recipe has no user-Let, so the
pass never registers the recipe in its `callee_info` map and no
`caller_sret_slot_table` entry is allocated for a recipe-callee App.

`emit_call.rs` then falls through to the Slice B transient path:

```
sub rsp, 16
lea rdi, [rsp+0]
<recipe splice — 9 CPUID stores>
```

…but the SysVRegs recipe splice branch in `emit_call.rs` returns
immediately after the splice, without executing the matching
`add rsp, 16` release. The caller's RSP is left 16 B low across the
recipe splice.

## Fix

`crates/paideia-as-elaborator/src/return_record_cons_pass.rs` — fold
every `enumerate_record_return_recipes()` entry into the `callee_info`
map at pass entry, keyed by the trait-qualified spelling
`"<trait_name>::<method_name>"`. This is the exact string
`walker_pipeline.rs`'s call-site scan stamps into
`CallMeta.callee_name` for a source-level `Trait::method(...)` call
(the callee's source text passes `is_valid_qualified_identifier` and
is recorded verbatim).

Recipe ABI is hard-wired to SysV: every entry in the recipe registry
uses the SysV caller convention today (recipes take args via
RDI/RSI/RDX/… after the sret shift). When a future MS-ABI recipe joins
the registry, this arm needs to consult the recipe's own ABI tag; a
comment marks the site.

The recipe loop runs AFTER the user-Let loop; a HashMap conflict
resolves recipe-wins. This is a defensive choice — the parser rejects
`::` inside plain identifiers, so a genuine collision cannot arise
from well-formed pdx.

The rest of the pass logic is unchanged: it walks each Lambda's
descendant Apps, resolves each App's `callee_name` from
`arena.call_sites()`, and looks it up in `callee_info`. Recipe callees
now flow through the same slot-packing arithmetic as user-Let callees,
so the resulting `[RBP - disp]` layout, per-caller frame bump, and
16-multiple round-up are identical.

## Tests (3 new)

Added to the same file's `#[cfg(test)] mod tests`:

1. **`recipe_callee_cpuid_leaf_allocates_persistent_slot`** — single
   App calling `CpuidOps::cpuid_leaf` (no user-Let stamp) → slot at
   `[RBP - 16]`, `padded_size = 16`, caller bump = 16.

2. **`recipe_callee_two_calls_pack_non_overlapping_slots`** — two
   recipe Apps in one caller → two distinct slots at `-16` and `-32`,
   bump = 32.

3. **`mixed_user_let_and_recipe_callees_both_get_slots`** — user-Let
   callee (24 B, 32 B padded slot at `-32`) + recipe callee (16 B
   slot at `-48`) in one caller Lambda → both slots present,
   non-overlapping; bump = 48.

Existing Slice C/D tests are unchanged and continue to pin the
user-Let path byte-identically:

  * `memory_callee_single_call_allocates_one_slot`
  * `intpair_callee_allocates_persistent_slot_in_slice_d`
  * `scalar_returning_callee_produces_no_entries`
  * `two_memory_calls_pack_non_overlapping_slots`

## Byte-exact caller sequence for a `CpuidOps::cpuid_leaf` caller

**Before Slice E** (Slice B transient fallback + unreleased):
```
sub rsp, 16                  ; 48 83 EC 10
lea rdi, [rsp+0]             ; 48 8D 3C 24
push rbx                     ; 53
mov rax, rsi                 ; 48 89 F0
mov rcx, rdx                 ; 48 89 D1
cpuid                        ; 0F A2
mov [rdi+0], eax             ; 89 07
mov [rdi+4], ebx             ; 89 5F 04
mov [rdi+8], ecx             ; 89 4F 08
mov [rdi+12], edx            ; 89 57 0C
pop rbx                      ; 5B
; splice returns — no `add rsp, 16` — RSP is 16 B low
```

**After Slice E** (persistent slot, no transient bump/release):
```
lea rdi, [rbp - slot_disp]   ; 48 8D BD <sdisp32>   (persistent)
push rbx                     ; 53
mov rax, rsi                 ; 48 89 F0
mov rcx, rdx                 ; 48 89 D1
cpuid                        ; 0F A2
mov [rdi+0], eax             ; 89 07
mov [rdi+4], ebx             ; 89 5F 04
mov [rdi+8], ecx             ; 89 4F 08
mov [rdi+12], edx            ; 89 57 0C
pop rbx                      ; 5B
; no `add rsp` needed — slot is caller-frame-owned, released by mov rsp, rbp
```

`slot_disp` is a signed 32-bit displacement (usually `-16` for the
first record-returning call in the caller). The caller's prologue's
`sub rsp, bump` reserved the space at function entry; the epilogue's
`mov rsp, rbp` releases it.

## Version bump

`Cargo.toml` `[workspace.package].version`: `0.36.78` → `0.36.79`.

## Slice-E status vs #1524

The Wave 54 "Known remaining gap" note in
`crates/paideia-as-elaborator/src/stdlib_lowering/cpuidops.rs`
docblock is now stale — Slice E closes it. Issue #1524 can close cleanly
once the Wave 54 comment is updated (documentation-only follow-up;
harmless to leave in place until the next `cpuidops.rs` touch).

## Constraints honoured

  * `enumerate_record_return_recipes()` unchanged.
  * Recipe registry (`stdlib_lowering::{cpuidops, mod}`) unchanged.
  * User-Let callee path byte-identical (no fixture changes).
  * `classify_sysv` / `classify_ms` wildcard arms retained for the
    `#[non_exhaustive]` placement enums.
  * No new encoder work; no `test rN,rN`, `and r11, imm64`, 2-op
    `imul r,imm`, multiline `pub let =`, or fingerprint tag work
    involved.

## Follow-ups (out of scope, not blockers)

  * The `@no_frame` caller path continues to fall through to Slice B
    (`is_caller_no_frame` gate in `emit_call.rs` discards the
    persistent slot). Recipes called from `@no_frame` callers still
    hit the unreleased-transient shape; a separate wave can either
    forbid `@no_frame` on callers that call record-returning recipes
    or extend the splice branch to emit the missing `add rsp`.
  * The Wave 54 stale-note in `cpuidops.rs` docblock (documentation
    only) should be scrubbed on the next touch of that file.
