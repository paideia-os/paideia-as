# add_one Fixture

## Purpose

The `add_one` fixture verifies that NASM and paideia-as produce identical instruction sequences for a simple increment function, forming the baseline for OS subsystem migration validation per OS-requirements §2.1 T2.

## What it computes

```c
uint64_t add_one(uint64_t x) {
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

This is the minimal form paideia-as currently emits (phase-1). The LEA with disp8 is a single instruction; RET is a one-byte opcode.

## Phase-1 Limitation

At milestone 1 (m1-013), paideia-as's emitter ignores the input `.pdx` file and always produces this canonical sequence. The module.pdx here is syntactically valid but semantically inert—it parses cleanly and exercises the build pipeline without driving per-node lowering (which arrives in m2/m5 when the IR walker wires up).

This is the **only fixture that passes at m1**. Future fixtures (e.g., `multiply`, `shift`, handler calls) will be added as the IR dispatcher lands and real instruction selection follows.

## References

- `design/toolchain/calling-convention.md` §3 (System V AMD64 ABI)
- `design/toolchain/abi.md` (shared ABI definition)
- `crates/paideia-as-emitter-elf/src/lower.rs::lower_add_one()`
