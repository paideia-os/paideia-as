# asm-reference

x86_64 (Intel i7-class) assembly reference programs for PaideiaOS.

These are **hand-written NASM-syntax** programs that document the
runtime ABI and instruction-set patterns the `paideia-as` compiler
ultimately produces. They are useful as:

- A teaching reference for the System V AMD64 calling convention as
  used in PaideiaOS (see `design/toolchain/calling-convention.md` §3).
- A smoke target for QEMU — the bootloader subdirectory boots on
  `qemu-system-x86_64` and is exercised by a bash test script.
- A point of comparison when reviewing `paideia-as`'s emitter output
  (PR-53's encoder, PR-54's prologue, PR-55's lowering).

## Layout

```
asm-reference/
├── README.md                 (this file)
├── algorithms/
│   ├── factorial.asm         iterative factorial (n in RDI → RAX)
│   ├── fibonacci.asm         iterative fibonacci
│   ├── strlen.asm            NUL-terminated string length
│   ├── memcpy.asm            REP MOVSB bulk copy
│   └── sum_array.asm         indexed u64 array sum
├── bootloader/
│   ├── boot.asm              512-byte MBR — prints banner via COM1
│   └── README.md             how it works + QEMU command line
└── scripts/
    └── test_bootloader.sh    builds + boots the bootloader, asserts
                              the banner appears on serial output
```

## Building the algorithms

Each `.asm` under `algorithms/` is a position-independent callable
function following the System V AMD64 calling convention (arguments
in RDI/RSI/RDX/RCX/R8/R9, return value in RAX). Build any one with:

```sh
nasm -f elf64 algorithms/factorial.asm -o factorial.o
```

The objects expose a single `global` symbol matching the file's
basename. Link them against any C driver to exercise:

```sh
cat > main.c <<EOF
#include <stdio.h>
extern unsigned long factorial(unsigned long);
int main(void) { printf("%lu\n", factorial(10)); return 0; }
EOF
nasm -f elf64 algorithms/factorial.asm -o factorial.o
gcc main.c factorial.o -o main
./main   # prints 3628800
```

## Running the bootloader

```sh
bash asm-reference/scripts/test_bootloader.sh
```

The script needs `nasm` and `qemu-system-x86_64` on `PATH`. It exits
0 on success (banner observed on the virtual serial console) and
prints a diagnostic on failure. See `bootloader/README.md` for the
mechanism.

## Relation to `paideia-as`

These files are **not consumed by the `paideia-as` assembler** —
they're written directly in NASM syntax, which is intentionally a
different surface from `.pdx` source. They exist to document what
the target machine code *looks like* when written by a human, so a
reader of an `.pdx` source file can mentally connect the high-level
form to its emitted bytes.

A future PR may add `paideia-as` lowering tests that compare a
`.pdx` source against one of these reference assemblies as ground
truth (modulo register-allocation freedom).
