; size_t strlen(const char *s)
;
; Compute the length of a NUL-terminated byte string, callable via
; the System V AMD64 calling convention.
;
;   - s in RDI.
;   - return value in RAX (the number of bytes before the NUL).
;
; Naive byte-at-a-time scan. A vectorised version (using PCMPEQB on
; SSE2 registers or VPCMPEQB on AVX2) is faster for long strings but
; not the point of this reference fragment.
;
; Trashes: nothing besides RAX.
;
; Build with: nasm -f elf64 strlen.asm -o strlen.o

        global  strlen

        section .text

strlen:
        mov     rax, rdi                ; cursor = start
.loop:
        cmp     byte [rax], 0
        je      .done
        inc     rax
        jmp     .loop
.done:
        sub     rax, rdi                ; length = cursor - start
        ret
