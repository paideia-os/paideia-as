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

## Currently Supported

### PauseOps::spin_hint()

- **Trait**: `PauseOps` (hardware pause hint for spin-waits).
- **Method**: `spin_hint() -> ()` (no args, no return value).
- **Mnemonic**: `pause` (x86_64 opcode `F3 90`).
- **Instruction mode**: Propagates from context (`InstrMode::Mode64` or `Mode32`).

### PerCpuOps::percpu_inc() and percpu_add() (PA-r16-007-followup, #1056)

- **Trait**: `PerCpuOps` (per-CPU counter operations with GS-prefix).
- **Methods**:
  - `percpu_inc(counter_gs_offset: u64) -> ()` — increment counter in GS-addressable memory.
  - `percpu_add(counter_gs_offset: u64, val: u64) -> ()` — add immediate to counter in GS-addressable memory.
- **Mnemonics**:
  - `percpu_inc`: `lock inc qword [gs:offset]` (LockInc with MemSeg/Gs wrapping).
  - `percpu_add`: `lock add qword [gs:offset], imm` (LockAdd with MemSeg/Gs wrapping).
- **Operand handling**: Requires literal-extraction at compile time; `addr_val` must fit in `i32` range.

### MmioOps::mmio_read_u32() and mmio_write_u32() (PA-r16-007-followup, #1057)

- **Trait**: `MmioOps` (memory-mapped I/O operations).
- **Methods**:
  - `mmio_read_u32(addr: u64) -> u32` — read 32-bit value from absolute MMIO address.
  - `mmio_write_u32(addr: u64, val: u32) -> ()` — write 32-bit value to absolute MMIO address.
- **Mnemonics**:
  - `mmio_read_u32`: `mov eax, dword [addr]` (MovSized W32, flat memory, no segment override).
  - `mmio_write_u32`: `mov dword [addr], imm` (MovSized W32, flat memory, no segment override).
- **Operand handling**: Requires literal-extraction at compile time; both `addr` must fit in `i32` range.
- **TSO/Ordering**: Bare MOV is TSO-strong on WB memory; MMIO regions may require explicit fences (deferred analysis — see note below).

### BytesOps Typed Accessors (PA-r16-007-followup, #1063)

- **Trait**: `BytesOps` (typed buffer byte accessors with SysV arg marshalling).
- **Methods**:
  - Getters: `get_u8(buf, off) -> u8`, `get_u16_le(buf, off) -> u16`, `get_u16_be(buf, off) -> u16`, `get_u32_le(buf, off) -> u32`, `get_u32_be(buf, off) -> u32`, `get_u64_le(buf, off) -> u64`, `get_u64_be(buf, off) -> u64`.
  - Setters: `put_u8(buf, off, val) -> ()`, `put_u16_le(buf, off, val)`, `put_u16_be(buf, off, val)`, `put_u32_le(buf, off, val)`, `put_u32_be(buf, off, val)`, `put_u64_le(buf, off, val)`, `put_u64_be(buf, off, val)`.
- **Arg Convention**: `ArgConvention::SysVRegs` — args are pre-marshalled into registers before recipe splicing (unlike Literal recipes above).
  - arg0 (buf) → RDI
  - arg1 (off) → RSI  
  - arg2 (val) → RDX (setters only)
- **Getters**:
  - LE variants (u8, u16_le, u32_le, u64_le): `MovSized{width} [RAX], MemSib{RDI, Some(RSI), X1, 0}` (load into RAX, result in return register).
  - BE variants (u16_be, u32_be, u64_be): load + byte-swap (see Byte-Swap Primitives below).
- **Setters**:
  - LE variants (u8, u16_le, u32_le, u64_le): `Add RDI, RSI` (fold offset) + `MovSized{width} [RDI+0], RDX` (store).
  - BE variants (u16_be, u32_be, u64_be): byte-swap value + `Add RDI, RSI` + `MovSized{width} [RDI+0], RDX` (see Byte-Swap Primitives below).
- **Encoder Workaround**: `encode_mov_sized` does not currently support `[MemSib{Some(index)}, Reg]` writes (indexed store). For setter recipes, offset is folded by `Add RDI, RSI` into the base, reducing the address to `[RDI+0]` with no index.

### Byte-Swap Primitives

