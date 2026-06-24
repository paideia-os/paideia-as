//! Pins the populated Instruction payload for a single Store node.
//!
//! This fixture constructs a synthetic Store IR node (store word to [base + index*4])
//! and verifies that populate_instruction_table produces a MOV instruction with the
//! correct encoding hint (opcode 0x89, operand_size 4).

use paideia_as_diagnostics::FileId;
use paideia_as_elaborator::populate::{PopulateContext, populate_instruction_table};
use paideia_as_ir::{
    CallSideTable, InstructionSideTable, IrArena, IrKind, InstrMode, LoadStoreSideTable, Mnemonic, Operand,
    alloc_store,
};

fn span() -> paideia_as_diagnostics::Span {
    paideia_as_diagnostics::Span::new(FileId::new(1).unwrap(), 0, 1)
}

#[test]
fn leaf_store_node_populates_as_mov_with_opcode_0x89() {
    // Build a synthetic Store node: store word to [base + index*4].
    let mut arena = IrArena::new();
    let mut ls_table = LoadStoreSideTable::new();

    let base = arena.alloc(IrKind::Var, span());
    let index = arena.alloc(IrKind::Var, span());
    let value = arena.alloc(IrKind::Var, span());

    let store_info = paideia_as_ir::LoadStoreInfo {
        width: paideia_as_ir::Width::Word,
        signedness: paideia_as_ir::Signedness::Unsigned,
        alignment: 4,
    };

    let store = alloc_store(
        &mut arena,
        &mut ls_table,
        base,
        index,
        value,
        store_info,
        span(),
    );

    // Run populate.
    let mut table = InstructionSideTable::new();
    let call_table = CallSideTable::new();
    let ctx = PopulateContext {
        arena: &arena,
        load_store: &ls_table,
        call_table: &call_table,
        instr_mode: InstrMode::Mode64,
    };
    let count = populate_instruction_table(&ctx, &mut table);

    // Assert.
    assert_eq!(count, 1);
    let inst = table.get(store).expect("Store should be populated");
    assert_eq!(inst.mnemonic, Mnemonic::Mov);
    assert_eq!(inst.operands.len(), 2);
    assert!(matches!(inst.operands[0], Operand::MemSib { .. }));
    assert!(matches!(inst.operands[1], Operand::Reg(_)));
    assert_eq!(
        inst.encoding_hint.as_ref().unwrap().opcode,
        0x89,
        "Store should encode as MOV r/m64, r64 (opcode 0x89)"
    );
    assert_eq!(
        inst.encoding_hint.as_ref().unwrap().operand_size,
        4,
        "Word store should have operand_size = 4"
    );
}

#[test]
fn leaf_store_byte_width_uses_correct_operand_size() {
    let mut arena = IrArena::new();
    let mut ls_table = LoadStoreSideTable::new();

    let base = arena.alloc(IrKind::Var, span());
    let index = arena.alloc(IrKind::Var, span());
    let value = arena.alloc(IrKind::Var, span());

    let store_info = paideia_as_ir::LoadStoreInfo {
        width: paideia_as_ir::Width::Byte,
        signedness: paideia_as_ir::Signedness::Unsigned,
        alignment: 1,
    };

    let store = alloc_store(
        &mut arena,
        &mut ls_table,
        base,
        index,
        value,
        store_info,
        span(),
    );

    let mut table = InstructionSideTable::new();
    let call_table = CallSideTable::new();
    let ctx = PopulateContext {
        arena: &arena,
        load_store: &ls_table,
        call_table: &call_table,
        instr_mode: InstrMode::Mode64,
    };
    populate_instruction_table(&ctx, &mut table);

    let inst = table.get(store).unwrap();
    assert_eq!(inst.mnemonic, Mnemonic::Mov);
    assert_eq!(
        inst.encoding_hint.as_ref().unwrap().operand_size,
        1,
        "Byte store should have operand_size = 1"
    );
}
