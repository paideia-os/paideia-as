//! Stdlib trait method → mnemonic sequence lowering.
//!
//! PA-r16-007-backtrack (#1036): a hardcoded registry that maps
//! `(trait_name, method_name)` pairs to the IR instruction sequences
//! they should lower to. Consulted by emit_call before its normal SysV
//! call-marshalling.
//!
//! PA-r16-007-followup (#1056): PerCpuOps::percpu_inc / percpu_add lowering.
//! Extends signature to accept arg_ids and arena so recipes can extract
//! integer-literal arguments at compile time (required for absolute-displacement
//! encoding).
//!
//! Scope: PauseOps::spin_hint(), PerCpuOps::percpu_inc/percpu_add in v0.16.
//! Follow-up issues track MmioOps, BytesOps, ChecksumOps retrofits.

use paideia_as_ir::{SmallVec, IrArena, IrNodeId, instruction::{InstrMode, Instruction, Mnemonic, Operand, SegPrefix}};

/// Error returned by lower_stdlib_method when recipe matching succeeds
/// but argument extraction fails.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StdlibLoweringError {
    /// A required argument is not an integer literal.
    NonLiteralArg {
        /// 0-based index of the failing argument.
        arg_index: usize,
        /// Qualified name like "PerCpuOps::percpu_inc".
        method: &'static str,
    },
}

