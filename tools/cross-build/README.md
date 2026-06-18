# Cross-Build Smoke Test

## Purpose

Verifies ABI parity between NASM and paideia-as instruction streams during OS subsystem migration, per OS-requirements §2.1 T2 and `design/toolchain/abi.md`.

The cross-build infrastructure builds the same logical module twice—once via NASM, once via paideia-as—extracts the mnemonic sequences, and compares them against an expected baseline. This ensures that as modules migrate from hand-coded assembly to the custom assembler, their observable behavior (instruction sequence, operand structure, register allocation) remains identical to the source of truth.

## Diff Level: Instruction-Stream

Per decision S6 in m1-013, the cross-build compares at instruction-stream level: mnemonic + operands. Register-allocation freedom is allowed (if both paths produce equivalent but syntactically different instructions, that is future work), but the mnemonic and operand structure must match the expected baseline exactly.

Example:
```
Expected:  lea    rax,[rdi+0x1]
NASM:      lea    rax,[rdi+0x1]      ✓ matches
paideia-as: lea   rax,[rdi+0x1]      ✓ matches (whitespace normalized)
```

If NASM or paideia-as diverges, the cross-build script prints a structured diff showing expected vs. actual per build path.

## Phase-1 Reality

At m1-013, paideia-as's emitter is a stub: it ignores the input `.pdx` file and always emits the canonical `lower_add_one` body:
```asm
lea rax, [rdi+1]
ret
```

This means:
- **Only the `add_one` fixture passes at m1.**
- The `module.pdx` source is syntactically valid but semantically inert.
- The IR walker does not yet dispatch on node children to drive instruction selection.
- Per-node payloads and the full lowering pipeline arrive in m2/m5.

As m2/m5 ship, fixtures will grow to cover:
- Arithmetic operations (add, sub, mul, div)
- Memory operations (load, store, lea with various displacements)
- Control flow (conditional branches, jumps)
- Effect handler calls
- And so on.

Each new fixture will have corresponding NASM and `.pdx` sources, plus an `expected-mnemonics.txt` baseline.

## Structure

```
tools/cross-build/
├── README.md                            (this file)
├── fixtures/
│   ├── add_one/
│   │   ├── module.asm                   NASM source
│   │   ├── module.pdx                   paideia-as source
│   │   ├── module.expect-mnemonics.txt  expected instruction sequence
│   │   └── README.md                    fixture description
│   └── [future fixtures]
└── tools/
    ├── extract-mnemonics.sh             ELF .o → text of mnemonics
    └── cross-build.sh                   orchestration script
```

## Tools

### `extract-mnemonics.sh`

```bash
extract-mnemonics.sh <elf-object-file>
```

Runs `objdump -d -M intel` on the `.o`, strips address/encoding columns, emits one mnemonic+operands per line. Comments and blank lines are removed. Output is normalized to Intel syntax for stability.

Example:
```bash
$ extract-mnemonics.sh module.o
lea    rax,[rdi+0x1]
ret
```

Requires: `binutils` (objdump).

### `cross-build.sh`

```bash
cross-build.sh <fixture-directory>
```

Orchestration script that:
1. Assembles `module.asm` via NASM (`nasm -f elf64`).
2. Builds `module.pdx` via paideia-as (`cargo run -p paideia-as -- build --emit elf64`).
3. Extracts mnemonics from both via `extract-mnemonics.sh`.
4. Compares against `module.expect-mnemonics.txt` (ground truth).
5. Exits 0 if both match expected; nonzero with a structured diff on failure.

Example:
```bash
$ bash tools/cross-build/tools/cross-build.sh tools/cross-build/fixtures/add_one/
[cross-build] Fixture: add_one
[cross-build] ✓ PASS: add_one
[cross-build]   NASM mnemonics match expected
[cross-build]   paideia-as mnemonics match expected
```

On failure, prints expected + actual diffs per build path using `diff -u`.

## Adding a Fixture

1. Create `tools/cross-build/fixtures/<name>/`.
2. Write `module.asm` (NASM source) and `module.pdx` (paideia-as source) to compile to the same instruction sequence.
3. Assemble NASM: `nasm -f elf64 module.asm -o module.o`.
4. Extract expected mnemonics: `tools/cross-build/tools/extract-mnemonics.sh module.o > module.expect-mnemonics.txt`.
5. Verify manually that both sources produce identical mnemonics.
6. Write a `README.md` explaining what the module computes and why it's needed.
7. Update the CI workflow (`.github/workflows/cross-build.yml`) if not already auto-discovering fixtures.

Once the IR walker lands (m2/m5), the `.pdx` sources will carry real semantics.

## CI Integration

See `.github/workflows/cross-build.yml`. The workflow:
- Runs on every push and pull request.
- Sets up NASM + binutils.
- Builds paideia-as once (`cargo build --release -p paideia-as`).
- Loops over all fixtures in `tools/cross-build/fixtures/*/` running `cross-build.sh`.
- At m1: marked as `continue-on-error: true` (only `add_one` passes; others will be added in m2/m5).
- At m2+: flips to required once multiple fixtures land.

## References

- `design/toolchain/abi.md` — shared ABI specification
- `design/toolchain/calling-convention.md` — System V AMD64 ABI
- `design/toolchain/custom-assembler.md` — custom assembler design
- `crates/paideia-as-emitter-elf/src/lower.rs` — current lowering stub
- OS-requirements §2.1 T2 — subsystem migration verification
