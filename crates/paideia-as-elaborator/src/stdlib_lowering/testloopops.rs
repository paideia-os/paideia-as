//! `TestLoopOps` stdlib-lowering recipes.
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

/// Dispatch a `TestLoopOps::<method_name>` call to its lowering recipe.
///
/// Returns `None` for unknown methods (caller falls through to normal
/// call emission).
pub(super) fn try_lower(
    method_name: &str,
    mode: InstrMode,
    arg_ids: &[IrNodeId],
    arena: &IrArena,
) -> Option<Result<LoweringRecipe, StdlibLoweringError>> {
    let _ = (mode, arg_ids, arena);
    match method_name {
        // PA-r16-007 (#1066): Test recipe demonstrating loop pattern with local labels.
        // Recipe: mov rax, 3; loop_top: dec rax; jnz loop_top
        #[cfg(test)]
        "test_countdown" => {
            use paideia_as_ir::instruction::Cond;
            let mut mov_ops: SmallVec<[Operand; 3]> = SmallVec::new();
            mov_ops.push(Operand::Reg(abi::RAX));
            mov_ops.push(Operand::Imm64(3));

            let mut dec_ops: SmallVec<[Operand; 3]> = SmallVec::new();
            dec_ops.push(Operand::Reg(abi::RAX));

            let mut jcc_ops: SmallVec<[Operand; 3]> = SmallVec::new();
            jcc_ops.push(Operand::LabelRef {
                name: "loop_top".to_string(),
                addend: 0,
            });

            Some(Ok(LoweringRecipe {
                instructions: vec![
                    Instruction {
                        mnemonic: Mnemonic::Mov,
                        operands: mov_ops,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
    },
                    Instruction {
                        mnemonic: Mnemonic::Dec,
                        operands: dec_ops,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
    },
                    Instruction {
                        mnemonic: Mnemonic::Jcc(Cond::Ne),
                        operands: jcc_ops,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
    },
                ],
                arg_convention: ArgConvention::Literal,
                // loop_top label aliases instruction at index 1 (the Dec)
                labels: vec![("loop_top", 1)],
                extern_target: None,
            }))
        }
        _ => None,
    }
}
