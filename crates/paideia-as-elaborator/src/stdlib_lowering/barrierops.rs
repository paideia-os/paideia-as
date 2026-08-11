//! `BarrierOps` stdlib-lowering recipes.
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

/// Dispatch a `BarrierOps::<method_name>` call to its lowering recipe.
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
        // PA-v0.21-004 (#1280): BarrierOps — mfence / sfence / lfence / pause.
        // Nullary intrinsics; each lowers to exactly one operand-less
        // mnemonic that the encoder already handles (mfence/sfence/lfence
        // covered by mem_fences.rs, pause by pause.rs). Args are Literal
        // convention because there are no args to marshal.
        "barrier_full" => {
            Some(Ok(LoweringRecipe {
                instructions: vec![Instruction {
                    mnemonic: Mnemonic::Mfence,
                    operands: SmallVec::new(),
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode,
                    emission_order: 0,
                }],
                arg_convention: ArgConvention::Literal,
                labels: vec![],
            }))
        }
        "barrier_store" => {
            Some(Ok(LoweringRecipe {
                instructions: vec![Instruction {
                    mnemonic: Mnemonic::Sfence,
                    operands: SmallVec::new(),
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode,
                    emission_order: 0,
                }],
                arg_convention: ArgConvention::Literal,
                labels: vec![],
            }))
        }
        "barrier_load" => {
            Some(Ok(LoweringRecipe {
                instructions: vec![Instruction {
                    mnemonic: Mnemonic::Lfence,
                    operands: SmallVec::new(),
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode,
                    emission_order: 0,
                }],
                arg_convention: ArgConvention::Literal,
                labels: vec![],
            }))
        }
        "cpu_pause" => {
            // Alias of PauseOps::spin_hint under the BarrierOps trait so
            // barrier discipline reads as one cohesive family at the
            // callsite. Byte-identical to spin_hint (F3 90). PauseOps
            // remains for the earlier PA-R16-007 consumers.
            Some(Ok(LoweringRecipe {
                instructions: vec![Instruction {
                    mnemonic: Mnemonic::Pause,
                    operands: SmallVec::new(),
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode,
                    emission_order: 0,
                }],
                arg_convention: ArgConvention::Literal,
                labels: vec![],
            }))
        }
        _ => None,
    }
}
