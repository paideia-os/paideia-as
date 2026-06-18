; void *memcpy(void *dst, const void *src, size_t n)
;
; Copy n bytes from src to dst (no overlap-safe guarantee — for
; that, use memmove). Callable via the System V AMD64 calling
; convention.
;
;   - dst in RDI.
;   - src in RSI.
;   - n in RDX.
;   - returns dst in RAX.
;
; The DF flag is assumed clear on entry (ABI guarantee).
;
; Trashes: RCX.
;
; Build with: nasm -f elf64 memcpy.asm -o memcpy.o

        global  memcpy

        section .text

memcpy:
        mov     rax, rdi                ; save dst for return
        mov     rcx, rdx                ; loop count for REP MOVSB
        rep movsb                       ; [RDI++] = [RSI++], RCX times
        ret
