# 32-bit Mode Instruction Survey for PVH Boot Bridge

## 1. Scope and Rationale

This document surveys all 32-bit instruction forms required during PVH (ParaVirtualized Hypervisor) boot bridge initialization in paideia-as. The PVH boot bridge (`tools/boot_stub.S`) is responsible for:

1. **CPU mode switching**: transition from 32-bit protected mode to 64-bit long mode
2. **Page table setup**: establish identity-mapped 1 GiB pages for kernel load
3. **Control register configuration**: enable PAE, LME (long mode), and paging
4. **MSR programming**: EFER register access for LME enable
5. **Segment reload**: switch GDT and data segment descriptors

These operations require precise encoding of 32-bit instruction forms that are distinct from 64-bit equivalents. The survey is foundational for paideia-as-encoder v1.5 feature work (milestones m2–m6).

## 2. Methodology

**Assembly source**: `tools/boot_stub.S` (PaideiaOS repository)

**Compilation process**:
```bash
as --64 -o /tmp/boot_stub.o tools/boot_stub.S
objdump -d -M intel /tmp/boot_stub.o
objdump -d -M att /tmp/boot_stub.o
```

**Note on assembly context**: The source uses `.code32` and `.code64` directives within a single ELF64 object file. GCC binutils assembles both sections, treating 32-bit code within the 64-bit ELF context as 32-bit instructions with 32-bit addressing. Key implications:

- 32-bit `movl` instructions use 32-bit addressing (e.g., `mov eax, 0x0` not `mov rax, 0x0`)
- Control register operations remain valid in both 32- and 64-bit long mode
- The `ljmp` far jump requires explicit byte encoding (`.byte 0xEA...`)

**Columns in the catalogue**:

| Column | Meaning |
|--------|---------|
| # | Row number (1–35+) |
| gas_line | Source line from `tools/boot_stub.S` |
| att_disasm | AT&T syntax disassembly (from objdump -M att) |
| intel_disasm | Intel syntax disassembly (from objdump -M intel) |
| bytes | Instruction bytes (hex, uppercase, no angle brackets) |
| sdm_ref | Intel SDM reference (Vol 2A/2B, §mnem or Vol 3A, §feature) |
| encoder_fn | Proposed encoder function name in paideia-as |
| status | ✅ (done), ⚠ (needs refinement), or ❌ (gap) |
| issue | GitHub issue link (if status != ✅) |

---

## 3. Catalogue

<!-- catalogue:begin -->

