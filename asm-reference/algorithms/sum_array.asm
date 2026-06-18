; uint64_t sum_array(const uint64_t *xs, size_t n)
;
; Sum the first n elements of a u64 array, callable via the System V
; AMD64 calling convention.
;
;   - xs in RDI.
;   - n in RSI.
;   - return value in RAX.
;
; Saturates to 2^64 - 1 on overflow (wrapping is normal for unsigned
; arithmetic on x86_64 add).
;
; Trashes: RCX.
;
; Build with: nasm -f elf64 sum_array.asm -o sum_array.o

        global  sum_array

        section .text

sum_array:
        xor     rax, rax                ; sum = 0
        xor     rcx, rcx                ; i = 0
.loop:
        cmp     rcx, rsi
        jae     .done
        add     rax, [rdi + rcx * 8]
        inc     rcx
        jmp     .loop
.done:
        ret
