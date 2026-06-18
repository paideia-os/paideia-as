; uint64_t fibonacci(uint64_t n)
;
; Iterative Fibonacci: fib(0) = 0, fib(1) = 1, fib(n) = fib(n-1) + fib(n-2).
; Callable via the System V AMD64 calling convention.
;
;   - n in RDI.
;   - return value in RAX.
;
; The state (a, b) starts at (0, 1); each iteration applies
; (a, b) -> (b, a + b). After n iterations the answer is in a.
;
; Trashes: RCX, RDX.
;
; Build with: nasm -f elf64 fibonacci.asm -o fibonacci.o

        global  fibonacci

        section .text

fibonacci:
        mov     rax, 0                  ; a = fib(0)
        mov     rcx, 1                  ; b = fib(1)
        test    rdi, rdi
        jz      .done
.loop:
        mov     rdx, rax                ; tmp = a
        mov     rax, rcx                ; a = b
        add     rcx, rdx                ; b = tmp + b
        dec     rdi
        jnz     .loop
.done:
        ret
