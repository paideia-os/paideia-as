//! Common test utilities for regression harness.

use paideia_as_diagnostics::{FileId, Span};
use paideia_as_ir::{
    IrArena, IrKind, IrNodeId,
    instruction::{Instruction, Mnemonic, Operand},
};
use smallvec::SmallVec;

/// Create a minimal test arena with a single function containing one basic block.
pub fn create_test_arena() -> (IrArena, IrNodeId) {
    let mut arena = IrArena::new();

    // Create a minimal functor node (root for testing).
    let file = FileId::new(1).unwrap();
    let span = Span::new(file, 0, 10);
    let func = arena.alloc(IrKind::Functor, span);

    (arena, func)
}

/// Insert a single instruction into the arena at the given node ID.
pub fn insert_instruction(
    arena: &mut IrArena,
    node_id: IrNodeId,
    mnemonic: Mnemonic,
    operands: Vec<Operand>,
) {
    let mut ops = SmallVec::new();
    for op in operands {
        ops.push(op);
    }
    let inst = Instruction {
        mnemonic,
        operands: ops,
        byte_offset_in_text: None,
        encoding_hint: None,
    };
    arena.instructions_mut().insert(node_id, inst);
}

/// Create a load node and insert it with instruction payload.
pub fn create_instruction_node(
    arena: &mut IrArena,
    mnemonic: Mnemonic,
    operands: Vec<Operand>,
) -> IrNodeId {
    let file = FileId::new(1).unwrap();
    let span = Span::new(file, 0, 5);
    let node_id = arena.alloc(IrKind::Load, span);
    insert_instruction(arena, node_id, mnemonic, operands);
    node_id
}
