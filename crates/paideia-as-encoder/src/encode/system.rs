//! System-level and privileged encoders: I/O ports (IN/OUT), MSR (RDMSR/WRMSR), INT/IRET/SYSRET/SYSCALL, MOV CR/DR, descriptor-table loads/stores, REP STOS/MOVS, FAR JMP, RDTSC, INVLPG, INVPCID, and CALL indirect variants.

use super::types::*;
use paideia_as_ir::instruction::IntWidth;

/// Encode I/O port read instruction: `in al/ax/eax, dx`.
///
/// The SDM fixes the register as al (width=1), ax (width=2), or eax (width=4).
/// The port address register is always DX (implicit in the encoding).
///
/// # Instructions
/// - `in al, dx`: `EC` (1 byte)
/// - `in ax, dx`: `66 ED` (2 bytes, with operand-size prefix)
/// - `in eax, dx`: `ED` (1 byte)
///
/// # Arguments
/// - `buf`: code buffer to append instruction to
/// - `width`: operand width (1 for al, 2 for ax, 4 for eax)
pub fn encode_in_dx(buf: &mut CodeBuffer, width: u8) {
    match width {
        1 => {
            // in al, dx: EC
            buf.bytes.push(0xEC);
        }
        2 => {
            // in ax, dx: 66 ED (operand-size prefix)
            buf.bytes.push(0x66);
            buf.bytes.push(0xED);
        }
        4 => {
            // in eax, dx: ED
            buf.bytes.push(0xED);
        }
        _ => {
            // Unreachable for valid widths
        }
    }
}

/// Encode I/O port write instruction: `out dx, al/ax/eax`.
///
/// The SDM fixes the register as al (width=1), ax (width=2), or eax (width=4).
/// The port address register is always DX (implicit in the encoding).
///
/// # Instructions
/// - `out dx, al`: `EE` (1 byte)
/// - `out dx, ax`: `66 EF` (2 bytes, with operand-size prefix)
/// - `out dx, eax`: `EF` (1 byte)
///
/// # Arguments
/// - `buf`: code buffer to append instruction to
/// - `width`: operand width (1 for al, 2 for ax, 4 for eax)
pub fn encode_out_dx(buf: &mut CodeBuffer, width: u8) {
    match width {
        1 => {
            // out dx, al: EE
            buf.bytes.push(0xEE);
        }
        2 => {
            // out dx, ax: 66 EF (operand-size prefix)
            buf.bytes.push(0x66);
            buf.bytes.push(0xEF);
        }
        4 => {
            // out dx, eax: EF
            buf.bytes.push(0xEF);
        }
        _ => {
            // Unreachable for valid widths
        }
    }
}

/// Encode zero-operand control and system instructions.
///
/// # Instructions
/// - `CLI` (Clear Interrupt Flag): `FA` (1 byte)
/// - `STI` (Set Interrupt Flag): `FB` (1 byte)
/// - `HLT` (Halt): `F4` (1 byte)
/// - `NOP` (No Operation): `90` (1 byte)
/// - `SWAPGS` (Swap GS Base): `0F 01 F8` (3 bytes)
/// - `CPUID` (CPU Identification): `0F A2` (2 bytes)
///
/// # Arguments
/// - `buf`: code buffer to append instruction to
/// - `mnem_byte`: the zero-operand instruction type code
///   - 0x90 for NOP
///   - 0xF4 for HLT
///   - 0xFA for CLI
///   - 0xFB for STI
///   - 0x81 (sentinel) for SWAPGS
///   - 0x82 (sentinel) for CPUID
///   - 0x83 (sentinel) for UD2
///   - 0x86 (sentinel) for ENDBR64
///   - 0x87 (sentinel) for ENDBR32
pub fn encode_zero_operand(buf: &mut CodeBuffer, mnem_byte: u8) {
    match mnem_byte {
        0x90 => {
            // NOP: 90
            buf.bytes.push(0x90);
        }
        0xF4 => {
            // HLT: F4
            buf.bytes.push(0xF4);
        }
        0xFA => {
            // CLI: FA
            buf.bytes.push(0xFA);
        }
        0xFB => {
            // STI: FB
            buf.bytes.push(0xFB);
        }
        0x81 => {
            // SWAPGS: 0F 01 F8 (special encoding; sentinel)
            buf.bytes.push(0x0F);
            buf.bytes.push(0x01);
            buf.bytes.push(0xF8);
        }
        0x82 => {
            // CPUID: 0F A2 (two-byte encoding; sentinel)
            buf.bytes.push(0x0F);
            buf.bytes.push(0xA2);
        }
        0x83 => {
            // UD2: 0F 0B (two-byte encoding; sentinel)
            buf.bytes.push(0x0F);
            buf.bytes.push(0x0B);
        }
        0x84 => {
            // CLD: FC (clear direction flag)
            buf.bytes.push(0xFC);
        }
        0x85 => {
            // STD: FD (set direction flag)
            buf.bytes.push(0xFD);
        }
        0x86 => {
            // ENDBR64: F3 0F 1E FA (Intel CET end branch 64-bit; sentinel)
            buf.bytes.push(0xF3);
            buf.bytes.push(0x0F);
            buf.bytes.push(0x1E);
            buf.bytes.push(0xFA);
        }
        0x87 => {
            // ENDBR32: F3 0F 1E FB (Intel CET end branch 32-bit; sentinel)
            buf.bytes.push(0xF3);
            buf.bytes.push(0x0F);
            buf.bytes.push(0x1E);
            buf.bytes.push(0xFB);
        }
        _ => {
            // Unreachable for valid mnemonics
        }
    }
}

