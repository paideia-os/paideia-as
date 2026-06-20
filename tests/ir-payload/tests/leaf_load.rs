//! Pins the populated Instruction payload for a single Load node.
//!
//! This fixture constructs a synthetic Load IR node (load qword from [base + index*8])
//! and verifies that populate_instruction_table produces a MOV instruction with the
//! correct encoding hint (opcode 0x8B, operand_size 8).

use paideia_as_diagnostics::FileId;
use paideia_as_elaborator::populate::{PopulateContext, populate_instruction_table};
use paideia_as_ir::{
    InstructionSideTable, IrArena, IrKind, LoadStoreSideTable, Mnemonic, Operand, alloc_load,
};

fn span() -> paideia_as_diagnostics::Span {
    paideia_as_diagnostics::Span::new(FileId::new(1).unwrap(), 0, 1)
}

#[test]
fn leaf_load_node_populates_as_mov_with_opcode_0x8b() {
    // Build a synthetic Load node: load qword from [base + index*8].
    let mut arena = IrArena::new();
    let mut ls_table = LoadStoreSideTable::new();

    let base = arena.alloc(IrKind::Var, span());
    let index = arena.alloc(IrKind::Var, span());

    let load_info = paideia_as_ir::LoadStoreInfo {
        width: paideia_as_ir::Width::Quad,
        signedness: paideia_as_ir::Signedness::Unsigned,
        alignment: 8,
    };

    let load = alloc_load(&mut arena, &mut ls_table, base, index, load_info, span());

    // Run populate.
    let mut table = InstructionSideTable::new();
    let ctx = PopulateContext {
        arena: &arena,
        load_store: &ls_table,
    };
    let count = populate_instruction_table(&ctx, &mut table);

    // Assert.
    assert_eq!(count, 1);
    let inst = table.get(load).expect("Load should be populated");
    assert_eq!(inst.mnemonic, Mnemonic::Mov);
    assert_eq!(inst.operands.len(), 2);
    assert!(matches!(inst.operands[0], Operand::Reg(_)));
    assert!(matches!(inst.operands[1], Operand::MemSib { .. }));
    assert_eq!(
        inst.encoding_hint.as_ref().unwrap().opcode,
        0x8B,
        "Load should encode as MOV r/m64, r64 (opcode 0x8B)"
    );
    assert_eq!(
        inst.encoding_hint.as_ref().unwrap().operand_size,
        8,
        "Quad load should have operand_size = 8"
    );
}

#[test]
fn leaf_load_half_width_uses_correct_operand_size() {
    let mut arena = IrArena::new();
    let mut ls_table = LoadStoreSideTable::new();

    let base = arena.alloc(IrKind::Var, span());
    let index = arena.alloc(IrKind::Var, span());

    let load_info = paideia_as_ir::LoadStoreInfo {
        width: paideia_as_ir::Width::Half,
        signedness: paideia_as_ir::Signedness::Signed,
        alignment: 2,
    };

    let load = alloc_load(&mut arena, &mut ls_table, base, index, load_info, span());

    let mut table = InstructionSideTable::new();
    let ctx = PopulateContext {
        arena: &arena,
        load_store: &ls_table,
    };
    populate_instruction_table(&ctx, &mut table);

    let inst = table.get(load).unwrap();
    assert_eq!(inst.mnemonic, Mnemonic::Mov);
    assert_eq!(
        inst.encoding_hint.as_ref().unwrap().operand_size,
        2,
        "Half-word load should have operand_size = 2"
    );
}