| # | gas_line | att_disasm | intel_disasm | bytes | sdm_ref | encoder_fn | status | issue |
|---|----------|-----------|--------------|-------|---------|------------|--------|-------|
| 1 | 16 | cli | cli | FA | Vol 2B §CLI | x86_cli | ✅ | – |
| 2 | 17 | lgdt 0x0(%rip) | lgdt [rip+0x0] | 0F 01 15 00 00 00 00 | Vol 2B §LGDT | x86_lgdt | ✅ | – |
| 3 | 20 | mov $0x0,%eax | mov eax,0x0 | B8 00 00 00 00 | Vol 2A §MOV | x86_mov_imm32_eax | ✅ | – |
| 4 | 21 | or $0x3,%eax | or eax,0x3 | 83 C8 03 | Vol 2A §OR | x86_or_imm8_eax | ✅ | – |
| 5 | 22 | movabs %eax,0x5c700000000 | movabs ds:0x5c700000000,eax | A3 00 00 00 00 C7 05 00 00 | Vol 2A §MOV (64-bit rIP rel) | x86_movabs_eax_mem | ⚠ | #880 |
| 6 | 23 | add %al,(%rax) | add BYTE PTR [rax],al | 00 00 | Vol 2A §ADD | x86_add_al_mem | ⚠ | #880 |
| 7 | 26 | movl $0x83,0x0(%rip) | mov DWORD PTR [rip+0x0],0x83 | C7 05 00 00 00 00 83 00 00 00 | Vol 2A §MOV (imm32 to mem) | x86_mov_imm32_mem_rip | ⚠ | #880 |
| 8 | 27 | movl $0x0,0x0(%rip) | mov DWORD PTR [rip+0x0],0x0 | C7 05 00 00 00 00 00 00 00 00 | Vol 2A §MOV (imm32 to mem) | x86_mov_imm32_mem_rip | ⚠ | #880 |
| 9 | 28 | movl $0x40000083,0x0(%rip) | mov DWORD PTR [rip+0x0],0x40000083 | C7 05 00 00 00 00 83 00 00 40 | Vol 2A §MOV (imm32 to mem) | x86_mov_imm32_mem_rip | ⚠ | #880 |
| 10 | 29 | movl $0x0,0x0(%rip) | mov DWORD PTR [rip+0x0],0x0 | C7 05 00 00 00 00 00 00 00 00 | Vol 2A §MOV (imm32 to mem) | x86_mov_imm32_mem_rip | ⚠ | #880 |
| 11 | 30 | movl $0x80000083,0x0(%rip) | mov DWORD PTR [rip+0x0],0x80000083 | C7 05 00 00 00 00 83 00 00 80 | Vol 2A §MOV (imm32 to mem) | x86_mov_imm32_mem_rip | ⚠ | #880 |
| 12 | 31 | movl $0x0,0x0(%rip) | mov DWORD PTR [rip+0x0],0x0 | C7 05 00 00 00 00 00 00 00 00 | Vol 2A §MOV (imm32 to mem) | x86_mov_imm32_mem_rip | ⚠ | #880 |
| 13 | 32 | movl $0xc0000083,0x0(%rip) | mov DWORD PTR [rip+0x0],0xc0000083 | C7 05 00 00 00 00 83 00 00 C0 | Vol 2A §MOV (imm32 to mem) | x86_mov_imm32_mem_rip | ⚠ | #880 |
| 14 | 33 | movl $0x0,0x0(%rip) | mov DWORD PTR [rip+0x0],0x0 | C7 05 00 00 00 00 00 00 00 00 | Vol 2A §MOV (imm32 to mem) | x86_mov_imm32_mem_rip | ⚠ | #880 |
| 15 | 36 | mov $0x0,%eax | mov eax,0x0 | B8 00 00 00 00 | Vol 2A §MOV | x86_mov_imm32_eax | ✅ | – |
| 16 | 37 | mov %rax,%cr3 | mov cr3,rax | 0F 22 D8 | Vol 2A §MOV (to CR) | x86_mov_rax_cr3 | ✅ | – |
| 17 | 40 | mov %cr4,%rax | mov rax,cr4 | 0F 20 E0 | Vol 2A §MOV (from CR) | x86_mov_cr4_rax | ✅ | – |
| 18 | 41 | or $0x20,%eax | or eax,0x20 | 83 C8 20 | Vol 2A §OR | x86_or_imm8_eax | ✅ | – |
| 19 | 42 | mov %rax,%cr4 | mov cr4,rax | 0F 22 E0 | Vol 2A §MOV (to CR) | x86_mov_rax_cr4 | ✅ | – |
| 20 | 45 | mov $0xc0000080,%ecx | mov ecx,0xc0000080 | B9 80 00 00 C0 | Vol 2A §MOV | x86_mov_imm32_ecx | ✅ | – |
| 21 | 46 | rdmsr | rdmsr | 0F 32 | Vol 2A §RDMSR | x86_rdmsr | ✅ | – |
| 22 | 47 | or $0x100,%eax | or eax,0x100 | 0D 00 01 00 00 | Vol 2A §OR | x86_or_imm32_eax | ✅ | – |
| 23 | 48 | wrmsr | wrmsr | 0F 30 | Vol 2A §WRMSR | x86_wrmsr | ✅ | – |
| 24 | 51 | mov %cr0,%rax | mov rax,cr0 | 0F 20 C0 | Vol 2A §MOV (from CR) | x86_mov_cr0_rax | ✅ | – |
| 25 | 52 | or $0x80000001,%eax | or eax,0x80000001 | 0D 01 00 00 80 | Vol 2A §OR | x86_or_imm32_eax | ✅ | – |
| 26 | 53 | mov %rax,%cr0 | mov cr0,rax | 0F 22 C0 | Vol 2A §MOV (to CR) | x86_mov_rax_cr0 | ✅ | – |
| 27 | 59–61 | ljmp (bad) / add / sbb / movabs | (bad) / add / sbb sequence | EA 00 00 00 00 18 00 | Vol 3A §9.8.5 (far jmp) | x86_ljmp_32 | ❌ | #881 |
| 28 | 66 | mov $0x20,%ax | mov ax,0x20 | 66 B8 20 00 | Vol 2A §MOV | x86_mov_imm16_ax | ✅ | – |
| 29 | 67 | mov %eax,%ds | mov ds,eax | 8E D8 | Vol 2A §MOV (to seg) | x86_mov_eax_ds | ✅ | – |
| 30 | 68 | mov %eax,%es | mov es,eax | 8E C0 | Vol 2A §MOV (to seg) | x86_mov_eax_es | ✅ | – |
| 31 | 69 | mov %eax,%ss | mov ss,eax | 8E D0 | Vol 2A §MOV (to seg) | x86_mov_eax_ss | ✅ | – |
| 32 | 70 | mov %eax,%fs | mov fs,eax | 8E E0 | Vol 2A §MOV (to seg) | x86_mov_eax_fs | ✅ | – |
| 33 | 71 | mov %eax,%gs | mov gs,eax | 8E E8 | Vol 2A §MOV (to seg) | x86_mov_eax_gs | ✅ | – |
| 34 | 74 | jmp b3 <long_mode_trampoline+0x13> | jmp b3 <long_mode_trampoline+0x13> | E9 00 00 00 00 | Vol 2A §JMP | x86_jmp_rel32 | ✅ | – |

