ENTRY(_start)

MEMORY
{
    kernel (rwx) : ORIGIN = 0x100000, LENGTH = 1M
}

SECTIONS
{
    . = 0x100000;

    .text : {
        *(.text*)
    } > kernel

    .data : {
        *(.data*)
    } > kernel

    .bss : {
        *(.bss*)
        *(COMMON)
    } > kernel
}
