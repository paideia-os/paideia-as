//! `ChecksumOps` stdlib-lowering recipes.
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

/// Dispatch a `ChecksumOps::<method_name>` call to its lowering recipe.
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
        "ipv4_checksum" => {
            // RFC 1071 one's-complement fold.
            // SysVRegs: RDI = hdr pointer, RSI = length (bytes).
            // Result low-16 of RAX.
            use paideia_as_ir::instruction::Cond;

            let build_inst = |mnemonic, ops: SmallVec<[Operand; 3]>| Instruction {
                mnemonic,
                operands: ops,
                encoding_hint: None,
                byte_offset_in_text: None,
                mode,
                emission_order: 0,
    };

            let make_ops = |ops: Vec<Operand>| -> SmallVec<[Operand; 3]> {
                let mut sv = SmallVec::new();
                for op in ops {
                    sv.push(op);
                }
                sv
            };

            Some(Ok(LoweringRecipe {
                instructions: vec![
                    // 0: xor rax, rax               ; sum = 0
                    build_inst(Mnemonic::Xor, make_ops(vec![Operand::Reg(abi::RAX), Operand::Reg(abi::RAX)])),
                    // 1: mov rcx, rsi               ; rcx = len
                    build_inst(Mnemonic::Mov, make_ops(vec![Operand::Reg(abi::RCX), Operand::Reg(abi::RSI)])),
                    // 2: shr rcx, 1                 ; rcx = word count
                    build_inst(Mnemonic::Shr, make_ops(vec![Operand::Reg(abi::RCX), Operand::Imm64(1)])),
                    // 3: test rcx, rcx              ; if zero, skip word loop
                    build_inst(Mnemonic::Test, make_ops(vec![Operand::Reg(abi::RCX), Operand::Reg(abi::RCX)])),
                    // 4: jz odd_check
                    build_inst(Mnemonic::Jcc(Cond::Zero), make_ops(vec![Operand::LabelRef { name: "odd_check".to_string(), addend: 0 }])),
                    // 5: loop_start: movzx rdx, word [rdi]
                    Instruction {
                        mnemonic: Mnemonic::Movzx,
                        operands: make_ops(vec![
                            Operand::Reg(abi::RDX),
                            Operand::MemSib { base: abi::RDI, index: None, scale: paideia_as_ir::instruction::Scale::X1, disp: 0 },
                        ]),
                        encoding_hint: Some(paideia_as_ir::instruction::EncodingHint { opcode: 0x0F, operand_size: 2 }),
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
    },
                    // 6: add rax, rdx
                    build_inst(Mnemonic::Add, make_ops(vec![Operand::Reg(abi::RAX), Operand::Reg(abi::RDX)])),
                    // 7: adc rax, 0                 ; carry propagation
                    build_inst(Mnemonic::Adc { width: IntWidth::W64 }, make_ops(vec![Operand::Reg(abi::RAX), Operand::Imm64(0)])),
                    // 8: add rdi, 2
                    build_inst(Mnemonic::Add, make_ops(vec![Operand::Reg(abi::RDI), Operand::Imm64(2)])),
                    // 9: dec rcx
                    build_inst(Mnemonic::Dec, make_ops(vec![Operand::Reg(abi::RCX)])),
                    // 10: jnz loop_start
                    build_inst(Mnemonic::Jcc(Cond::NonZero), make_ops(vec![Operand::LabelRef { name: "loop_start".to_string(), addend: 0 }])),
                    // 11: odd_check: test rsi, 1
                    build_inst(Mnemonic::Test, make_ops(vec![Operand::Reg(abi::RSI), Operand::Imm64(1)])),
                    // 12: jz fold
                    build_inst(Mnemonic::Jcc(Cond::Zero), make_ops(vec![Operand::LabelRef { name: "fold".to_string(), addend: 0 }])),
                    // 13: movzx rdx, byte [rdi]
                    Instruction {
                        mnemonic: Mnemonic::Movzx,
                        operands: make_ops(vec![
                            Operand::Reg(abi::RDX),
                            Operand::MemSib { base: abi::RDI, index: None, scale: paideia_as_ir::instruction::Scale::X1, disp: 0 },
                        ]),
                        encoding_hint: Some(paideia_as_ir::instruction::EncodingHint { opcode: 0x0F, operand_size: 1 }),
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
    },
                    // 14: add rax, rdx
                    build_inst(Mnemonic::Add, make_ops(vec![Operand::Reg(abi::RAX), Operand::Reg(abi::RDX)])),
                    // 15: adc rax, 0
                    build_inst(Mnemonic::Adc { width: IntWidth::W64 }, make_ops(vec![Operand::Reg(abi::RAX), Operand::Imm64(0)])),
                    // 16: fold: mov rdx, rax       ; first fold pass
                    build_inst(Mnemonic::Mov, make_ops(vec![Operand::Reg(abi::RDX), Operand::Reg(abi::RAX)])),
                    // 17: shr rdx, 16              ; extract high 16 bits
                    build_inst(Mnemonic::Shr, make_ops(vec![Operand::Reg(abi::RDX), Operand::Imm64(16)])),
                    // 18: and rax, 0xffff          ; mask sum to 16 bits
                    build_inst(Mnemonic::And, make_ops(vec![Operand::Reg(abi::RAX), Operand::Imm64(0xffff)])),
                    // 19: add rax, rdx             ; low16 + high16 (may exceed 0xffff)
                    build_inst(Mnemonic::Add, make_ops(vec![Operand::Reg(abi::RAX), Operand::Reg(abi::RDX)])),
                    // 20: mov rdx, rax             ; second fold pass
                    build_inst(Mnemonic::Mov, make_ops(vec![Operand::Reg(abi::RDX), Operand::Reg(abi::RAX)])),
                    // 21: shr rdx, 16              ; extract any carry
                    build_inst(Mnemonic::Shr, make_ops(vec![Operand::Reg(abi::RDX), Operand::Imm64(16)])),
                    // 22: and rax, 0xffff          ; mask low 16
                    build_inst(Mnemonic::And, make_ops(vec![Operand::Reg(abi::RAX), Operand::Imm64(0xffff)])),
                    // 23: add rax, rdx             ; fold again (guaranteed to be <= 0xffff now)
                    build_inst(Mnemonic::Add, make_ops(vec![Operand::Reg(abi::RAX), Operand::Reg(abi::RDX)])),
                    // 24: not rax                  ; one's complement
                    build_inst(Mnemonic::Not, make_ops(vec![Operand::Reg(abi::RAX)])),
                    // 25: and rax, 0xffff          ; mask to 16 bits (upper bits are 1s from not)
                    build_inst(Mnemonic::And, make_ops(vec![Operand::Reg(abi::RAX), Operand::Imm64(0xffff)])),
                ],
                arg_convention: ArgConvention::SysVRegs,
                labels: vec![
                    ("loop_start", 5),
                    ("odd_check", 11),
                    ("fold", 16),
                ],
            }))
        }
        _ => None,
    }
}