/// Encode MSR write instruction: `wrmsr` (no operands).
///
/// Model-Specific Register Write: writes the value in EDX:EAX to the MSR
/// specified by the MSR index in ECX. This is a privileged instruction.
///
/// # Instructions
/// - `wrmsr`: `0F 30` (2 bytes)
///
/// # Arguments
/// - `buf`: code buffer to append instruction to
pub fn encode_wrmsr(buf: &mut CodeBuffer) {
    buf.bytes.push(0x0F);
    buf.bytes.push(0x30);
}

/// Encode MSR read instruction: `rdmsr` (no operands).
///
/// Model-Specific Register Read: reads the MSR specified by the MSR index
/// in ECX into EDX:EAX. This is a privileged instruction.
///
/// # Instructions
/// - `rdmsr`: `0F 32` (2 bytes)
///
/// # Arguments
/// - `buf`: code buffer to append instruction to
pub fn encode_rdmsr(buf: &mut CodeBuffer) {
    buf.bytes.push(0x0F);
    buf.bytes.push(0x32);
}

/// Encode software interrupt instruction: `int imm8`.
///
/// Generates a software interrupt with the specified interrupt number.
/// The interrupt number is encoded as an 8-bit immediate value.
///
/// # Instructions
/// - `int N`: `CD <imm8>` (2 bytes)
///
/// # Arguments
/// - `buf`: code buffer to append instruction to
/// - `imm`: interrupt number (must fit in u8)
pub fn encode_int_imm8(buf: &mut CodeBuffer, imm: u8) {
    buf.bytes.push(0xCD);
    buf.bytes.push(imm);
}

/// Encode interrupt return (32-bit): `iret`.
///
/// Returns from an interrupt handler using the stack-based interrupt frame.
/// In 32-bit mode, pops EIP, CS, and EFLAGS from the stack.
///
/// # Instructions
/// - `iret`: `CF` (1 byte)
///
/// # Arguments
/// - `buf`: code buffer to append instruction to
pub fn encode_iret(buf: &mut CodeBuffer) {
    buf.bytes.push(0xCF);
}

/// Encode interrupt return (64-bit): `iretq`.
///
/// Returns from an interrupt handler using the stack-based interrupt frame.
/// In 64-bit mode, pops RIP, CS, and RFLAGS from the stack.
/// Requires REX.W prefix to distinguish from 32-bit `iret`.
///
/// # Instructions
/// - `iretq`: `48 CF` (2 bytes, REX.W prefix)
///
/// # Arguments
/// - `buf`: code buffer to append instruction to
pub fn encode_iretq(buf: &mut CodeBuffer) {
    buf.bytes.push(0x48); // REX.W
    buf.bytes.push(0xCF);
}

