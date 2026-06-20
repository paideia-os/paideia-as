//! Populate the IR Instruction side-table from an elaborated module.
//!
//! Phase-3-m2-003 minimum: this is the chokepoint that wires together
//! m1-006 Load/Store nodes + m1-004 intrinsic calls + m2-001 Instruction
//! payloads. It walks the IR tree, recognises Load / Store / intrinsic-
//! call sites, and inserts the corresponding Instruction record into
//! the side-table.
//!
//! Full coverage (every AST construct) is m2-004+; this issue ships
//! the chokepoint + the three load-bearing cases (Load, Store, intrinsic
//! App) so opt passes can start consuming real per-node payloads.

use paideia_as_ir::{
    EncodingHint, Instruction, InstructionSideTable, IrArena, IrKind, IrNodeId,
    LoadStoreSideTable, Mnemonic, Operand, RegId, Scale, SmallVec, Width as IrWidth,
};

/// Context for populating the instruction table.
///
/// Holds references to the IR arena and the load/store side-table,
/// which are needed to inspect Load/Store nodes and extract their metadata.
pub struct PopulateContext<'a> {
    /// The IR arena containing all nodes.
    pub arena: &'a IrArena,
    /// The load/store side-table with width/signedness/alignment metadata.
    pub load_store: &'a LoadStoreSideTable,
}

/// Populate the instruction side-table by walking every node in the arena
/// and recognising the Load / Store / intrinsic-call cases.
///
/// Returns the count of nodes that were populated.
///
/// # Arguments
///
/// * `ctx` - The populate context with arena and load/store side-table references.
/// * `table` - The instruction side-table to populate.
///
/// # Returns
///
/// The number of instructions inserted into the table.
pub fn populate_instruction_table(
    ctx: &PopulateContext,
    table: &mut InstructionSideTable,
) -> usize {
    let mut populated = 0;
    // Walk all nodes in the arena by index.
    for i in 0..ctx.arena.len() {
        // Convert 0-based index to 1-based IrNodeId.
        if let Some(node_id) = IrNodeId::new((i + 1) as u32) && populate_one(ctx, node_id, table)
        {
            populated += 1;
        }
    }
    populated
}

/// Populate a single node if it matches a recognizable pattern.
///
/// Returns `true` if an instruction was inserted, `false` otherwise.
fn populate_one(ctx: &PopulateContext, id: IrNodeId, table: &mut InstructionSideTable) -> bool {
    let node = match ctx.arena.get(id) {
        Some(n) => n,
        None => return false,
    };
    match node.kind {
        IrKind::Load => {
            // Children: [pointer, index]. LoadStoreInfo gives width.
            let info = match ctx.load_store.get(id) {
                Some(i) => *i,
                None => return false, // no metadata yet
            };
            let children = ctx.arena.children(id);
            if children.len() != 2 {
                return false;
            }
            let base_reg = node_to_reg(ctx, children[0]);
            let index_reg = node_to_reg(ctx, children[1]);
            let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
            ops.push(Operand::Reg(RegId(0))); // RAX (canonical dest; real reg-alloc is m2-004+)
            ops.push(Operand::MemSib {
                base: base_reg,
                index: Some(index_reg),
                scale: width_to_scale(info.width),
                disp: 0,
            });
            let inst = Instruction {
                mnemonic: Mnemonic::Mov,
                operands: ops,
                encoding_hint: Some(EncodingHint {
                    opcode: 0x8B,
                    operand_size: info.width.bytes() as u8,
                }),
            };
            table.insert(id, inst);
            true
        }
        IrKind::Store => {
            // Children: [pointer, index, value]. LoadStoreInfo gives width.
            let info = match ctx.load_store.get(id) {
                Some(i) => *i,
                None => return false, // no metadata yet
            };
            let children = ctx.arena.children(id);
            if children.len() != 3 {
                return false;
            }
            let base_reg = node_to_reg(ctx, children[0]);
            let index_reg = node_to_reg(ctx, children[1]);
            let value_reg = node_to_reg(ctx, children[2]);
            let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
            ops.push(Operand::MemSib {
                base: base_reg,
                index: Some(index_reg),
                scale: width_to_scale(info.width),
                disp: 0,
            });
            ops.push(Operand::Reg(value_reg));
            let inst = Instruction {
                mnemonic: Mnemonic::Mov,
                operands: ops,
                encoding_hint: Some(EncodingHint {
                    opcode: 0x89,
                    operand_size: info.width.bytes() as u8,
                }),
            };
            table.insert(id, inst);
            true
        }
        // Intrinsic App detection: would require Call-node introspection
        // + name resolution. Phase-3-m2-003 deferred — populated only
        // when the App lowering wires in m2-004.
        _ => false,
    }
}

/// Placeholder register allocation: returns a register for a node.
///
/// Phase-3-m2-003 placeholder: returns RDI for now.
/// Real reg-alloc is m2-004 (which threads from the calling-convention
/// arg-slot table to per-node SSA-style register assignment).
fn node_to_reg(_ctx: &PopulateContext, _id: IrNodeId) -> RegId {
    RegId(7) // RDI
}

