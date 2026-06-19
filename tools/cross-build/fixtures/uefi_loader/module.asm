; uefi_loader.asm — Phase-2-m6-009 minimum UEFI loader fixture.
;
; Placeholder matching the m1-013 add_one pattern: computes x + 1 where x is
; passed in RDI, returns in RAX via System V AMD64 calling convention.
;
; This trivial fixture documents the cross-build interface for UEFI loaders.
; Real UEFI loaders would:
;   - take EFI_HANDLE in RCX
;   - take EFI_SYSTEM_TABLE* in RDX
;   - dereference ConOut
;   - call OutputString
;
; That functionality will follow when m6-010+ wires real codegen from the elaborator.
;
; Build with: nasm -f elf64 module.asm -o module.o

	section .text
	global efi_main

efi_main:
	lea rax, [rdi + 1]	; x + 1 (matching add_one pattern)
	ret