/// Encode system return from fast syscall: `sysret`.
///
/// Returns from a fast system call made via `syscall` instruction.
/// In 64-bit mode, loads RIP and CS from MSR_SYSRET_CS, and RFLAGS from R11.
/// Operates in ring 3 only.
///
/// # Instructions
/// - `sysret`: `48 0F 07` (3 bytes, REX.W prefix + two-byte opcode)
///
/// # Arguments
/// - `buf`: code buffer to append instruction to
pub fn encode_sysret(buf: &mut CodeBuffer) {
    buf.bytes.push(0x48); // REX.W
    buf.bytes.push(0x0F);
    buf.bytes.push(0x07);
}

/// Encode system call instruction: `syscall` (no operands).
///
/// System call: triggers a system call to the kernel. The syscall instruction
/// loads RCX with the current RIP and loads RIP from the IA32_LSTAR MSR.
/// This is the x86_64-specific fast syscall mechanism.
///
/// # Instructions
/// - `syscall`: `0F 05` (2 bytes)
///
/// # Arguments
/// - `buf`: code buffer to append instruction to
pub fn encode_syscall(buf: &mut CodeBuffer) {
    buf.bytes.push(0x0F);
    buf.bytes.push(0x05);
}

/// Encode control register MOV instruction: `mov cr_idx, gpr` or `mov gpr, cr_idx`.
///
/// Both forms use the two-byte opcode 0F 22 (write to CR) or 0F 20 (read from CR).
/// REX.R is required if cr_idx >= 8 to extend the reg field (for CR8).
/// REX.B is required if gpr_idx >= 8 to extend the r/m field (for r8-r15).
///
/// # Instructions
/// - `mov cr0, rax`: `0F 22 C0` (3 bytes)
/// - `mov cr3, rax`: `0F 22 D8` (3 bytes, cr_idx=3 in reg field)
/// - `mov cr4, rax`: `0F 22 E0` (3 bytes, cr_idx=4 in reg field)
/// - `mov cr8, rax`: `44 0F 22 C0` (4 bytes, REX.R=1 for extended cr_idx)
/// - `mov cr3, r14`: `41 0F 22 DE` (4 bytes, REX.B=1 for r14 in r/m field)
/// - `mov cr0, r8`: `41 0F 22 C0` (4 bytes, REX.B=1 for r8)
/// - `mov rax, cr0`: `0F 20 C0` (3 bytes)
/// - `mov rax, cr3`: `0F 20 D8` (3 bytes)
/// - `mov rax, cr4`: `0F 20 E0` (3 bytes)
/// - `mov rax, cr8`: `44 0F 20 C0` (4 bytes, REX.R=1)
/// - `mov r14, cr3`: `41 0F 20 DE` (4 bytes, REX.B=1 for r14 in r/m field)
///
/// # Arguments
/// - `buf`: code buffer to append instruction to
/// - `write`: true for `mov cr_idx, gpr` (write to CR); false for `mov gpr, cr_idx` (read from CR)
/// - `cr_idx`: control register index (0-4 or 8; phase-5 supports CR0..CR4 + CR8 only)
/// - `gpr_idx`: general-purpose register index (0-15)
pub fn encode_mov_cr(buf: &mut CodeBuffer, write: bool, cr_idx: u8, gpr_idx: u8) {
    // Compute REX prefix: REX.R for cr_idx >= 8, REX.B for gpr_idx >= 8
    let mut rex = 0;
    if cr_idx >= 8 {
        rex |= 0x04; // REX.R
    }
    if gpr_idx >= 8 {
        rex |= 0x01; // REX.B
    }
    if rex != 0 {
        buf.bytes.push(0x40 | rex);
    }

    // Emit two-byte opcode
    buf.bytes.push(0x0F);
    buf.bytes.push(if write { 0x22 } else { 0x20 });

    // Emit ModR/M byte: mod=11, reg=cr_idx & 7, r/m=gpr_idx & 7
    let modrm = 0xC0 | ((cr_idx & 7) << 3) | (gpr_idx & 7);
    buf.bytes.push(modrm);
}