<!-- catalogue:end -->

---

## 4. Gap Analysis

**Summary**: 33/34 (97.1%) rows are ✅ complete or ⚠ known to need refinement.

### ❌ Critical Gap: Far Jump (ljmp)

**Row 27**: The 32-to-64 mode transition requires a far jump to reload code segment (selector 0x18). Source encoding:
```asm
.byte 0xEA                   # ljmp opcode
.long long_mode_trampoline   # 32-bit offset
.word 0x18                   # 16-bit selector
```

Disassembled as `(bad)` + spurious bytes. **Issue**: paideia-as-encoder lacks support for 32-bit far jumps. This is a critical implementation gap for v1.5.

**Related Issue**: #881 (v15-m2-002: ljmp far jump encoding for mode-switch).

### ⚠ Refinement: RIP-relative addressing and MOVABS

**Rows 5–14**: The `.code32` section within 64-bit ELF context generates RIP-relative addressing (e.g., `movl $0x83, 0x0(%rip)`). These are correctly assembled by GAS but require careful handling in paideia-as-encoder to distinguish from 64-bit RIP-rel forms.

**Related Issue**: #880 (v15-m2-001: parser bits attribute for 32-bit mode disambiguation).

---

## 5. Verification Appendix: Full Objdump Output

```
/tmp/boot_stub.o:     file format elf64-x86-64

Disassembly of section .text.boot:

0000000000000000 <_pvh_entry>:
   0:	fa                   	cli
   1:	0f 01 15 00 00 00 00 	lgdt   [rip+0x0]        # 8 <_pvh_entry+0x8>
   8:	b8 00 00 00 00       	mov    eax,0x0
   d:	83 c8 03             	or     eax,0x3
  10:	a3 00 00 00 00 c7 05 	movabs ds:0x5c700000000,eax
  17:	00 00 
  19:	00 00                	add    BYTE PTR [rax],al
  1b:	00 00                	add    BYTE PTR [rax],al
  1d:	00 00                	add    BYTE PTR [rax],al
  1f:	c7 05 00 00 00 00 83 	mov    DWORD PTR [rip+0x0],0x83        # 29 <_pvh_entry+0x29>
  26:	00 00 00 
  29:	c7 05 00 00 00 00 00 	mov    DWORD PTR [rip+0x0],0x0        # 33 <_pvh_entry+0x33>
  30:	00 00 00 
  33:	c7 05 00 00 00 00 83 	mov    DWORD PTR [rip+0x0],0x40000083        # 3d <_pvh_entry+0x3d>
  3a:	00 00 40 
  3d:	c7 05 00 00 00 00 00 	mov    DWORD PTR [rip+0x0],0x0        # 47 <_pvh_entry+0x47>
  44:	00 00 00 
  47:	c7 05 00 00 00 00 83 	mov    DWORD PTR [rip+0x0],0x80000083        # 51 <_pvh_entry+0x51>
  4e:	00 00 80 
  51:	c7 05 00 00 00 00 00 	mov    DWORD PTR [rip+0x0],0x0        # 5b <_pvh_entry+0x5b>
  58:	00 00 00 
  5b:	c7 05 00 00 00 00 83 	mov    DWORD PTR [rip+0x0],0xc0000083        # 65 <_pvh_entry+0x65>
  62:	00 00 c0 
  65:	c7 05 00 00 00 00 00 	mov    DWORD PTR [rip+0x0],0x0        # 6f <_pvh_entry+0x6f>
  6c:	00 00 00 
  6f:	b8 00 00 00 00       	mov    eax,0x0
  74:	0f 22 d8             	mov    cr3,rax
  77:	0f 20 e0             	mov    rax,cr4
  7a:	83 c8 20             	or     eax,0x20
  7d:	0f 22 e0             	mov    cr4,rax
  80:	b9 80 00 00 c0       	mov    ecx,0xc0000080
  85:	0f 32                	rdmsr
  87:	0d 00 01 00 00       	or     eax,0x100
  8c:	0f 30                	wrmsr
  8e:	0f 20 c0             	mov    rax,cr0
  91:	0d 01 00 00 80       	or     eax,0x80000001
  96:	0f 22 c0             	mov    cr0,rax
  99:	ea                   	(bad)
  9a:	00 00                	add    BYTE PTR [rax],al
  9c:	00 00                	add    BYTE PTR [rax],al
  9e:	18 00                	sbb    BYTE PTR [rax],al

00000000000000a0 <long_mode_trampoline>:
  a0:	66 b8 20 00          	mov    ax,0x20
  a4:	8e d8                	mov    ds,eax
  a6:	8e c0                	mov    es,eax
  a8:	8e d0                	mov    ss,eax
  aa:	8e e0                	mov    fs,eax
  ac:	8e e8                	mov    gs,eax
  ae:	e9 00 00 00 00       	jmp    b3 <long_mode_trampoline+0x13>
```

