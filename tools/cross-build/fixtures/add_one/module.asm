; uint64_t add_one(uint64_t x)
;
; Increment the input by 1. Callable via the System V AMD64 calling
; convention (see design/toolchain/calling-convention.md §3):
;
;   - x in RDI.
;   - return value in RAX.
;
; Computes: x + 1
;
; Build with: nasm -f elf64 module.asm -o module.o

        global  add_one

        section .text

add_one:
        lea     rax, [rdi + 1]
        ret
