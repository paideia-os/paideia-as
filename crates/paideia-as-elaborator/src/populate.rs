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
    CallSideTable, EncodingHint, InstrMode, Instruction, InstructionSideTable, IrArena, IrKind,
    IrNodeId, LoadStoreSideTable, Mnemonic, Operand, RegId, Scale, SmallVec, Width as IrWidth,
};

/// Context for populating the instruction table.
///
/// Holds references to the IR arena, load/store side-table, and call side-table,
/// which are needed to inspect Load/Store/App nodes and extract their metadata.
pub struct PopulateContext<'a> {
    /// The IR arena containing all nodes.
    pub arena: &'a IrArena,
    /// The load/store side-table with width/signedness/alignment metadata.
    pub load_store: &'a LoadStoreSideTable,
    /// The call side-table with callee name / intrinsic flag metadata.
    pub call_table: &'a CallSideTable,
    /// Phase 15 m2-002a: The current instruction mode (Mode64 or Mode32).
    pub instr_mode: InstrMode,
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
        if let Some(node_id) = IrNodeId::new((i + 1) as u32)
            && populate_one(ctx, node_id, table)
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
                byte_offset_in_text: None,
                mode: ctx.instr_mode,
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
                byte_offset_in_text: None,
                mode: ctx.instr_mode,
            };
            table.insert(id, inst);
            true
        }
        IrKind::App => {
            // Intrinsic App detection: check if this is an intrinsic call
            // via the call side-table.
            let meta = match ctx.call_table.get(id) {
                Some(m) => m,
                None => return false, // no metadata yet
            };
            if !meta.is_intrinsic {
                return false; // not an intrinsic call
            }
            synthesise_intrinsic_instruction(ctx, id, meta, table)
        }
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

