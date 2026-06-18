; uint64_t factorial(uint64_t n)
;
; Iterative factorial, callable via the System V AMD64 calling
; convention (see design/toolchain/calling-convention.md §3):
;
;   - n in RDI.
;   - return value in RAX.
;
; n = 0 yields 1 (the empty product). Overflows silently for n >= 21
; because 21! exceeds 2^64.
;
; Trashes: RCX, RDX (mul writes RDX:RAX).
;
; Build with: nasm -f elf64 factorial.asm -o factorial.o

        global  factorial

        section .text

factorial:
        mov     rax, 1                  ; acc = 1
        mov     rcx, rdi                ; counter = n
        test    rcx, rcx
        jz      .done                   ; 0! = 1
.loop:
        mul     rcx                     ; RAX = RAX * RCX
        loop    .loop                   ; --RCX; jne .loop
.done:
        ret
