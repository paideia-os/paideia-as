# Wave 57 (v0.36.81) — paideia-as#1560: @endian(be) signed-narrow (i16/i32) recipe

Retires T0567 diagnostic for the i16/i32 case by landing the correct
three-instruction lowering on both load and store sides. T0567
remains as a defensive arm for genuinely-unsupported widths
(anything other than 1/2/4/8).

## Scope

Wave 56 (v0.36.80, #1508 B2-015) landed `@endian(be)` byte-swap for
u8/u16/u32/u64/i64. Signed narrow (i16/i32) with `@endian(be)`
refused with T0567 because a bare `bswap r32` after a
`movsxd r64, [mem]` leaves stale sign-extension in the upper 32
bits — a silent miscompile hazard the Wave-56 landing explicitly
guarded against pending the follow-up recipe.

Wave 57 lands the follow-up.

## Load recipe (@endian(be), signed narrow)

**i16**: `movsx r64, word[mem]; rol r16, 8; movsx r64, r16`
  * `movsx r64, word[mem]` — the pre-existing `emit_widening_load`
    dispatch (opcode 0x0F BF /r, operand_size 2). Loads raw
    big-endian bytes as a mis-signed 64-bit value.
  * `rol r16, 8` — swaps the low 16 bits in place (Rol{W16},
    66h prefix). Upper 48 remain stale sign-extension.
  * `movsx r64, r16` (reg-reg) — re-derives sign from the swapped
    low half. Upper 48 now track the true sign.

**i32**: `movsxd r64, dword[mem]; bswap r32; movsxd r64, r32`
  * `movsxd r64, dword[mem]` — the pre-existing `emit_widening_load`
    dispatch (opcode 0x63 /r, operand_size 4).
  * `bswap r32` — swaps the low 32 bits. Zero-extends upper 32
    (Intel-defined) — wrong for negative values.
  * `movsxd r64, r32` (reg-reg) — re-derives sign from the swapped
    low half. Upper 32 now track the true sign.

## Store recipe (@endian(be), signed narrow)

**i16**: `mov r11, rdx; rol r11w, 8; mov word[rdi], r11w`
**i32**: `mov r11, rdx; bswap r11d; mov dword[rdi], r11d`

Both are byte-identical in shape to the u16/u32 store recipes
Wave 56 landed. No re-sign-extend is needed on the store side
because `MovSized{width}` narrows the write to exactly `field_size`
bytes and drops the stale upper bits left by `rol r16` / `bswap r32`
on R11.

## Files changed

* `crates/paideia-as-elaborator/src/emit_field_access.rs`
  * `emit_endian_load_swap_if_needed` — appends
    `emit_movsx_widen_after_swap` when `field_signed && size in {2, 4}`.
  * `emit_endian_store_swap_if_needed` — the Wave-56 signed-narrow
    T0567 refusal is removed; falls through to the standard
    scratch-copy + `emit_bswap_low_bits` path.
  * `emit_bswap_low_bits` — signed-narrow refusal branch removed;
    `field_signed` is now unconsulted inside the helper (renamed
    to `_field_signed`). T0567 kept as the defensive arm for
    truly-unsupported widths.
  * `emit_movsx_widen_after_swap` — new private helper. Emits
    reg-reg `movsx r64, r16` (opcode 0x0F, operand_size 2) or
    `movsxd r64, r32` (opcode 0x63, operand_size 4).
  * `t0567_code()` doc updated to reflect the retirement + the
    surviving defensive arm.

* `crates/paideia-as-elaborator/src/emit_walker_tests/endian_byteswap.rs`
  * `i16_be_load_swap_and_sign_extend` — byte-exact assertion.
  * `i32_be_load_swap_and_sign_extend` — byte-exact assertion.
  * `i16_be_store_truncate_swap_narrow` — byte-exact assertion.
  * `i32_be_store_truncate_swap_narrow` — byte-exact assertion.

## Preservation

* Wave 56 semantics for unsigned + i64/u64 are byte-identical
  (`emit_bswap_low_bits` now takes `_field_signed` and dispatches
  purely on `field_size`; the emission for sizes 2/4/8 unsigned
  is unchanged).
* Unannotated load/store paths remain byte-identical
  (HashMap-miss in `field_endian` short-circuits before any
  new code runs).
* `@endian(le)` remains a no-op on the native little-endian
  x86_64 target for every scalar width.

## Encoder pitfalls avoided

* `movsx r64, r16` reg-reg carries `EncodingHint { opcode: 0x0F,
  operand_size: 2 }` — the two-byte 0F BF /r escape, matching
  `emit_field_access_movsx_reg`'s existing convention and
  `encode_movsx`'s dispatch through `movsx_reg64`.
* `movsxd r64, r32` reg-reg carries `EncodingHint { opcode: 0x63,
  operand_size: 4 }` — the single-byte MOVSXD.
* No `test rN,rN` (banned encoder pitfall from
  reference_pdx_encoder_pitfalls memory) — not needed here.
* Non-exhaustive match reminder: `emit_endian_load_swap_if_needed`
  still destructures `Endianness` explicitly (`Le` / `Be`) so a
  new variant on the AST enum triggers a compile break.

## Version

* workspace.version: 0.36.80 → 0.36.81
* CHANGELOG.md: v0.36.81 entry prepended above v0.36.80

## Retirement scope

T0567 fires for one remaining case: `@endian(be)` on a scalar with
`field_size` other than 1/2/4/8 — a genuinely-unsupported width
that shouldn't arise from any current parser path. Keeping the
diagnostic (rather than deleting it entirely) preserves the
defensive backstop.