/// Look up the lowering recipe for `(trait_name, method_name)`.
/// Returns:
/// - `None` if the pair is not a known stdlib trait method, signalling
///   emit_call should fall through to normal call emission.
/// - `Some(Ok(recipe))` if the method matched and args were successfully
///   extracted as integer literals. Recipe is spliced in place of the call.
/// - `Some(Err(NonLiteralArg))` if the method matched but at least one arg
///   is not an integer literal. Caller should emit diagnostic and skip lowering.
///
/// The returned Vec<Instruction> (on Ok) is spliced in place of the call —
/// no arg-marshalling, no `call target`, no `ret`.
#[must_use]
pub fn lower_stdlib_method(
    trait_name: &str,
    method_name: &str,
    mode: InstrMode,
    arg_ids: &[IrNodeId],
    arena: &IrArena,
) -> Option<Result<Vec<Instruction>, StdlibLoweringError>> {
    match (trait_name, method_name) {
        ("PauseOps", "spin_hint") => {
            // PauseOps::spin_hint takes no arguments, always succeeds.
            Some(Ok(vec![Instruction {
                mnemonic: Mnemonic::Pause,
                operands: SmallVec::new(),
                encoding_hint: None,
                byte_offset_in_text: None,
                mode,
            }]))
        }
        ("PerCpuOps", "percpu_inc") => {
            // percpu_inc(counter_gs_offset: u64) → lock inc qword [gs:offset]
            if arg_ids.len() != 1 {
                return Some(Err(StdlibLoweringError::NonLiteralArg {
                    arg_index: 0,
                    method: "PerCpuOps::percpu_inc",
                }));
            }

            let disp_val = match arena.literal_values().get(arg_ids[0]) {
                Some(val) => val,
                None => {
                    return Some(Err(StdlibLoweringError::NonLiteralArg {
                        arg_index: 0,
                        method: "PerCpuOps::percpu_inc",
                    }));
                }
            };

            // Validate that disp fits in i32
            if disp_val < i32::MIN as i64 || disp_val > i32::MAX as i64 {
                return Some(Err(StdlibLoweringError::NonLiteralArg {
                    arg_index: 0,
                    method: "PerCpuOps::percpu_inc",
                }));
            }

            let mut operands = SmallVec::new();
            operands.push(Operand::MemSeg {
                seg: SegPrefix::Gs,
                inner: Box::new(Operand::MemDisp {
                    disp: disp_val as i32,
                }),
            });

            Some(Ok(vec![Instruction {
                mnemonic: Mnemonic::LockInc {
                    width: paideia_as_ir::instruction::IntWidth::W64,
                },
                operands,
                encoding_hint: None,
                byte_offset_in_text: None,
                mode,
            }]))
        }
        ("PerCpuOps", "percpu_add") => {
            // percpu_add(counter_gs_offset: u64, val: u64) → lock add qword [gs:offset], imm
            if arg_ids.len() != 2 {
                return Some(Err(StdlibLoweringError::NonLiteralArg {
                    arg_index: 0,
                    method: "PerCpuOps::percpu_add",
                }));
            }

            let disp_val = match arena.literal_values().get(arg_ids[0]) {
                Some(val) => val,
                None => {
                    return Some(Err(StdlibLoweringError::NonLiteralArg {
                        arg_index: 0,
                        method: "PerCpuOps::percpu_add",
                    }));
                }
            };

            let imm_val = match arena.literal_values().get(arg_ids[1]) {
                Some(val) => val,
                None => {
                    return Some(Err(StdlibLoweringError::NonLiteralArg {
                        arg_index: 1,
                        method: "PerCpuOps::percpu_add",
                    }));
                }
            };

            // Validate that disp fits in i32
            if disp_val < i32::MIN as i64 || disp_val > i32::MAX as i64 {
                return Some(Err(StdlibLoweringError::NonLiteralArg {
                    arg_index: 0,
                    method: "PerCpuOps::percpu_add",
                }));
            }

            let mut operands = SmallVec::new();
            operands.push(Operand::MemSeg {
                seg: SegPrefix::Gs,
                inner: Box::new(Operand::MemDisp {
                    disp: disp_val as i32,
                }),
            });
            operands.push(Operand::Imm64(imm_val));

            Some(Ok(vec![Instruction {
                mnemonic: Mnemonic::LockAdd {
                    width: paideia_as_ir::instruction::IntWidth::W64,
                },
                operands,
                encoding_hint: None,
                byte_offset_in_text: None,
                mode,
            }]))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_ir::{IrArena, IrNodeId};

    #[test]
    fn pause_ops_spin_hint_returns_pause_mnemonic() {
        let arena = IrArena::new();
        let insts = lower_stdlib_method("PauseOps", "spin_hint", InstrMode::Mode64, &[], &arena)
            .expect("pause recipe should exist")
            .expect("pause lowering should succeed");
        assert_eq!(insts.len(), 1);
        assert_eq!(insts[0].mnemonic, Mnemonic::Pause);
        assert!(insts[0].operands.is_empty());
    }

    #[test]
    fn unknown_trait_returns_none() {
        let arena = IrArena::new();
        assert!(lower_stdlib_method("UnknownTrait", "some_method", InstrMode::Mode64, &[], &arena).is_none());
    }

    #[test]
    fn known_trait_unknown_method_returns_none() {
        let arena = IrArena::new();
        assert!(lower_stdlib_method("PauseOps", "nonexistent", InstrMode::Mode64, &[], &arena).is_none());
    }

    #[test]
    fn percpu_inc_lowers_to_gs_lock_inc() {
        let mut arena = IrArena::new();
        let lit_id = IrNodeId::new(1).expect("valid node id");
        arena.literal_values_mut().insert(lit_id, 0x1000);

        let result = lower_stdlib_method(
            "PerCpuOps",
            "percpu_inc",
            InstrMode::Mode64,
            &[lit_id],
            &arena,
        )
        .expect("recipe should exist")
        .expect("lowering should succeed");

        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].mnemonic,
            Mnemonic::LockInc {
                width: paideia_as_ir::instruction::IntWidth::W64
            }
        );
        assert_eq!(result[0].operands.len(), 1);

        // Verify the operand is MemSeg { Gs, MemDisp { 0x1000 } }
        match &result[0].operands[0] {
            Operand::MemSeg { seg, inner } => {
                assert_eq!(*seg, SegPrefix::Gs);
                match inner.as_ref() {
                    Operand::MemDisp { disp } => {
                        assert_eq!(*disp, 0x1000);
                    }
                    _ => panic!("expected MemDisp inner operand"),
                }
            }
            _ => panic!("expected MemSeg operand"),
        }
    }

    #[test]
    fn percpu_add_lowers_to_gs_lock_add() {
        let mut arena = IrArena::new();
        let disp_id = IrNodeId::new(1).expect("valid node id");
        let val_id = IrNodeId::new(2).expect("valid node id");
        arena.literal_values_mut().insert(disp_id, 0x2000);
        arena.literal_values_mut().insert(val_id, 5);

        let result = lower_stdlib_method(
            "PerCpuOps",
            "percpu_add",
            InstrMode::Mode64,
            &[disp_id, val_id],
            &arena,
        )
        .expect("recipe should exist")
        .expect("lowering should succeed");

        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].mnemonic,
            Mnemonic::LockAdd {
                width: paideia_as_ir::instruction::IntWidth::W64
            }
        );
        assert_eq!(result[0].operands.len(), 2);

        // Verify the first operand is MemSeg { Gs, MemDisp { 0x2000 } }
        match &result[0].operands[0] {
            Operand::MemSeg { seg, inner } => {
                assert_eq!(*seg, SegPrefix::Gs);
                match inner.as_ref() {
                    Operand::MemDisp { disp } => {
                        assert_eq!(*disp, 0x2000);
                    }
                    _ => panic!("expected MemDisp inner operand"),
                }
            }
            _ => panic!("expected MemSeg operand"),
        }

        // Verify the second operand is Imm64(5)
        match &result[0].operands[1] {
            Operand::Imm64(val) => {
                assert_eq!(*val, 5);
            }
            _ => panic!("expected Imm64 operand"),
        }
    }

    #[test]
    fn percpu_inc_non_literal_returns_err() {
        let arena = IrArena::new();
        // Pass an arg_id that's not in the literal_values table
        let missing_id = IrNodeId::new(999).expect("valid node id");

        let result = lower_stdlib_method(
            "PerCpuOps",
            "percpu_inc",
            InstrMode::Mode64,
            &[missing_id],
            &arena,
        )
        .expect("recipe should exist");

        match result {
            Err(StdlibLoweringError::NonLiteralArg { arg_index, method }) => {
                assert_eq!(arg_index, 0);
                assert_eq!(method, "PerCpuOps::percpu_inc");
            }
            Ok(_) => panic!("expected error for non-literal arg"),
        }
    }

    #[test]
    fn percpu_add_non_literal_arg1_returns_err() {
        let mut arena = IrArena::new();
        let disp_id = IrNodeId::new(1).expect("valid node id");
        let missing_id = IrNodeId::new(999).expect("valid node id");
        arena.literal_values_mut().insert(disp_id, 0x2000);

        let result = lower_stdlib_method(
            "PerCpuOps",
            "percpu_add",
            InstrMode::Mode64,
            &[disp_id, missing_id],
            &arena,
        )
        .expect("recipe should exist");

        match result {
            Err(StdlibLoweringError::NonLiteralArg { arg_index, method }) => {
                assert_eq!(arg_index, 1);
                assert_eq!(method, "PerCpuOps::percpu_add");
            }
            Ok(_) => panic!("expected error for non-literal arg"),
        }
    }
}
