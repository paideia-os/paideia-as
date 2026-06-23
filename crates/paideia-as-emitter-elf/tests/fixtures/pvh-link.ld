/* PVH-aware linker script for testing PA10-001

   This script demonstrates how to link a paideia-as object file
   with PVH note support for QEMU -kernel acceptance.
*/

ENTRY(_start)

PHDRS {
  text PT_LOAD FLAGS(5);  /* Read + Execute */
  note PT_NOTE FLAGS(4);  /* Read-only */
}

SECTIONS {
  . = 0x100000;

  .text : {
    *(.text*)
  } :text

  .rodata : {
    *(.rodata*)
  } :text

  .data : {
    *(.data*)
  } :text

  .note.Xen : {
    KEEP(*(.note.Xen))
  } :note
}