/// Synthesise an instruction for an intrinsic call.
///
/// Phase-3-m1-001 honest scope: only index_u64, index_u64_set, and
/// ptr_sub_bytes_u64 are properly implemented. Other intrinsics emit
/// a stub Mov(RDI, RAX).
fn synthesise_intrinsic_instruction(
    ctx: &PopulateContext,
    id: IrNodeId,
    meta: &paideia_as_ir::CallMeta,
    table: &mut InstructionSideTable,
) -> bool {
    let inst = match meta.callee_name.as_str() {
        "index_u64" => {
            // Children: [callee, ptr, index]
            let children = ctx.arena.children(id);
            if children.len() < 3 {
                return false;
            }
            // index_u64 reads from memory: (ptr: *u64, index: u64) -> u64
            // Synthesise: Mov RAX, [RDI + RSI*8]
            let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
            ops.push(Operand::Reg(RegId(0))); // RAX (result register)
            ops.push(Operand::MemSib {
                base: RegId(7),        // RDI (ptr)
                index: Some(RegId(7)), // RSI (index) — placeholder for reg alloc
                scale: Scale::X8,
                disp: 0,
            });
            Instruction {
                mnemonic: Mnemonic::Mov,
                operands: ops,
                encoding_hint: Some(EncodingHint {
                    opcode: 0x8B,
                    operand_size: 8,
                }),
                byte_offset_in_text: None,
                mode: ctx.instr_mode,
            }
        }
        "index_u64_set" => {
            // Children: [callee, ptr, index, value]
            let children = ctx.arena.children(id);
            if children.len() < 4 {
                return false;
            }
            // index_u64_set writes to memory: (ptr: *u64, index: u64, value: u64) -> ()
            // Synthesise: Mov [RDI + RSI*8], RAX
            let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
            ops.push(Operand::MemSib {
                base: RegId(7),        // RDI (ptr)
                index: Some(RegId(7)), // RSI (index) — placeholder for reg alloc
                scale: Scale::X8,
                disp: 0,
            });
            ops.push(Operand::Reg(RegId(0))); // RAX (value to write)
            Instruction {
                mnemonic: Mnemonic::Mov,
                operands: ops,
                encoding_hint: Some(EncodingHint {
                    opcode: 0x89,
                    operand_size: 8,
                }),
                byte_offset_in_text: None,
                mode: ctx.instr_mode,
            }
        }
        "ptr_sub_bytes_u64" => {
            // Children: [callee, ptr1, ptr2]
            let children = ctx.arena.children(id);
            if children.len() < 3 {
                return false;
            }
            // ptr_sub_bytes_u64 computes byte distance: (ptr1: *u64, ptr2: *u64) -> u64
            // Synthesise: Sub RAX, RDI
            let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
            ops.push(Operand::Reg(RegId(0))); // RAX
            ops.push(Operand::Reg(RegId(7))); // RDI
            Instruction {
                mnemonic: Mnemonic::Sub,
                operands: ops,
                encoding_hint: Some(EncodingHint {
                    opcode: 0x29,
                    operand_size: 8,
                }),
                byte_offset_in_text: None,
                mode: ctx.instr_mode,
            }
        }
        _ => {
            // Other intrinsics: stub Mov(RDI, RAX)
            let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
            ops.push(Operand::Reg(RegId(0))); // RAX
            ops.push(Operand::Reg(RegId(7))); // RDI
            Instruction {
                mnemonic: Mnemonic::Mov,
                operands: ops,
                encoding_hint: Some(EncodingHint {
                    opcode: 0x89,
                    operand_size: 8,
                }),
                byte_offset_in_text: None,
                mode: ctx.instr_mode,
            }
        }
    };
    table.insert(id, inst);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::FileId;
    use paideia_as_ir::load_store::{LoadStoreInfo, Signedness, alloc_load, alloc_store};
    use paideia_as_ir::{CallMeta, CallSideTable};

    fn span() -> paideia_as_diagnostics::Span {
        paideia_as_diagnostics::Span::new(FileId::new(1).unwrap(), 0, 1)
    }

    #[test]
    fn populate_empty_arena_returns_zero() {
        let arena = IrArena::new();
        let load_store = LoadStoreSideTable::new();
        let call_table = CallSideTable::new();
        let ctx = PopulateContext {
            arena: &arena,
            load_store: &load_store,
            call_table: &call_table,
            instr_mode: InstrMode::Mode64,
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

        let call_table = CallSideTable::new();
        let ctx = PopulateContext {
            arena: &arena,
            load_store: &load_store,
            call_table: &call_table,
            instr_mode: InstrMode::Mode64,
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

        let call_table = CallSideTable::new();
        let ctx = PopulateContext {
            arena: &arena,
            load_store: &load_store,
            call_table: &call_table,
            instr_mode: InstrMode::Mode64,
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

        let call_table = CallSideTable::new();
        let ctx = PopulateContext {
            arena: &arena,
            load_store: &load_store,
            call_table: &call_table,
            instr_mode: InstrMode::Mode64,
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

        let call_table = CallSideTable::new();
        let ctx = PopulateContext {
            arena: &arena,
            load_store: &load_store,
            call_table: &call_table,
            instr_mode: InstrMode::Mode64,
        };
        let mut table = InstructionSideTable::new();
        let populated = populate_instruction_table(&ctx, &mut table);

        // Load has no side-table entry, so it should be skipped
        assert_eq!(populated, 0);
        assert!(table.is_empty());
    }

    // ── Intrinsic call tests ────────────────────────────────────────

    #[test]
    fn call_side_table_insert_and_get() {
        let mut table = CallSideTable::new();
        let call_id = IrNodeId::new(1).unwrap();

        let meta = CallMeta {
            callee_name: "index_u64".to_string(),
            arg_count: 2,
            is_intrinsic: true,
        };

        table.insert(call_id, meta.clone());
        let retrieved = table.get(call_id);

        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().callee_name, "index_u64");
        assert_eq!(retrieved.unwrap().arg_count, 2);
        assert!(retrieved.unwrap().is_intrinsic);
    }

    #[test]
    fn call_side_table_intrinsic_call_ids_filters() {
        let mut table = CallSideTable::new();

        let intrinsic_id = IrNodeId::new(1).unwrap();
        let user_id = IrNodeId::new(2).unwrap();

        table.insert(
            intrinsic_id,
            CallMeta {
                callee_name: "index_u64".to_string(),
                arg_count: 2,
                is_intrinsic: true,
            },
        );

        table.insert(
            user_id,
            CallMeta {
                callee_name: "my_func".to_string(),
                arg_count: 1,
                is_intrinsic: false,
            },
        );

        let intrinsic_ids: Vec<_> = table.intrinsic_call_ids().collect();
        assert_eq!(intrinsic_ids.len(), 1);
        assert_eq!(intrinsic_ids[0], intrinsic_id);
    }

    #[test]
    fn populate_recognises_intrinsic_call_index_u64() {
        let mut arena = IrArena::new();
        let load_store = LoadStoreSideTable::new();
        let mut call_table = CallSideTable::new();

        // Create an App node: [callee, ptr, index]
        let callee_id = arena.alloc(IrKind::Var, span());
        let ptr_id = arena.alloc(IrKind::Var, span());
        let idx_id = arena.alloc(IrKind::Literal, span());

        let app_id = arena.alloc_with_children(IrKind::App, span(), [callee_id, ptr_id, idx_id]);

        // Register as intrinsic
        call_table.insert(
            app_id,
            CallMeta {
                callee_name: "index_u64".to_string(),
                arg_count: 2,
                is_intrinsic: true,
            },
        );

        let ctx = PopulateContext {
            arena: &arena,
            load_store: &load_store,
            call_table: &call_table,
            instr_mode: InstrMode::Mode64,
        };

        let mut table = InstructionSideTable::new();
        let populated = populate_instruction_table(&ctx, &mut table);

        assert_eq!(populated, 1);
        let inst = table.get(app_id).unwrap();
        assert_eq!(inst.mnemonic, Mnemonic::Mov);
        assert_eq!(inst.operands.len(), 2);
    }

    #[test]
    fn populate_skips_non_intrinsic_user_call() {
        let mut arena = IrArena::new();
        let load_store = LoadStoreSideTable::new();
        let mut call_table = CallSideTable::new();

        // Create an App node
        let callee_id = arena.alloc(IrKind::Var, span());
        let arg_id = arena.alloc(IrKind::Literal, span());

        let app_id = arena.alloc_with_children(IrKind::App, span(), [callee_id, arg_id]);

        // Register as non-intrinsic user call
        call_table.insert(
            app_id,
            CallMeta {
                callee_name: "my_function".to_string(),
                arg_count: 1,
                is_intrinsic: false,
            },
        );

        let ctx = PopulateContext {
            arena: &arena,
            load_store: &load_store,
            call_table: &call_table,
            instr_mode: InstrMode::Mode64,
        };

        let mut table = InstructionSideTable::new();
        let populated = populate_instruction_table(&ctx, &mut table);

        // Non-intrinsic calls should not populate instructions
        assert_eq!(populated, 0);
        assert!(table.is_empty());
    }

    #[test]
    fn populate_synthesises_mov_for_index_u64() {
        let mut arena = IrArena::new();
        let load_store = LoadStoreSideTable::new();
        let mut call_table = CallSideTable::new();

        let callee_id = arena.alloc(IrKind::Var, span());
        let ptr_id = arena.alloc(IrKind::Var, span());
        let idx_id = arena.alloc(IrKind::Literal, span());

        let app_id = arena.alloc_with_children(IrKind::App, span(), [callee_id, ptr_id, idx_id]);

        call_table.insert(
            app_id,
            CallMeta {
                callee_name: "index_u64".to_string(),
                arg_count: 2,
                is_intrinsic: true,
            },
        );

        let ctx = PopulateContext {
            arena: &arena,
            load_store: &load_store,
            call_table: &call_table,
            instr_mode: InstrMode::Mode64,
        };

        let mut table = InstructionSideTable::new();
        populate_instruction_table(&ctx, &mut table);

        let inst = table.get(app_id).unwrap();
        assert_eq!(inst.mnemonic, Mnemonic::Mov);
        assert_eq!(inst.operands.len(), 2);
        // First operand should be RAX
        assert!(matches!(inst.operands[0], Operand::Reg(RegId(0))));
        // Second operand should be MemSib with scale X8 (for u64)
        match inst.operands[1] {
            Operand::MemSib {
                scale: Scale::X8, ..
            } => {}
            _ => panic!("Expected MemSib with X8 scale"),
        }
        assert_eq!(inst.encoding_hint.unwrap().opcode, 0x8B);
        assert_eq!(inst.encoding_hint.unwrap().operand_size, 8);
    }

    #[test]
    fn populate_synthesises_sub_for_ptr_sub_bytes() {
        let mut arena = IrArena::new();
        let load_store = LoadStoreSideTable::new();
        let mut call_table = CallSideTable::new();

        let callee_id = arena.alloc(IrKind::Var, span());
        let ptr1_id = arena.alloc(IrKind::Var, span());
        let ptr2_id = arena.alloc(IrKind::Var, span());

        let app_id = arena.alloc_with_children(IrKind::App, span(), [callee_id, ptr1_id, ptr2_id]);

        call_table.insert(
            app_id,
            CallMeta {
                callee_name: "ptr_sub_bytes_u64".to_string(),
                arg_count: 2,
                is_intrinsic: true,
            },
        );

        let ctx = PopulateContext {
            arena: &arena,
            load_store: &load_store,
            call_table: &call_table,
            instr_mode: InstrMode::Mode64,
        };

        let mut table = InstructionSideTable::new();
        populate_instruction_table(&ctx, &mut table);

        let inst = table.get(app_id).unwrap();
        assert_eq!(inst.mnemonic, Mnemonic::Sub);
        assert_eq!(inst.operands.len(), 2);
        assert!(matches!(inst.operands[0], Operand::Reg(RegId(0)))); // RAX
        assert!(matches!(inst.operands[1], Operand::Reg(RegId(7)))); // RDI
        assert_eq!(inst.encoding_hint.unwrap().opcode, 0x29);
        assert_eq!(inst.encoding_hint.unwrap().operand_size, 8);
    }
}