- **u32/u64 byte-swapping**: Uses dedicated `Bswap32` and `Bswap` mnemonics (x86_64 built-in instructions).
- **u16 byte-swapping** (PA-r16-007, #1065): Uses `Rol{W16} [reg, 8]` (16-bit rotate left by 8 bits).
  - Rationale: Rotating left by 8 bits swaps the two bytes of a 16-bit value. No dedicated Bswap16 instruction exists.
  - Recipes:
    - `get_u16_be`: `MovSized{W16} [RAX], MemSib` + `Rol{W16} [RAX, 8]`.
    - `put_u16_be`: `Rol{W16} [RDX, 8]` + `Add RDI, RSI` + `MovSized{W16} [RDI+0], RDX`.
  - Encoding: Rol{W16} emits 0x66 operand-size prefix + REX.B (if needed) + opcode C1/D1 + ModRM.

### ChecksumOps::ipv4_checksum() (PA-r16-007, #1067)

- **Trait**: `ChecksumOps` (network checksum algorithms).
- **Method**: `ipv4_checksum(hdr: &[u8], len: usize) -> u16` — compute RFC 1071 one's-complement fold checksum.
- **Arg Convention**: `ArgConvention::SysVRegs` — args pre-marshalled into registers.
  - arg0 (hdr pointer) → RDI
  - arg1 (length bytes) → RSI
  - Result in low-16 bits of RAX.
- **Recipe**: 21-instruction RFC 1071 fold implementation with three local labels:
  - **`loop_start` (inst 5)**: Main loop processes words (16-bit units).
    - Load 16-bit word: `movzx rdx, word [rdi]` (zero-extend to 64-bit).
    - Accumulate: `add rax, rdx` + `adc rax, 0` (carry propagation).
    - Advance pointer: `add rdi, 2`, decrement counter: `dec rcx`, loop back on `jnz loop_start`.
  - **`odd_check` (inst 11)**: Test if length is odd (remaining byte).
    - If odd: load and accumulate single byte: `movzx rdx, byte [rdi]` + `add rax, rdx` + `adc rax, 0`.
  - **`fold` (inst 16)**: Fold carry bits back into sum.
    - Extract high 16 bits: `mov rdx, rax; shr rdx, 16; add rax, rdx; adc rax, 0`.
    - One's-complement: `not rax` (result in low-16 bits of RAX).
- **Labels**: Demonstrates label/Jcc extension from #1066 and Adc-imm from #1069.
- **Encoding hints**: Movzx instructions include operand-size hints (0x0F opcode with width 2 for word, 1 for byte).

## Explicitly Deferred

### MmioOps Variants (u8, u16, u64)

- **Methods**: `mmio_read_u8`, `mmio_write_u8`, `mmio_read_u16`, `mmio_write_u16`, `mmio_read_u64`, `mmio_write_u64`.
- **Reason**: u32 variants (complete) cover immediate uses; u8/u16/u64 require additional width-specific recipe work.
- **Future**: each size variant needs its own mnemonic and encoding recipe (e.g., `mov al, byte [addr]` for u8).

### MmioOps Ordering Fences

- **Question**: TSO vs. MMIO device ordering. Bare MOV is TSO-strong on Write-Back (WB) memory, but memory-mapped device regions may require explicit fences.
- **Status**: Deferred pending softarch re-review of device-ordering requirements.
- **Path forward**: Wrap MMIO methods in optional fence prologue/epilogue (mfence/lfence/sfence) when the device region is marked non-WB.

### 1. PerCpuOps Extended Methods (future issue)

- **Methods**: `percpu_read`, `percpu_write`, `percpu_dec`.
- **Rationale**: `percpu_inc` and `percpu_add` cover typical counter uses; `read`/`write`/`dec` deferred to Phase 18 m3+.
- **Mnemonics**: `gs:` prefix + `mov`/`sub` recipes.

### 2. BytesOps Extended Methods (future issue)

- **Methods**: `memcpy`, internal `rep movsb`, `rep stosb`.
- **Blocker**: requires operand-source resolution (loading `rcx`, `rdi`, `rsi` from arguments).
- **Mnemonics**: `mov` (args→registers) + `rep movsb`/`rep stosb`.

### 3. ChecksumOps Extended Methods (future issue)

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

- **paideia-as-ir**: Mnemonic::{Pause, MovSized, LockInc, LockAdd, Add, Bswap, Bswap32, Xor, Shr, Test, Jcc, Movzx, Adc, Dec, Mov, Not} variants.
- **paideia-as-elaborator**:
  - stdlib_lowering module: ~650 lines (PauseOps + PerCpuOps + MmioOps + BytesOps + ChecksumOps recipes).
    - Unit tests: 24 tests (1 Pause, 3 PerCpuOps, 4 MmioOps, 12 BytesOps, 1 ChecksumOps, 3 negative/edge cases).
  - emit_call.rs: resolver + early return (unchanged from #1056, extended to support ArgConvention::SysVRegs).
  - lib.rs: module declaration (unchanged).
- **Integration tests**:
  - `stdlib_percpu_lowering.rs`: 9 test suites (PerCpuOps coverage).
  - `stdlib_mmio_lowering.rs`: 7 tests (MmioOps u32 read/write coverage).
  - `stdlib_bytes_lowering.rs`: 4 tests (BytesOps get/put coverage, SysVRegs path verification).
  - `stdlib_checksum_lowering.rs`: 6 tests (ChecksumOps::ipv4_checksum recipe shape, instruction sequence, labels, encoding hints, convention, negative cases).
  - Total: 14 integration suites, all passing.
- **Release canaries**: clean.

## Related Issues

- **#973**: PauseOps trait definition (paideia-stdlib).
- **#1036**: PerCpuOps lowering (elaborator) — percpu_inc, percpu_add recipes.
- **#1056**: percpu_inc/percpu_add roundtrip integration tests.
- **#1057**: MmioOps lowering (elaborator) — mmio_read_u32, mmio_write_u32 recipes.
- **#1062**: ArgConvention enum + SysVRegs convention for pre-marshalled args.
- **#1063**: BytesOps typed accessor lowering — 12 get/put recipes + SysVRegs integration.
- **#1065**: Rol{W16} primitive addition + BytesOps::get_u16_be, put_u16_be recipes (completed).
- **#1066**: Recipe labels and Jcc local-label references — label mangling framework (completed).
- **#1067**: ChecksumOps::ipv4_checksum recipe — RFC 1071 fold with label-based loops (completed).
- **#1069**: Adc with immediate (width-aware) — carry propagation for checksum fold.
- **#975–977**: RefcountOps, FreelistOps, BitmapOps (Phase 16 M1, deferred).
- **Future**: PerCpuOps extended (read/write/dec), MmioOps variants (u8/u16/u64), BytesOps extended (memcpy), ChecksumOps extended (crc32b/crc32d).

## References

- PA-r16-007-backtrack softarch design (GitHub discussion #1036).
- `design/infrastructure/stdlib-trait-categories.md` (trait taxonomy).
- `tests/build-emit/cow_fs_stub.pdx` (test fixture with PauseOps definition).
