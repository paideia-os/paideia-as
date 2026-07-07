# Stdlib Trait Method Lowering (PA-r16-007-backtrack, #1036)

## Overview

PA-r16-007-backtrack (#1036) implements **direct mnemonic lowering for bodyless stdlib trait methods**. This is a hardcoded registry that maps `(trait_name, method_name)` pairs to the instruction sequences they lower to, consulted by `emit_call` before normal SysV call-marshalling.

**Goal**: Cross-repo blocker is only `PauseOps::spin_hint()` (paideia-os spin-waits on atomic compare-and-swap). Other 4 traits get follow-up issues.

## Motivation

Stdlib trait methods like `PauseOps::spin_hint()` have **no Paideia source bodies**—they are pure hardware mnemonics. Treating them as regular function calls incurs unnecessary prologue/epilogue and call-stack overhead. Instead, they should lower directly to their intended mnemonic sequences.

Example: a spin-wait loop should emit bare `pause` instructions, not `call spin_hint; ret` stubs.

## Registry Design

Location: `crates/paideia-as-elaborator/src/stdlib_lowering.rs`.

```rust
pub fn lower_stdlib_method(
    trait_name: &str,
    method_name: &str,
    mode: InstrMode,
) -> Option<Vec<Instruction>> { ... }
```

- **Input**: `(trait_name, method_name)` extracted from the target name at emit_call.
- **Output**: `Option<Vec<Instruction>>` — the mnemonic sequence to splice, or `None` if not a stdlib method.
- **Scope**: Match arms for each supported `(trait, method)` pair. Currently:
  - `("PauseOps", "spin_hint")` → `[Instruction { mnemonic: Pause, operands: [], ... }]`

## Why Not Attribute-Based?

A proposal to use `@intrinsic("F3 90")` attributes was considered but deferred:
- Standing directive (v0.13+) freezes attribute vocabulary until v0.20.
- Hardcoded registry + match arms can be refined rapidly per phase.
- Attribute-based approach requires parser/type-checker changes; hardcoding is faster.

## Integration Points

### 1. emit_call.rs

Before arg-marshalling, a resolver checks if target_name is in the form `TraitName::method_name`:

```rust
fn resolve_stdlib_trait_method(target: &str) -> Option<(String, String)> {
    let (t, m) = target.rsplit_once("::")?;
    Some((t.to_string(), m.to_string()))
}
```

If it resolves to a known stdlib method, the recipe is emitted and the function returns early:

```rust
if let Some((trait_name, method_name)) = resolve_stdlib_trait_method(&target_name) {
    if let Some(recipe) = crate::stdlib_lowering::lower_stdlib_method(...) {
        // Emit recipe instructions, skip normal call/ret
        return;
    }
}
```

### 2. Target Name Shape

- **Source**: extracted from IR span during `populate_instruction_table` in `cmd_build.rs`.
- **Format**: literal source text, e.g., `"PauseOps::spin_hint"` when source has `PauseOps::spin_hint()`.
- **Assumption**: qualified names use `::` separator (confirmed by test fixtures using `perform Io::...` syntax).

## Currently Supported: PauseOps::spin_hint()

- **Trait**: `PauseOps` (hardware pause hint for spin-waits).
- **Method**: `spin_hint() -> ()` (no args, no return value).
- **Mnemonic**: `pause` (x86_64 opcode `F3 90`).
- **Instruction mode**: Propagates from context (`InstrMode::Mode64` or `Mode32`).

## Explicitly Deferred

Follow-up issues will add the remaining four stdlib traits:

### 1. PerCpuOps (future issue)
- **Methods**: `percpu_read`, `percpu_write`, `percpu_inc`, `percpu_dec`.
- **Blocker**: requires `disp32` operand plumbing (address-of-cpu-local register + offset).
- **Mnemonics**: `gs:` prefix + `mov`/`add`/`sub` recipes.

### 2. MmioOps (future issue)
- **Methods**: `mmio_read_u32`, `mmio_write_u32`, `mmio_read_u8`, `mmio_write_u8`.
- **Blocker**: requires ordering-fence semantics analysis.
- **Mnemonics**: `mov`, `mfence`, `lfence`, `sfence` recipes depending on constraints.

### 3. BytesOps (future issue)
- **Methods**: `memcpy`, internal `rep movsb`, `rep stosb`.
- **Blocker**: requires operand-source resolution (loading `rcx`, `rdi`, `rsi` from arguments).
- **Mnemonics**: `mov` (args→registers) + `rep movsb`/`rep stosb`.

### 4. ChecksumOps (future issue)
- **Methods**: `crc32b`, `crc32d`.
- **Blocker**: requires operand-width plumbing (immediate vs. register encodings).
- **Mnemonics**: `crc32` with width-specific encodings.

## Extension Procedure

To add a new stdlib trait method:

1. **Add match arm** to `lower_stdlib_method`:
   ```rust
   ("NewTrait", "new_method") => Some(vec![...]),
   ```

2. **Add unit test** to `stdlib_lowering.rs::tests`:
   ```rust
   #[test]
   fn new_trait_new_method_returns_expected_recipe() { ... }
   ```

3. **Add integration test** to `stdlib_pause_lowering.rs` (or new file) verifying round-trip.

4. **Update this design doc** with the new entry.

## Testing

### Unit Tests
- `crates/paideia-as-elaborator/src/stdlib_lowering.rs::tests`
  - `pause_ops_spin_hint_returns_pause_mnemonic`: verifies recipe for PauseOps::spin_hint.
  - `unknown_trait_returns_none`: unknown traits fall through.
  - `known_trait_unknown_method_returns_none`: known traits with unknown methods fall through.

### Integration Tests
- `crates/paideia-as-elaborator/tests/stdlib_pause_lowering.rs`
  - `stdlib_pauseops_spin_hint_lowers_to_pause_mnemonic`: end-to-end lowering.
  - `pause_mnemonic_encodes_to_f3_90`: instruction structure correct (encoder tests verify bytes).
  - Negative tests: unknown traits/methods return `None`.

## Baselines

- **paideia-as-ir**: 411 lines (Mnemonic::Pause variant exists).
- **paideia-as-elaborator**:
  - stdlib_lowering module: new, ~50 lines.
  - emit_call.rs: +15 lines (resolver + early return).
  - lib.rs: +1 line (module declaration).
  - Pre-existing: 747 lines; 7 integration suites clean.
- **Release canaries**: clean.

## Related Issues

- **#973**: PauseOps trait definition (paideia-stdlib).
- **#1036**: This issue (elaborator lowering).
- **#975–977**: RefcountOps, FreelistOps, BitmapOps (Phase 16 M1, deferred).
- **Future**: PerCpuOps, MmioOps, BytesOps, ChecksumOps (v0.17+).

## References

- PA-r16-007-backtrack softarch design (GitHub discussion #1036).
- `design/infrastructure/stdlib-trait-categories.md` (trait taxonomy).
- `tests/build-emit/cow_fs_stub.pdx` (test fixture with PauseOps definition).
