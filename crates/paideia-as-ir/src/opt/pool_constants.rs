//! Constant-pool emission for repeated 64-bit immediates.
//!
//! Phase-4-m1-010 (flip apply): detects repeated Imm64 operands via InstructionSideTable
//! and emits a pool entry + rewrites with O1509 diagnostic. Actual PC-relative load
//! emission is deferred to m2 encode stage.

use super::{OptDiagSink, OptPass};
use crate::node::IrNodeId;
use crate::IrArena;
use crate::instruction::Operand;
use std::collections::HashMap;

/// The constant-pool optimization pass.
pub struct PoolConstantsPass;

/// Detect repeated 64-bit immediates in the InstructionSideTable.
/// Returns a map from i64 constant → number of occurrences.
/// Constants appearing ≥2 times are candidates for the constant pool.
pub fn detect_repeated_imm64(arena: &IrArena) -> HashMap<i64, usize> {
    let mut counts = HashMap::new();
    let instructions = arena.instructions();

    // Iterate over all instructions in the side-table
    for (_node_id, inst) in instructions.entries() {
        // Scan operands for Imm64 values
        for operand in &inst.operands {
            if let Operand::Imm64(value) = operand {
                *counts.entry(*value).or_insert(0) += 1;
            }
        }
    }

    counts
}

/// Filter the count map down to pool-candidates (occurrence ≥ 2).
pub fn pool_candidates(counts: &HashMap<i64, usize>) -> Vec<i64> {
    let mut candidates: Vec<i64> = counts
        .iter()
        .filter(|&(_, &n)| n >= 2)
        .map(|(&c, _)| c)
        .collect();
    candidates.sort();
    candidates
}

