# paideia-as#1508 (PAS-DEBT-B2-015) — Elaborator-side `@endian(be|le)` byte-swap insertion

**Wave 55, v0.36.80, 2026-09-28.**

## Summary

Closes the deferred elaborator half of paideia-as#1372 (v0.28-M1-003).
The parser already accepted `@endian(be|le)` on integral-scalar struct
fields and stashed the annotation on `StructFieldAttrTable` per
field-name NodeId; the load/store paths ignored it. Wave 55 wires the
annotation through the struct registry into `EmitPassState` and
inserts a width-appropriate byte-swap at emit time.

## Files touched

  * `crates/paideia-as-elaborator/src/struct_registry.rs`
      * Added `field_endian: HashMap<RecordTypeId, Vec<Option<Endianness>>>`
        parallel to `fields`, `field_type_nodes`.
      * `build_struct_registry` pushes into it at exactly the same
        sites as `field_descriptors` (three vectors stay
        index-aligned).
      * Public `endian_of(type_id, field_index) -> Option<Endianness>`
        accessor.
      * Alignment test now asserts `field_endian` length matches
        `fields`.

  * `crates/paideia-as-elaborator/src/emit_pass_state.rs`
      * Added `struct_field_endian: HashMap<(RecordTypeId, u32),
        Endianness>` — sparse, only annotated fields land there.
      * Accessors: `insert_field_endian`, `field_endian`.
      * `Default` impl carries the new field.

  * `crates/paideia-as/src/cmd_build/walker_pipeline.rs`
      * After `finalise_record_layouts`, iterates
        `registry.field_endian` and calls `insert_field_endian` for
        every `Some(endian)` entry.

  * `crates/paideia-as-elaborator/src/emit_field_access.rs`
      * `visit_field_access_with_reg`: appends
        `emit_endian_load_swap_if_needed` after
        `emit_widening_load`.
      * `visit_field_assign`: prepends
        `emit_endian_store_swap_if_needed` before the store.
      * New helpers on `impl EmitWalker`:
          - `emit_endian_load_swap_if_needed`
          - `emit_endian_store_swap_if_needed`
          - `emit_bswap_low_bits`
      * New diagnostic `T0567` (signed narrow scalar + `@endian(be)`
        deferred).

  * `crates/paideia-as-elaborator/src/emit_walker_tests.rs`
      * Registers the new `endian_byteswap` submodule.

  * `crates/paideia-as-elaborator/src/emit_walker_tests/endian_byteswap.rs`
      * New file, 8 tests: 5 load-side + 3 store-side (BE variants
        for u16/u32/u64 + LE no-op + unannotated regressions).

  * `Cargo.toml` — workspace.version 0.36.79 → 0.36.80.

  * `CHANGELOG.md` — new top entry.

## Byte-order recipes

Little-endian native x86_64:

  * **u16**: `movzx r64, word[base+disp]; rol r16, 8`
      - No dedicated `bswap r16` — the 16-bit form of `bswap` is
        undefined per Intel SDM Vol 2A.
      - `rol r16, 8` (66h prefix; `Mnemonic::Rol { width: W16 }`
        with imm=8) swaps the two low bytes; upper 48 bits stay
        zero from `movzx`.

  * **u32**: `mov r32, [base+disp]; bswap r32`
      - `Mnemonic::MovSized{W32}` load zero-extends to r64.
      - `Mnemonic::Bswap32` (0F C8+rd — no REX.W) reverses low 4
        bytes and zero-extends.

  * **u64**: `mov r64, [base+disp]; bswap r64`
      - `Mnemonic::MovSized{W64}` load.
      - `Mnemonic::Bswap` (REX.W 0F C8+rd) reverses all 8 bytes.

  * **i64**: same recipe as u64 (bit-pattern identical under
    two's complement).

  * **u8/i8**: byte-swap is the identity; no-op emitted.

  * **`@endian(le)` on x86_64**: no-op.

  * **i8/i16/i32 with `@endian(be)`**: emits `T0567` and skips
    the byte-swap. The load path uses `movsx`/`movsxd`, so the
    upper bits carry stale sign-extension after a `bswap` on the
    low width. Correct sequencing requires
    `mov-low; bswap; movsx-widen` — deferred to a follow-up
    wave.

## Store recipe

For each store where `@endian(be)` is present on a supported size:

```
mov  r11, value_reg          # 64-bit copy — store width narrows on write
bswap r11 | bswap32 r11 | rol r11w, 8   # per size
mov  [base + disp], r11      # MovSized{width}, narrowed
```

R11 is the canonical last-resort scratch across
`emit_store_record.rs` and `emit_int_match.rs`; caller-saved in
SysV, outside the argument-passing sequence, so no live value
binding is clobbered.

## Test coverage (8 new tests)

Load side:
  * `endian_be_load_u64_emits_bswap_r64_after_mov`
  * `endian_be_load_u32_emits_bswap32_after_mov`
  * `endian_be_load_u16_emits_rol_r16_after_movzx`
  * `endian_le_load_u32_is_noop_on_native_le`
  * `unannotated_u32_load_is_byte_identical_to_pre_wave` (regression)

Store side:
  * `endian_be_store_u32_emits_mov_r11_bswap32_store`
  * `endian_be_store_u64_emits_mov_r11_bswap_store`
  * `unannotated_u32_store_is_byte_identical_to_pre_wave` (regression)

Plus the existing `test_build_struct_registry_alignment_with_
unsupported_field` gains a `field_endian`-length assertion.

## Scalars covered in this wave

| type      | load        | store       |
|-----------|-------------|-------------|
| u8/i8     | no-op       | no-op       |
| u16       | LANDED      | LANDED      |
| u32       | LANDED      | LANDED      |
| u64       | LANDED      | LANDED      |
| i16/i32   | T0567 defer | T0567 defer |
| i64       | LANDED      | LANDED      |

All three "primary" scalar sizes (u16/u32/u64) land in this wave.
Signed narrow (i16/i32) deferred with a diagnostic (no silent
miscompile).

## Byte-identity preservation

Every existing fixture that touches struct fields keeps the pre-
Wave-55 load/store byte sequence — the endian helpers early-return
on a HashMap-miss without touching the instruction stream. Only
sources that literally write `@endian(be|le)` in `.pdx` change
shape.

## Issue #1508 close status

**Yes — can close** on the u8/u16/u32/u64/i64 majority path. The
signed-narrow (i16/i32) sub-case is scoped-out with T0567 and
handed off as a follow-up (open an incremental issue if needed).
The parser side is unchanged.