/// Convert a Width to the corresponding Scale for SIB addressing.
fn width_to_scale(w: IrWidth) -> Scale {
    match w {
        IrWidth::Byte => Scale::X1,
        IrWidth::Half => Scale::X2,
        IrWidth::Word => Scale::X4,
        IrWidth::Quad => Scale::X8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::FileId;
    use paideia_as_ir::load_store::{alloc_load, alloc_store, LoadStoreInfo, Signedness};

    fn span() -> paideia_as_diagnostics::Span {
        paideia_as_diagnostics::Span::new(FileId::new(1).unwrap(), 0, 1)
    }

    #[test]
    fn populate_empty_arena_returns_zero() {
        let arena = IrArena::new();
        let load_store = LoadStoreSideTable::new();
        let ctx = PopulateContext {
            arena: &arena,
            load_store: &load_store,
        };
        let mut table = InstructionSideTable::new();
        let populated = populate_instruction_table(&ctx, &mut table);
        assert_eq!(populated, 0);
        assert!(table.is_empty());
    }

    #[test]
    fn populate_load_node_inserts_mov_instruction() {
        let mut arena = IrArena::new();
        let mut load_store = LoadStoreSideTable::new();

        let ptr_id = arena.alloc(IrKind::Var, span());
        let idx_id = arena.alloc(IrKind::Literal, span());

        let info = LoadStoreInfo {
            width: IrWidth::Quad,
            signedness: Signedness::Unsigned,
            alignment: 8,
        };

        let load_id = alloc_load(&mut arena, &mut load_store, ptr_id, idx_id, info, span());

        let ctx = PopulateContext {
            arena: &arena,
            load_store: &load_store,
        };
        let mut table = InstructionSideTable::new();
        let populated = populate_instruction_table(&ctx, &mut table);

        assert_eq!(populated, 1);
        assert_eq!(table.len(), 1);

        let inst = table.get(load_id).unwrap();
        assert_eq!(inst.mnemonic, Mnemonic::Mov);
        assert_eq!(inst.operands.len(), 2);
        assert!(matches!(inst.operands[0], Operand::Reg(_)));
        assert!(matches!(inst.operands[1], Operand::MemSib { .. }));
        assert_eq!(inst.encoding_hint.unwrap().opcode, 0x8B);
        assert_eq!(inst.encoding_hint.unwrap().operand_size, 8);
    }

    #[test]
    fn populate_store_node_inserts_mov_with_opcode_0x89() {
        let mut arena = IrArena::new();
        let mut load_store = LoadStoreSideTable::new();

        let ptr_id = arena.alloc(IrKind::Var, span());
        let idx_id = arena.alloc(IrKind::Literal, span());
        let val_id = arena.alloc(IrKind::Literal, span());

        let info = LoadStoreInfo {
            width: IrWidth::Half,
            signedness: Signedness::Signed,
            alignment: 2,
        };

        let store_id = alloc_store(
            &mut arena,
            &mut load_store,
            ptr_id,
            idx_id,
            val_id,
            info,
            span(),
        );

        let ctx = PopulateContext {
            arena: &arena,
            load_store: &load_store,
        };
        let mut table = InstructionSideTable::new();
        let populated = populate_instruction_table(&ctx, &mut table);

        assert_eq!(populated, 1);
        assert_eq!(table.len(), 1);

        let inst = table.get(store_id).unwrap();
        assert_eq!(inst.mnemonic, Mnemonic::Mov);
        assert_eq!(inst.operands.len(), 2);
        assert!(matches!(inst.operands[0], Operand::MemSib { .. }));
        assert!(matches!(inst.operands[1], Operand::Reg(_)));
        assert_eq!(inst.encoding_hint.unwrap().opcode, 0x89);
        assert_eq!(inst.encoding_hint.unwrap().operand_size, 2);
    }

    #[test]
    fn populate_mixed_arena_skips_unrecognized_kinds() {
        let mut arena = IrArena::new();
        let mut load_store = LoadStoreSideTable::new();

        // Create a Load node
        let ptr_id = arena.alloc(IrKind::Var, span());
        let idx_id = arena.alloc(IrKind::Literal, span());
        let info = LoadStoreInfo {
            width: IrWidth::Word,
            signedness: Signedness::Unsigned,
            alignment: 4,
        };
        let _load_id = alloc_load(&mut arena, &mut load_store, ptr_id, idx_id, info, span());

        // Create a Placeholder node (not recognized)
        let _placeholder_id = arena.alloc(IrKind::Placeholder, span());

        let ctx = PopulateContext {
            arena: &arena,
            load_store: &load_store,
        };
        let mut table = InstructionSideTable::new();
        let populated = populate_instruction_table(&ctx, &mut table);

        // Only the Load should be populated; Placeholder is ignored
        assert_eq!(populated, 1);
        assert_eq!(table.len(), 1);
    }

    #[test]
    fn populate_load_without_side_table_entry_skips_node() {
        let mut arena = IrArena::new();
        let load_store = LoadStoreSideTable::new();

        let ptr_id = arena.alloc(IrKind::Var, span());
        let idx_id = arena.alloc(IrKind::Literal, span());

        // Create Load node but don't insert into load_store
        let _load_id = arena.alloc_with_children(IrKind::Load, span(), [ptr_id, idx_id]);

        let ctx = PopulateContext {
            arena: &arena,
            load_store: &load_store,
        };
        let mut table = InstructionSideTable::new();
        let populated = populate_instruction_table(&ctx, &mut table);

        // Load has no side-table entry, so it should be skipped
        assert_eq!(populated, 0);
        assert!(table.is_empty());
    }
}