impl OptPass for PoolConstantsPass {
    fn name(&self) -> &'static str {
        "pool-constants"
    }

    fn apply(&self, arena: &mut IrArena, _root: IrNodeId, sink: &mut OptDiagSink) -> bool {
        // Phase-4-m1-010: detect repeated Imm64 values using InstructionSideTable.
        let counts = detect_repeated_imm64(arena);
        let candidates = pool_candidates(&counts);

        if !candidates.is_empty() {
            // Emit O1509 diagnostic with count of unique pooled values.
            sink.emit(
                "pool-constants",
                format!("O1509 rewrote {} sites", candidates.len()),
            );

            // Phase-4-m1-010 minimum: intern constants in ConstantPoolTable
            // and populate the side-table. Actual PC-relative load rewrite
            // is deferred to m2 emit-stage follow-up.
            let pool = arena.constant_pool_mut();
            for value in candidates {
                pool.intern(value);
            }

            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instruction::Instruction;
    use crate::node::IrKind;
    use paideia_as_diagnostics::{FileId, Span};
    use smallvec::SmallVec;

    #[test]
    fn pool_candidates_detects_repeated_imm() {
        let mut counts = HashMap::new();
        counts.insert(0x1111_1111_1111_1111i64, 2);
        counts.insert(0x2222_2222_2222_2222i64, 1);
        counts.insert(0x3333_3333_3333_3333i64, 3);

        let candidates = pool_candidates(&counts);

        // Should only include values with occurrence ≥ 2, sorted
        assert_eq!(
            candidates,
            vec![0x1111_1111_1111_1111i64, 0x3333_3333_3333_3333i64]
        );
    }

    #[test]
    fn pool_candidates_skips_unique_imm() {
        let mut counts = HashMap::new();
        counts.insert(0x1111_1111_1111_1111i64, 1);
        counts.insert(0x2222_2222_2222_2222i64, 1);
        counts.insert(0x3333_3333_3333_3333i64, 1);

        let candidates = pool_candidates(&counts);

        assert!(
            candidates.is_empty(),
            "unique immediates should produce no pool candidates"
        );
    }

    #[test]
    fn pool_constants_detects_repeated_imm() {
        let mut arena = IrArena::new();
        let file = FileId::new(1).unwrap();
        let span = Span::new(file, 0, 10);

        // Create two instructions with repeated Imm64 value
        let inst1_id = arena.alloc(IrKind::Var, span);
        let inst2_id = arena.alloc(IrKind::Literal, span);

        let value = 0x1111_2222_3333_4444i64;
        let mut operands1 = SmallVec::new();
        operands1.push(Operand::Imm64(value));
        let inst1 = Instruction {
            mnemonic: crate::instruction::Mnemonic::Mov,
            operands: operands1,
            encoding_hint: None,
        };

        let mut operands2 = SmallVec::new();
        operands2.push(Operand::Imm64(value));
        let inst2 = Instruction {
            mnemonic: crate::instruction::Mnemonic::Mov,
            operands: operands2,
            encoding_hint: None,
        };

        arena.instructions_mut().insert(inst1_id, inst1);
        arena.instructions_mut().insert(inst2_id, inst2);

        let counts = detect_repeated_imm64(&arena);
        assert_eq!(counts.get(&value), Some(&2), "Should detect 2 occurrences");
    }

    #[test]
    fn pool_constants_emits_o1509_per_unique_pooled() {
        let mut arena = IrArena::new();
        let file = FileId::new(1).unwrap();
        let span = Span::new(file, 0, 10);

        // Create instructions with 2 repeated immediates
        let inst1_id = arena.alloc(IrKind::Var, span);
        let inst2_id = arena.alloc(IrKind::Literal, span);
        let inst3_id = arena.alloc(IrKind::Var, span);

        let value1 = 0x1111_1111_1111_1111i64;
        let value2 = 0x2222_2222_2222_2222i64;

        let mut operands1 = SmallVec::new();
        operands1.push(Operand::Imm64(value1));
        let inst1 = Instruction {
            mnemonic: crate::instruction::Mnemonic::Mov,
            operands: operands1,
            encoding_hint: None,
        };

        let mut operands2 = SmallVec::new();
        operands2.push(Operand::Imm64(value1));
        let inst2 = Instruction {
            mnemonic: crate::instruction::Mnemonic::Add,
            operands: operands2,
            encoding_hint: None,
        };

        let mut operands3 = SmallVec::new();
        operands3.push(Operand::Imm64(value2));
        operands3.push(Operand::Imm64(value2));
        let inst3 = Instruction {
            mnemonic: crate::instruction::Mnemonic::Sub,
            operands: operands3,
            encoding_hint: None,
        };

        arena.instructions_mut().insert(inst1_id, inst1);
        arena.instructions_mut().insert(inst2_id, inst2);
        arena.instructions_mut().insert(inst3_id, inst3);

        let pass = PoolConstantsPass;
        let mut sink = OptDiagSink::new();
        let dummy_id = IrNodeId::new(1).unwrap();

        let changed = pass.apply(&mut arena, dummy_id, &mut sink);

        assert!(changed, "PoolConstantsPass should detect and rewrite");
        assert_eq!(sink.diagnostics.len(), 1, "Expected one diagnostic emitted");
        assert_eq!(sink.diagnostics[0].pass, "pool-constants");
        assert!(
            sink.diagnostics[0]
                .message
                .contains("O1509 rewrote 2 sites"),
            "Diagnostic should report 2 pooled constants"
        );

        // Verify the pool was populated
        let pool = arena.constant_pool();
        assert_eq!(pool.len(), 2);
        assert_eq!(pool.offset_of(value1), Some(0));
        assert_eq!(pool.offset_of(value2), Some(8));
    }

    #[test]
    fn constant_pool_table_intern_deterministic() {
        let mut pool = crate::ConstantPoolTable::new();
        let value = 0x1111_2222_3333_4444i64;
        let offset1 = pool.intern(value);
        let offset2 = pool.intern(value);
        assert_eq!(offset1, offset2, "Same value should return same offset");
        assert_eq!(pool.len(), 1, "Pool should contain exactly one entry");
    }

    #[test]
    fn pool_constants_skips_unique_imm() {
        let mut arena = IrArena::new();
        let file = FileId::new(1).unwrap();
        let span = Span::new(file, 0, 10);

        // Create instructions with unique Imm64 values
        let inst1_id = arena.alloc(IrKind::Var, span);
        let inst2_id = arena.alloc(IrKind::Literal, span);

        let mut operands1 = SmallVec::new();
        operands1.push(Operand::Imm64(0x1111_1111_1111_1111i64));
        let inst1 = Instruction {
            mnemonic: crate::instruction::Mnemonic::Mov,
            operands: operands1,
            encoding_hint: None,
        };

        let mut operands2 = SmallVec::new();
        operands2.push(Operand::Imm64(0x2222_2222_2222_2222i64));
        let inst2 = Instruction {
            mnemonic: crate::instruction::Mnemonic::Add,
            operands: operands2,
            encoding_hint: None,
        };

        arena.instructions_mut().insert(inst1_id, inst1);
        arena.instructions_mut().insert(inst2_id, inst2);

        let pass = PoolConstantsPass;
        let mut sink = OptDiagSink::new();
        let dummy_id = IrNodeId::new(1).unwrap();

        let changed = pass.apply(&mut arena, dummy_id, &mut sink);

        assert!(
            !changed,
            "PoolConstantsPass should not rewrite unique immediates"
        );
        assert_eq!(
            sink.diagnostics.len(),
            0,
            "No diagnostic for unique immediates"
        );
        assert_eq!(
            arena.constant_pool().len(),
            0,
            "Pool should remain empty for unique immediates"
        );
    }
}
