/* Phase 6 m6-002: cap_smoke linker script (userspace ELF).
 *
 * Statically linked, no glibc. Uses Linux ABI syscall for exit.
 * Single segment: .text at 0x400000, .bss at 0x600000 (above 2 MiB
 * boundary to avoid kernel low-mem region).
 */

OUTPUT_FORMAT(elf64-x86-64)
OUTPUT_ARCH(i386:x86-64)
ENTRY(_start)

SECTIONS
{
    . = 0x400000;

    .text : ALIGN(4K) {
        *(.text._start)
        *(.text*)
    }

    .rodata : ALIGN(4K) {
        *(.rodata*)
    }

    .data : ALIGN(4K) {
        *(.data*)
    }

    . = 0x600000;

    .bss : ALIGN(4K) {
        *(COMMON)
        *(.bss*)
    }

    /DISCARD/ : { *(.comment) *(.note*) *(.eh_frame*) }
}