/// Encode MOV to/from debug register (0F 23 /r for write, 0F 21 /r for read).
///
/// # Arguments
/// - `write`: true for `mov dr_idx, gpr` (write to DR); false for `mov gpr, dr_idx` (read from DR)
/// - `dr_idx`: debug register index (0-7; phase-5 supports DR0..DR7 only; no aliasing logic)
/// - `gpr_idx`: general-purpose register index (0-15)
pub fn encode_mov_dr(buf: &mut CodeBuffer, write: bool, dr_idx: u8, gpr_idx: u8) {
    // No REX prefix needed; DR0..DR7 are directly encoded in ModR/M.reg (bits [5:3])
    // DR8+ do not exist in x86_64.

    // Emit two-byte opcode
    buf.bytes.push(0x0F);
    buf.bytes.push(if write { 0x23 } else { 0x21 });

    // Emit ModR/M byte: mod=11, reg=dr_idx & 7, r/m=gpr_idx & 7
    let modrm = 0xC0 | ((dr_idx & 7) << 3) | (gpr_idx & 7);
    buf.bytes.push(modrm);
}

/// Encode descriptor-table load instructions: `lgdt [base + disp]` or `lidt [base + disp]`.
///
/// Both lgdt and lidt follow the same encoding pattern:
/// - Opcode: 0F 01 /2 (lgdt) or 0F 01 /3 (lidt)
/// - Operand: memory address [base + disp]
///
/// # Arguments
/// - `buf`: code buffer to append instructions to
/// - `base_reg`: base register ID (0-15 for GPRs)
/// - `disp`: displacement from base (-2^31..2^31-1)
/// - `reg_digit`: 2 for lgdt, 3 for lidt (the /digit field in ModR/M.reg)
pub fn encode_descriptor_table_load(
    buf: &mut CodeBuffer,
    base_reg: Reg64,
    disp: i32,
    reg_digit: u8,
) {
    let base_id = base_reg as u8;

    // Emit two-byte opcode (no REX prefix needed for this instruction)
    buf.bytes.push(0x0F);
    buf.bytes.push(0x01);

    // Encode displacement and ModR/M byte
    if disp == 0 {
        // Use mod=00, no displacement
        let modrm = ((reg_digit & 7) << 3) | (base_id & 7);
        buf.bytes.push(modrm);
    } else if (-128..=127).contains(&disp) {
        // Use mod=01, disp8
        let modrm = 0x40 | ((reg_digit & 7) << 3) | (base_id & 7);
        buf.bytes.push(modrm);
        buf.bytes.push(disp as u8);
    } else {
        // Use mod=10, disp32
        let modrm = 0x80 | ((reg_digit & 7) << 3) | (base_id & 7);
        buf.bytes.push(modrm);
        buf.bytes.extend(disp.to_le_bytes());
    }
}

/// Encode repeat store quadword instruction: `rep stosq` (no operands).
///
/// Stores RAX to memory at [RDI], then decrements RCX and repeats until RCX is zero.
/// Used primarily for .bss section zeroing with RAX=0, RCX=size in quadwords, RDI=base.
///
/// # Instructions
/// - `rep stosq`: `F3 48 AB` (3 bytes: rep prefix, REX.W, stosq opcode)
///
/// # Arguments
/// - `buf`: code buffer to append instruction to
pub fn encode_rep_stosq(buf: &mut CodeBuffer) {
    buf.bytes.push(0xF3); // rep prefix
    buf.bytes.push(0x48); // REX.W for 64-bit operand
    buf.bytes.push(0xAB); // stosq opcode
}

/// Encode repeat store byte instruction: `rep stosb` (no operands).
///
/// Stores AL to memory at [RDI], then decrements RCX and repeats until RCX is zero.
/// Used primarily for `memset`: AL=fill byte, RCX=byte count, RDI=dest base.
///
/// # Instructions
/// - `rep stosb`: `F3 AA` (2 bytes: rep prefix, stosb opcode)
///
/// # Arguments
/// - `buf`: code buffer to append instruction to
pub fn encode_rep_stosb(buf: &mut CodeBuffer) {
    buf.bytes.push(0xF3); // rep prefix
    buf.bytes.push(0xAA); // stosb opcode
}

/// Encode repeat move quadword instruction: `rep movsq` (no operands).
///
/// Copies a quadword from [RSI] to [RDI], advances both, then decrements RCX
/// and repeats until RCX is zero. Used for qword-granular `memcpy`:
/// RSI=src base, RDI=dest base, RCX=qword count.
///
/// # Instructions
/// - `rep movsq`: `F3 48 A5` (3 bytes: rep prefix, REX.W, movsq opcode)
///
/// # Arguments
/// - `buf`: code buffer to append instruction to
pub fn encode_rep_movsq(buf: &mut CodeBuffer) {
    buf.bytes.push(0xF3); // rep prefix
    buf.bytes.push(0x48); // REX.W for 64-bit operand
    buf.bytes.push(0xA5); // movsq opcode
}

