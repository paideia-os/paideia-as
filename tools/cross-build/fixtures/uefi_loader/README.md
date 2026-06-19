# UEFI Loader Fixture

## Purpose

The `uefi_loader` fixture verifies that NASM and paideia-as produce identical instruction sequences for a minimal UEFI entry point, forming a second cross-build smoke test for the m6-009 phase-2 milestone.

## What it computes

A placeholder UEFI loader fixture matching the m1-013 `add_one` pattern (for phase-2 compatibility):

```c
uint64_t efi_main(uint64_t x) {
    return x + 1;
}
```

Invoked via the System V AMD64 calling convention: input in `RDI`, output in `RAX`.

## Encoding

Both build paths must produce exactly:
```asm
lea    rax,[rdi+0x1]
ret
```

This is the canonical `add_one` sequence. LEA with disp8 is a single instruction; RET is a one-byte opcode.

## Phase-2 Limitation

At milestone m6-009, paideia-as's emitter ignores the input `.pdx` file and always produces a hardcoded ELF64 binary with this canonical stub sequence. The module.pdx here is syntactically valid but semantically inert—it parses cleanly and exercises the build pipeline without driving per-node lowering (which arrives in m6-010+ when the IR walker wires up).

Real UEFI loaders at that point will:
- Take `EFI_HANDLE` in RCX
- Take `EFI_SYSTEM_TABLE*` in RDX
- Dereference `ConOut`
- Call `OutputString` to display a message
- Return `EFI_SUCCESS`

This fixture demonstrates parity for the return-0 pattern and documents the interface for future expansions.

## References

- `design/toolchain/calling-convention.md` §4 (Microsoft x64 ABI)
- `design/uefi/entry-point.md` (UEFI entry point specification)
