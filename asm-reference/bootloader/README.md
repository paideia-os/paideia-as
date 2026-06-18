# Bootloader

The simplest possible MBR-style boot sector that QEMU's SeaBIOS can
load and execute on `qemu-system-x86_64`. 512 bytes, real mode, no
filesystem driver, no transition to long mode — just enough to
prove the toolchain end-to-end.

## What it does

1. Sets up segment registers (DS/ES/SS = 0) and a stack at the load
   address (`SP = 0x7C00`).
2. Initialises the COM1 UART at I/O port 0x3F8 for 9600-8N1.
3. Writes the banner `Hello, paideia-os boot!\r\n` to the serial
   console one byte at a time, polling the transmitter-holding-
   register-empty (THR) bit in LSR before each write.
4. Drains the THR so the host sees the entire message.
5. Signals QEMU to exit via the `isa-debug-exit` device (I/O port
   0xF4) — writing 0 produces exit code 1, which the test script
   recognises as success.
6. If `isa-debug-exit` isn't wired up, halts via `cli ; hlt`.

## Layout

```
0x7C00  start:          BIOS jumps here.
0x7C03  COM1 setup
0x7C3?  print loop
0x7C??  drain + isa-debug-exit
0x7C??  banner string
0x7DFE  0xAA55          MBR signature.
```

The whole sector is < 200 bytes; the remaining padding is zero.

## Why serial, not BIOS int 0x10?

QEMU's `-display none` is necessary for headless / CI runs. BIOS
int 0x10 (teletype output) writes to the VGA framebuffer, which is
hard to capture without a display. Serial output via `0x3F8` with
`-serial file:<path>` lets the test harness diff the host file
against the expected banner.

## Building manually

```sh
nasm -f bin boot.asm -o boot.bin
ls -l boot.bin       # 512 bytes
```

## Running on QEMU manually

```sh
qemu-system-x86_64 \
    -drive format=raw,file=boot.bin,if=floppy \
    -display none \
    -serial stdio \
    -device isa-debug-exit,iobase=0xf4,iosize=0x04 \
    -no-reboot
```

You should see the banner on stdout, then QEMU exits with status 1.

## Caveats

- This bootloader stays in 16-bit real mode. Transitioning to long
  mode (loading a GDT, enabling PAE + LME, paging tables, switching
  CS to a 64-bit code segment) takes another ~200 lines and is out
  of scope for "simplest possible".
- The serial port is configured for 9600 baud to keep the divisor
  calculation simple. QEMU runs the UART instantly, so the baud
  rate has no real effect on test wall-clock.
- The `isa-debug-exit` device is a QEMU-only extension. On real
  hardware the bootloader would just halt — there's no clean way
  to "exit" from boot context.

## Used by

`asm-reference/scripts/test_bootloader.sh` — the smoke test. Runs
in under a second on a modern machine.