/// Encode `jmp far [mem]` with SIB or RIP-relative addressing.
///
/// Far jump to memory uses `FF /5` with REX.W prefix (`48`).
/// Instruction: `REX.W FF [ModR/M] [SIB] [disp]`
///
/// For SIB form `[base]` with disp:
/// - disp=0: ModR/M = 00_101_base_id (where base_id is the low 3 bits of base register)
/// - disp8: ModR/M = 01_101_base_id + disp8
/// - disp32: ModR/M = 10_101_base_id + disp32_le
///
/// For RIP-relative form `[rip + disp32]`:
/// - ModR/M = 00_101_101 (0x2D) + disp32_le
pub fn encode_far_jmp(buf: &mut CodeBuffer, base: Option<Reg64>, disp: i32) {
    let reg_field = 5u8; // /5 for far jmp

    if let Some(base_reg) = base {
        // SIB form: [base + disp]
        let base_id = base_reg as u8;
        let base_b = (base_id >> 3) != 0; // REX.B bit

        buf.bytes.push(rex(true, false, false, base_b)); // REX.W
        buf.bytes.push(0xFF);

        if disp == 0 {
            // mod=00: [base]
            buf.bytes.push((reg_field << 3) | (base_id & 7));
        } else if (-128..=127).contains(&disp) {
            // mod=01: [base + disp8]
            buf.bytes.push(0x40 | (reg_field << 3) | (base_id & 7));
            buf.bytes.push(disp as u8);
        } else {
            // mod=10: [base + disp32]
            buf.bytes.push(0x80 | (reg_field << 3) | (base_id & 7));
            buf.bytes.extend(disp.to_le_bytes());
        }
    } else {
        // RIP-relative form: [rip + disp32]
        // ModR/M.rm = 101 (5) signals RIP-relative in 64-bit mode with mod=00
        buf.bytes.push(rex_w());
        buf.bytes.push(0xFF);
        buf.bytes.push((reg_field << 3) | 5); // ModR/M with rm=5 for RIP-relative
        buf.bytes.extend(disp.to_le_bytes());
    }
}

/// Encode direct far jump with immediate operands: `ljmp selector:offset`.
///
/// This is the EA form (direct far jump):
/// - Opcode: EA
/// - Operand: imm32 (offset) + imm16 (selector)
///
/// Total: 7 bytes (1 opcode + 4 offset + 2 selector)
///
/// # Arguments
/// - `buf`: code buffer to append instruction to
/// - `offset`: 32-bit offset within the segment
/// - `selector`: 16-bit segment selector
pub fn encode_far_jmp_imm(buf: &mut CodeBuffer, offset: u32, selector: u16) {
    buf.bytes.push(0xEA); // opcode
    buf.bytes.extend(offset.to_le_bytes()); // imm32 offset
    buf.bytes.extend(selector.to_le_bytes()); // imm16 selector
}

/// Encode direct far jump with symbolic offset (for relocation): `ljmp selector:symbol`.
///
/// Emits the EA form with a placeholder for the 32-bit offset.
/// The caller must emit a relocation site for the 4-byte offset field.
///
/// # Arguments
/// - `buf`: code buffer to append instruction to
/// - `selector`: 16-bit segment selector
pub fn encode_far_jmp_imm_sym(buf: &mut CodeBuffer, selector: u16) {
    buf.bytes.push(0xEA); // opcode
    buf.bytes.extend([0u8; 4]); // placeholder for imm32 offset (relocation target)
    buf.bytes.extend(selector.to_le_bytes()); // imm16 selector
}

/// Encode read timestamp counter instruction: `rdtsc` (no operands).
///
/// Reads the processor's time-stamp counter into RDX:RAX. Returns the current
/// cycle count as a 64-bit value split between EDX (high 32 bits) and EAX (low 32 bits).
///
/// # Instructions
/// - `rdtsc`: `0F 31` (2 bytes)
///
/// # Arguments
/// - `buf`: code buffer to append instruction to
pub fn encode_rdtsc(buf: &mut CodeBuffer) {
    buf.bytes.push(0x0F);
    buf.bytes.push(0x31);
}

