//! `BitfieldOps` stdlib-lowering recipes.
//!
//! Extracted 2026-08-11 from `stdlib_lowering.rs` (God-file refactor).
//! Each stdlib trait family lives in its own submodule so unrelated
//! intrinsic families evolve independently.

#![allow(unused_imports)]

use paideia_as_ir::{
    IrArena, IrNodeId, SmallVec, abi,
    instruction::{InstrMode, Instruction, IntWidth, Mnemonic, Operand, SegPrefix},
};

use super::{ArgConvention, LoweringRecipe, StdlibLoweringError};

/// Dispatch a `BitfieldOps::<method_name>` call to its lowering recipe.
///
/// Returns `None` for unknown methods (caller falls through to normal
/// call emission).
pub(super) fn try_lower(
    method_name: &str,
    mode: InstrMode,
    arg_ids: &[IrNodeId],
    arena: &IrArena,
) -> Option<Result<LoweringRecipe, StdlibLoweringError>> {
    let _ = (arg_ids, arena);
    match method_name {
        // PA-v0.21-012 (#1288): BitfieldOps — pure shift-and-mask decoders.
        //
        // get_bits(word: u64, start: u32, width: u32) -> u64
        //   SysVRegs: RDI = word, RSI = start (low byte usable as CL), RDX = width.
        //   Return in RAX (SysV).
        //
        //   Recipe:
        //     mov rax, rdi        ; rax  = word
        //     mov rcx, rsi        ; cl   = start (only low 6 bits used by shr r64,cl)
        //     shr rax, cl         ; rax >>= start
        //     mov r9,  1
        //     mov rcx, rdx        ; cl   = width
        //     shl r9,  cl         ; r9   = 1 << width
        //     sub r9,  1          ; r9   = (1 << width) - 1  (mask)
        //     and rax, r9         ; rax  = ((word >> start) & mask)
        //
        //   Register discipline: RDI is caller-saved and dies at the recipe
        //   boundary (we consume it as the input `word`); RSI/RDX likewise.
        //   R9 is caller-saved scratch. RCX is the mandatory shift-count reg
        //   per SDM Vol 2 SHL/SHR; it is caller-saved and its inbound value
        //   (nothing — this is a leaf function invoked as a call) is dead.
        //
        //   Undefined-behavior contract: width == 64 is disallowed. Hardware
        //   masks the CL shift count to 6 bits, so `shl r9, 64` becomes
        //   `shl r9, 0` and the mask ends at 0, returning 0 for every bit
        //   read. The AC ("phase 1 read/write only, ACPI SDT length field
        //   width=32") stays inside the width <= 63 envelope; the trait
        //   doc calls this out.
        "get_bits" => {
            let mov_rax_rdi = {
                let mut ops = SmallVec::new();
                ops.push(Operand::Reg(abi::RAX));
                ops.push(Operand::Reg(abi::RDI));
                ops
            };
            let mov_rcx_rsi = {
                let mut ops = SmallVec::new();
                ops.push(Operand::Reg(abi::RCX));
                ops.push(Operand::Reg(abi::RSI));
                ops
            };
            let shr_rax_cl = {
                let mut ops = SmallVec::new();
                ops.push(Operand::Reg(abi::RAX));
                ops.push(Operand::Reg(abi::RCX));
                ops
            };
            let mov_r9_one = {
                let mut ops = SmallVec::new();
                ops.push(Operand::Reg(abi::R9));
                ops.push(Operand::Imm64(1));
                ops
            };
            let mov_rcx_rdx = {
                let mut ops = SmallVec::new();
                ops.push(Operand::Reg(abi::RCX));
                ops.push(Operand::Reg(abi::RDX));
                ops
            };
            let shl_r9_cl = {
                let mut ops = SmallVec::new();
                ops.push(Operand::Reg(abi::R9));
                ops.push(Operand::Reg(abi::RCX));
                ops
            };
            let sub_r9_one = {
                let mut ops = SmallVec::new();
                ops.push(Operand::Reg(abi::R9));
                ops.push(Operand::Imm64(1));
                ops
            };
            let and_rax_r9 = {
                let mut ops = SmallVec::new();
                ops.push(Operand::Reg(abi::RAX));
                ops.push(Operand::Reg(abi::R9));
                ops
            };
            Some(Ok(LoweringRecipe {
                instructions: vec![
                    Instruction { mnemonic: Mnemonic::Mov, operands: mov_rax_rdi, encoding_hint: None, byte_offset_in_text: None, mode, emission_order: 0 },
                    Instruction { mnemonic: Mnemonic::Mov, operands: mov_rcx_rsi, encoding_hint: None, byte_offset_in_text: None, mode, emission_order: 0 },
                    Instruction { mnemonic: Mnemonic::Shr, operands: shr_rax_cl,  encoding_hint: None, byte_offset_in_text: None, mode, emission_order: 0 },
                    Instruction { mnemonic: Mnemonic::Mov, operands: mov_r9_one,  encoding_hint: None, byte_offset_in_text: None, mode, emission_order: 0 },
                    Instruction { mnemonic: Mnemonic::Mov, operands: mov_rcx_rdx, encoding_hint: None, byte_offset_in_text: None, mode, emission_order: 0 },
                    Instruction { mnemonic: Mnemonic::Shl, operands: shl_r9_cl,   encoding_hint: None, byte_offset_in_text: None, mode, emission_order: 0 },
                    Instruction { mnemonic: Mnemonic::Sub, operands: sub_r9_one,  encoding_hint: None, byte_offset_in_text: None, mode, emission_order: 0 },
                    Instruction { mnemonic: Mnemonic::And, operands: and_rax_r9,  encoding_hint: None, byte_offset_in_text: None, mode, emission_order: 0 },
                ],
                arg_convention: ArgConvention::SysVRegs,
                labels: vec![],
                extern_target: None,
            }))
        }
        // set_bits(word: u64, start: u32, width: u32, val: u64) -> u64
        //   SysVRegs: RDI = word, RSI = start, RDX = width, RCX = val.
        //   Return in RAX (SysV).
        //
        //   RCX doubles as the shift-count register, so val must be spilled
        //   to a caller-saved scratch (R8) before either shift.
        //
        //   Recipe:
        //     mov r8,  rcx        ; r8   = val   (rescue before CX becomes shift count)
        //     mov r9,  1
        //     mov rcx, rdx        ; cl   = width
        //     shl r9,  cl         ; r9   = 1 << width
        //     sub r9,  1          ; r9   = raw_mask (low `width` bits set)
        //     and r8,  r9         ; r8   = val & raw_mask   (clamp payload)
        //     mov rcx, rsi        ; cl   = start
        //     shl r8,  cl         ; r8   = payload in place
        //     shl r9,  cl         ; r9   = mask in place
        //     not r9              ; r9   = ~mask
        //     and rdi, r9         ; rdi  = word with slot cleared
        //     or  rdi, r8         ; rdi |= payload
        //     mov rax, rdi        ; RAX  = return
        "set_bits" => {
            let mov_r8_rcx  = { let mut o = SmallVec::new(); o.push(Operand::Reg(abi::R8));  o.push(Operand::Reg(abi::RCX)); o };
            let mov_r9_one  = { let mut o = SmallVec::new(); o.push(Operand::Reg(abi::R9));  o.push(Operand::Imm64(1));       o };
            let mov_rcx_rdx = { let mut o = SmallVec::new(); o.push(Operand::Reg(abi::RCX)); o.push(Operand::Reg(abi::RDX));  o };
            let shl_r9_cl   = { let mut o = SmallVec::new(); o.push(Operand::Reg(abi::R9));  o.push(Operand::Reg(abi::RCX));  o };
            let sub_r9_one  = { let mut o = SmallVec::new(); o.push(Operand::Reg(abi::R9));  o.push(Operand::Imm64(1));       o };
            let and_r8_r9   = { let mut o = SmallVec::new(); o.push(Operand::Reg(abi::R8));  o.push(Operand::Reg(abi::R9));   o };
            let mov_rcx_rsi = { let mut o = SmallVec::new(); o.push(Operand::Reg(abi::RCX)); o.push(Operand::Reg(abi::RSI));  o };
            let shl_r8_cl   = { let mut o = SmallVec::new(); o.push(Operand::Reg(abi::R8));  o.push(Operand::Reg(abi::RCX));  o };
            let shl_r9_cl2  = { let mut o = SmallVec::new(); o.push(Operand::Reg(abi::R9));  o.push(Operand::Reg(abi::RCX));  o };
            let not_r9      = { let mut o = SmallVec::new(); o.push(Operand::Reg(abi::R9));                                    o };
            let and_rdi_r9  = { let mut o = SmallVec::new(); o.push(Operand::Reg(abi::RDI)); o.push(Operand::Reg(abi::R9));   o };
            let or_rdi_r8   = { let mut o = SmallVec::new(); o.push(Operand::Reg(abi::RDI)); o.push(Operand::Reg(abi::R8));   o };
            let mov_rax_rdi = { let mut o = SmallVec::new(); o.push(Operand::Reg(abi::RAX)); o.push(Operand::Reg(abi::RDI));  o };
            Some(Ok(LoweringRecipe {
                instructions: vec![
                    Instruction { mnemonic: Mnemonic::Mov, operands: mov_r8_rcx,  encoding_hint: None, byte_offset_in_text: None, mode, emission_order: 0 },
                    Instruction { mnemonic: Mnemonic::Mov, operands: mov_r9_one,  encoding_hint: None, byte_offset_in_text: None, mode, emission_order: 0 },
                    Instruction { mnemonic: Mnemonic::Mov, operands: mov_rcx_rdx, encoding_hint: None, byte_offset_in_text: None, mode, emission_order: 0 },
                    Instruction { mnemonic: Mnemonic::Shl, operands: shl_r9_cl,   encoding_hint: None, byte_offset_in_text: None, mode, emission_order: 0 },
                    Instruction { mnemonic: Mnemonic::Sub, operands: sub_r9_one,  encoding_hint: None, byte_offset_in_text: None, mode, emission_order: 0 },
                    Instruction { mnemonic: Mnemonic::And, operands: and_r8_r9,   encoding_hint: None, byte_offset_in_text: None, mode, emission_order: 0 },
                    Instruction { mnemonic: Mnemonic::Mov, operands: mov_rcx_rsi, encoding_hint: None, byte_offset_in_text: None, mode, emission_order: 0 },
                    Instruction { mnemonic: Mnemonic::Shl, operands: shl_r8_cl,   encoding_hint: None, byte_offset_in_text: None, mode, emission_order: 0 },
                    Instruction { mnemonic: Mnemonic::Shl, operands: shl_r9_cl2,  encoding_hint: None, byte_offset_in_text: None, mode, emission_order: 0 },
                    Instruction { mnemonic: Mnemonic::Not, operands: not_r9,      encoding_hint: None, byte_offset_in_text: None, mode, emission_order: 0 },
                    Instruction { mnemonic: Mnemonic::And, operands: and_rdi_r9,  encoding_hint: None, byte_offset_in_text: None, mode, emission_order: 0 },
                    Instruction { mnemonic: Mnemonic::Or,  operands: or_rdi_r8,   encoding_hint: None, byte_offset_in_text: None, mode, emission_order: 0 },
                    Instruction { mnemonic: Mnemonic::Mov, operands: mov_rax_rdi, encoding_hint: None, byte_offset_in_text: None, mode, emission_order: 0 },
                ],
                arg_convention: ArgConvention::SysVRegs,
                labels: vec![],
                extern_target: None,
            }))
        }
        _ => None,
    }
}
