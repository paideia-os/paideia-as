/* paideia-as stage-0b entry-point — GAS syntax.
 *
 * This is the 1:1 counterpart of the NASM stage-0a entry-point at
 * tools/cross-build/fixtures/uefi_loader/module.asm. The dual stage-0
 * commitment (see design/toolchain/bootstrap.md §0) requires:
 *
 *   1. Both stage-0 sources produce byte-identical .text content (modulo
 *      the m10-002 DDC allowlist).
 *   2. Both can be cross-checked by the DDC harness.
 *
 * Computes: x + 1
 *
 * System V AMD64 calling convention:
 *   - x in RDI.
 *   - return value in RAX.
 *
 * Build with: as --64 entrypoint.s -o entrypoint.o
 * Compare with stage-0a:
 *   nasm -f elf64 tools/cross-build/fixtures/uefi_loader/module.asm -o stage-0a.o
 *   cmp <(objdump -d --no-show-raw-insn entrypoint.o | grep -v '^$\|file format') \
 *       <(objdump -d --no-show-raw-insn stage-0a.o | grep -v '^$\|file format')
 */

	.intel_syntax noprefix
	.text
	.global efi_main
efi_main:
	lea rax, [rdi + 1]   /* x + 1 (matching the add_one pattern) */
	ret
