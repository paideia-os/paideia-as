# CHANGELOG-1554 Slice B: emit_call sret wiring + arg-shift

**Wave**: v0.36.69 (2026-09-27)
**Issue**: paideia-as#1554 (PAS-DEBT-B4-002)
**Predecessor**: v0.36.68 Slice A (record-return type plumbing)
**Successor**: Slice C (callee-side wiring + persistent frame slot + pair-unpack)

## Scope

Slice B consumes the Slice A record-return infrastructure at the emit
sites. When a callee's `Symbol` carries `return_record_layout`,
`emit_call.rs`'s new branch:

- Resolves an aggregate-return placement via
  `abi::sysv_return_placement_from_layout` /
  `abi::ms_return_placement_from_layout`.
- **On `Memory` placement**: reserves a caller-owned stack slot, splices
  the caller sret prelude (LEA into RDI/RCX), shifts real arg
  registers right by one, releases the slot after the CALL.
- **On register-return placement**: leaves the CALL emission
  byte-identical to the scalar path (Slice C wires the caller-side
  pair-unpack once a destination buffer exists).
- **On absent / scalar / None placement**: entirely no-op — the
  scalar path is preserved byte-identical (two regression tests
  pin this).

## ArgConvention decision — path (b)

Slice B lands path (b) per the Slice A report: layout-driven probe of
`Symbol::return_record_layout` at every emit-call site, NOT new
`ArgConvention::SysVSret` / `MsSret` variants. Rationale documented in
`emit_call.rs` module docstring.

## Files changed

### Additions

- `crates/paideia-as-ir/src/abi.rs` (+18 lines):
  `sysv_return_placement_from_layout` helper mirroring
  `ms_return_placement_from_layout`; module docstring `DONE` line.
- `crates/paideia-as-elaborator/src/emit_call.rs`:
  - `AggregateReturnShape` enum (Absent / SysvSret / MsSret).
  - `sret_padded_slot_bytes` helper (align/16-multiple rounding).
  - Caller-side sret branch spliced into `emit_call_args_and_call`
    after scratch pushes, before arg-marshalling.
  - `effective_arg_idx = arg_idx + sret_shift` uniform indexing shift.
  - Post-CALL sret slot release before scratch-pop.
  - T0521 diagnostic for sret-shape calls with real arg spillover
    past shifted register pool.
  - Module docstring updated with path (b) rationale.
- `crates/paideia-as-elaborator/src/emit_walker/emit_core.rs`:
  - Docstring note on `emit_ret` marking callee-side wiring as
    intentionally deferred to Slice C.
- `crates/paideia-as-elaborator/src/emit_walker_tests/sret_call_wiring.rs`
  (new file, 5 tests):
  - `sysv_memory_24b_emits_sret_sub_lea_arg_shift_add` — SysV Memory shape.
  - `ms_memory_16b_emits_sret_via_rcx_arg_shift_to_rdx_r8` — MS Memory shape.
  - `sysv_intpair_16b_leaves_call_emission_byte_identical_to_scalar` —
    register-return no-op regression.
  - `scalar_return_regression_no_sret_wiring_when_layout_absent` —
    absent-layout regression.
  - `sysv_placement_from_layout_composes_classifier_and_reducer` —
    direct unit test for the new abi helper.
- `crates/paideia-as-elaborator/src/emit_walker_tests.rs`:
  registers the new test module.
- `tests/data/sret_slice_b/sret_16b_pair.pdx` — SysV IntPair fixture.
- `tests/data/sret_slice_b/sret_24b_memory.pdx` — SysV Memory (>16B) fixture.
- `tests/data/sret_slice_b/sret_ms_16b_memory.pdx` — MS Memory
  (RCX-sret) fixture.

### Version bump

- `Cargo.toml` `workspace.package.version` → `0.36.69`.

## What's left for Slice C

- Return-position record-cons materialisation pass (callee-local
  aggregate buffer at a known RBP-relative disp).
- `emit_ret` callee-side wiring (splice `sysv_callee_sret_store` /
  `ms_callee_sret_store` for Memory, `sysv_callee_load_return_pair` /
  `ms_callee_load_return_reg` for register placement).
- Replace immediate `add rsp, N` sret release in `emit_call.rs` with
  a persistent frame slot so the caller can read fields at
  `[RBP + slot_disp + field_offset]` past the CALL.
- Emit caller-side pair-unpack (`sysv_caller_read_return_pair` /
  `ms_caller_read_return_reg`) for register-return placements once
  a destination local binding exists to name.

## Test count

- 5 new tests in `sret_call_wiring.rs` (byte-shape assertions on
  emission streams for SysV Memory, MS Memory, SysV IntPair
  register-return, scalar-return regression, and a direct abi helper
  unit test).
- 3 new `.pdx` fixtures under `tests/data/sret_slice_b/` for
  parse+layout smoke coverage of the three ABI-shape cases.