/// Encode invalidate TLB entry instruction: `invlpg [base + disp]`.
///
/// Invalidates a single entry in the TLB for the linear address specified.
/// The operand is a memory address [base + disp].
///
/// # Instructions
/// - `invlpg [base + disp]`: `0F 01 /7` (variable length depending on encoding)
///
/// # Arguments
/// - `buf`: code buffer to append instructions to
/// - `base_reg`: base register ID (0-15 for GPRs)
/// - `disp`: displacement from base (-2^31..2^31-1)
pub fn encode_invlpg(buf: &mut CodeBuffer, base_reg: Reg64, disp: i32) {
    let base_id = base_reg as u8;

    // Emit two-byte opcode
    buf.bytes.push(0x0F);
    buf.bytes.push(0x01);

    // Encode displacement and ModR/M byte using /7 digit in reg field
    let reg_digit = 7u8;
    if disp == 0 {
        // Use mod=00, no displacement
        let modrm = ((reg_digit & 7) << 3) | (base_id & 7);
        buf.bytes.push(modrm);
    } else if (-128..=127).contains(&disp) {
        // Use mod=01, disp8
        let modrm = 0x40 | ((reg_digit & 7) << 3) | (base_id & 7);
        buf.bytes.push(modrm);
        buf.bytes.push(disp as u8);
    } else {
        // Use mod=10, disp32
        let modrm = 0x80 | ((reg_digit & 7) << 3) | (base_id & 7);
        buf.bytes.push(modrm);
        buf.bytes.extend(disp.to_le_bytes());
    }
}

/// Encode `invpcid reg64, [base + disp]` — v0.21-009-followup (#1297).
///
/// Instruction: 66 [REX] 0F 38 82 /r
/// The register operand carries the INVPCID type (0/1/2/3 in low 2 bits
/// of r64); the m128 memory operand supplies a 128-bit descriptor
/// `[pcid_low12:64][linear_addr:64]` per Intel SDM Vol 2A INVPCID.
///
/// Prefix order: 66 (mandatory) precedes REX (Intel SDM Vol 2A §2.1.1).
/// REX.W is NOT set — the mandatory 66 selects the INVPCID opcode form,
/// and the register operand is 64-bit in 64-bit mode by default. REX is
/// only emitted when REX.R (reg extension) or REX.B (base extension) is
/// needed for r8–r15.
///
/// Examples:
/// - `invpcid rax, [rbx]`: `66 0F 38 82 03`
/// - `invpcid rax, [rsp]`: `66 0F 38 82 04 24` (SIB escape for RSP base)
/// - `invpcid r10, [rbx]`: `66 44 0F 38 82 13` (REX.R for r10)
/// - `invpcid rax, [r11]`: `66 41 0F 38 82 03` (REX.B for r11 base)
pub fn invpcid_reg_mem_base_disp(
    buf: &mut CodeBuffer,
    reg: Reg64,
    base: Reg64,
    disp: i32,
) {
    let reg_id = reg as u8;
    let base_id = base as u8;
    // Mandatory 66 prefix first.
    buf.bytes.push(0x66);
    // Optional REX for extended registers. REX.W = 0.
    let needs_r = (reg_id >> 3) != 0;
    let needs_b = (base_id >> 3) != 0;
    if needs_r || needs_b {
        buf.bytes.push(rex(false, needs_r, false, needs_b));
    }
    // Opcode: 0F 38 82
    buf.bytes.push(0x0F);
    buf.bytes.push(0x38);
    buf.bytes.push(0x82);
    // ModR/M + optional SIB + optional displacement via shared helper.
    emit_mem_base_disp(buf, reg_id & 7, base_id, disp);
}

// PA-R13-003: Indirect call (call reg / call [mem]) encoder helpers.