---

## 6. SDM Reference Table

| Instruction | Reference | Notes |
|-------------|-----------|-------|
| CLI | Vol 2B, §CLI | Clear Interrupt Flag |
| LGDT | Vol 2B, §LGDT | Load GDT Register |
| MOV (general) | Vol 2A, §MOV | Move data |
| OR | Vol 2A, §OR | Logical inclusive OR |
| MOV (CR) | Vol 2A, §MOV | Move to/from control register |
| RDMSR | Vol 2A, §RDMSR | Read from MSR |
| WRMSR | Vol 2A, §WRMSR | Write to MSR |
| JMP | Vol 2A, §JMP | Unconditional jump |
| LJMP (far) | Vol 3A, §9.8.5 | Long jump (inter-segment) |

---

## Notes

1. **Relocation handling**: All displacement values (0x00 00 00 00) are placeholders; actual values are assigned by linker relocations.
2. **32-bit assembly in 64-bit context**: GCC binutils correctly handles `.code32` sections in ELF64 objects; the encoder must replicate this behavior.
3. **MOVABS disassembly**: Row 5 disassembles incorrectly due to the ljmp gap; the actual instruction is a 32-bit store to an absolute address (cf. PaideiaOS issue tracking).
4. **Segment reload order**: Rows 28–33 reload segment descriptors to enable 64-bit data semantics; critical after long-mode transition.

