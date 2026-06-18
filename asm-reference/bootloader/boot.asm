; Simplest-possible paideia-os MBR bootloader, for QEMU smoke-testing.
;
; This is a 512-byte boot sector that:
;
;   1. Sets up segment registers + a stack at 0x7C00.
;   2. Initialises COM1 (I/O port 0x3F8) at 9600-8N1.
;   3. Prints "Hello, paideia-os boot!\r\n" to the serial console.
;   4. Signals QEMU to exit cleanly via the isa-debug-exit device at
;      I/O port 0xF4.
;   5. If isa-debug-exit isn't wired up, halts the CPU forever.
;
; Loaded at 0x7C00 in 16-bit real mode by BIOS / QEMU's SeaBIOS.
;
; Build: nasm -f bin boot.asm -o boot.bin
; Run:   qemu-system-x86_64 -drive format=raw,file=boot.bin \
;          -display none -serial stdio \
;          -device isa-debug-exit,iobase=0xf4,iosize=0x04
;
; See `asm-reference/scripts/test_bootloader.sh` for the smoke test.

        BITS    16
        ORG     0x7C00

        ; ---- Segments + stack ----------------------------------------
start:
        cli
        xor     ax, ax
        mov     ds, ax
        mov     es, ax
        mov     ss, ax
        mov     sp, 0x7C00
        sti

        ; ---- COM1 (0x3F8) at 9600 baud, 8 data, no parity, 1 stop -----
        ;
        ; UART register layout (DLAB=0): RBR/THR=0, IER=1, IIR/FCR=2,
        ; LCR=3, MCR=4, LSR=5, MSR=6, SCR=7. With DLAB=1 set in LCR:
        ; DLL replaces RBR/THR (port 0) and DLM replaces IER (port 1).
        ; Divisor = 115200 / 9600 = 12.

        mov     dx, 0x3F8 + 1           ; IER
        xor     al, al
        out     dx, al                  ; disable all UART IRQs

        mov     dx, 0x3F8 + 3           ; LCR
        mov     al, 0x80
        out     dx, al                  ; DLAB = 1

        mov     dx, 0x3F8 + 0           ; DLL
        mov     al, 12
        out     dx, al
        mov     dx, 0x3F8 + 1           ; DLM
        xor     al, al
        out     dx, al                  ; divisor latch = 12

        mov     dx, 0x3F8 + 3           ; LCR
        mov     al, 0x03                ; 8N1, DLAB = 0
        out     dx, al

        mov     dx, 0x3F8 + 2           ; FCR
        mov     al, 0xC7                ; enable + clear FIFOs, 14-byte trig
        out     dx, al

        mov     dx, 0x3F8 + 4           ; MCR
        mov     al, 0x0B                ; DTR + RTS + OUT2
        out     dx, al

        ; ---- Print banner ---------------------------------------------
        mov     si, banner
.print:
        lodsb                           ; AL = [DS:SI]; SI++
        test    al, al
        jz      .done
        mov     bl, al                  ; stash byte
.wait_thr:
        mov     dx, 0x3F8 + 5           ; LSR
        in      al, dx
        test    al, 0x20                ; bit 5 = THR empty
        jz      .wait_thr
        mov     dx, 0x3F8 + 0           ; THR
        mov     al, bl
        out     dx, al
        jmp     .print

.done:
        ; Drain the THR so QEMU's serial file has the full message
        ; before we trigger the debug-exit.
.drain:
        mov     dx, 0x3F8 + 5
        in      al, dx
        test    al, 0x40                ; bit 6 = transmitter idle
        jz      .drain

        ; isa-debug-exit (port 0xF4): writing N causes QEMU to exit
        ; with exit code (N << 1) | 1. Write 0 -> exit code 1 = success
        ; in this test harness's convention.
        mov     dx, 0xF4
        xor     al, al
        out     dx, al

        ; Fallback: halt forever if isa-debug-exit isn't wired up.
.halt:
        cli
        hlt
        jmp     .halt

banner: db      "Hello, paideia-os boot!", 0x0D, 0x0A, 0

        ; ---- MBR padding + boot signature ----------------------------
        times   510 - ($ - $$) db 0
        dw      0xAA55