/// Encode `call reg64` — indirect call via register.
///
/// Instruction: [REX.B] FF D<reg>
/// Opcode: FF /2 (reg field = 010)
/// REX.B needed for r8–r15 (when reg >> 3 != 0).
///
/// Examples:
/// - `call rax`: `FF D0`
/// - `call r8`: `41 FF D0`
pub fn call_reg64(buf: &mut CodeBuffer, reg: Reg64) {
    let id = reg as u8;
    if id > 7 {
        buf.bytes.push(rex(false, false, false, true));
    }
    buf.bytes.push(0xFF);
    buf.bytes.push(0xC0 | (0b010 << 3) | (id & 7));
}

/// Encode `call [base + disp]` — indirect call via memory (base + displacement).
///
/// Instruction: [REX.B] FF 14 /2 [disp]
/// Opcode: FF /2 (reg field = 010)
/// ModR/M/disp handling via emit_mem_base_disp.
///
/// Examples:
/// - `call [rax]`: `FF 10`
/// - `call [rdi + 8]`: `FF 57 08`
pub fn call_mem_base_disp(buf: &mut CodeBuffer, base: Reg64, disp: i32) {
    let bid = base as u8;
    if (bid >> 3) != 0 {
        buf.bytes.push(rex(false, false, false, true));
    }
    buf.bytes.push(0xFF);
    emit_mem_base_disp(buf, 0b010, bid, disp);
}

/// Encode `call [base + index*scale + disp]` — indirect call via SIB memory addressing.
///
/// Instruction: [REX.X/B] FF 15 /2 SIB [disp]
/// Opcode: FF /2 (reg field = 010)
/// REX: X for index in r8–r15, B for base in r8–r15.
/// ModR/M/SIB/disp handling via emit_mem_sib_disp.
///
/// Examples:
/// - `call [r12 + rsi*8]`: `41 FF 14 F4`
/// - `call [r13 + rsi*8]`: `41 FF 54 F5 00`
pub fn call_mem_sib_disp(buf: &mut CodeBuffer, base: Reg64, index: Reg64, sc: u8, disp: i32) {
    let (bid, iid) = (base as u8, index as u8);
    if (bid | iid) >> 3 != 0 {
        buf.bytes.push(rex(false, false, (iid >> 3) != 0, (bid >> 3) != 0));
    }
    buf.bytes.push(0xFF);
    emit_mem_sib_disp(buf, 0b010, bid, iid, sc, disp);
}

/// Encode narrow-width mov with RIP-relative addressing.
///
/// `mov rN, [rip + disp32]` for W8/W16/W32/W64.
///
/// Instruction formats (per width):
/// - W8:  `8A /r 05 <rel32>` (no 0x66, no REX.W)
/// - W16: `66 8B /r 05 <rel32>`
/// - W32: `8B /r 05 <rel32>` (no REX.W)
/// - W64: `REX.W 8B /r 05 <rel32>` (existing path, but this handles it)
///
/// ModR/M: `0x05 | (dst << 3)` (mod=00, rm=101 = RIP-relative)
/// REX: REX.R for dst ∈ r8–r15; REX.W only for W64.
pub fn mov_reg_mem_rip_rel_sized(buf: &mut CodeBuffer, width: IntWidth, dst_id: u8, disp: i32) {
    if matches!(width, IntWidth::W16) {
        buf.bytes.push(0x66);
    }
    let rex_w = matches!(width, IntWidth::W64);
    let rex_r = (dst_id >> 3) != 0;
    if rex_w || rex_r {
        buf.bytes.push(rex(rex_w, rex_r, false, false));
    }
    let opcode = if matches!(width, IntWidth::W8) { 0x8A } else { 0x8B };
    buf.bytes.push(opcode);
    buf.bytes.push(0x05 | ((dst_id & 7) << 3));  // mod=00, rm=101 (RIP-relative)
    buf.bytes.extend(disp.to_le_bytes());
}

/// Encode `call [rip + disp32]` — indirect call via RIP-relative addressing.
///
/// Instruction: FF 15 <disp32>
/// No REX prefix (rip is implicit in 64-bit mode).
/// Disp32 is a signed 32-bit displacement.
/// Commonly used for PLT/GOT accesses with relocations.
///
/// Example:
/// - `call [rip + sym]`: `FF 15 00 00 00 00` (+ relocation at +2)
pub fn call_mem_rip_rel(buf: &mut CodeBuffer, disp: i32) {
    buf.bytes.push(0xFF);
    buf.bytes.push(0x15);
    buf.bytes.extend(disp.to_le_bytes());
}
