//! Integration test for PA-R17-015: call [rip + sym + addend]
//!
//! Verifies that the elaborator wiring for field-access function calls is correct
//! by directly testing the IR-level emit path (without requiring #1073 AST→IR lowering).
//!
//! IR-level test hand-synthesizes:
//! - Lambda with App(FieldAccess(Var)) as body
//! - FieldAccessInfo and RecordLayout for the vtable structure
//! - Module-level symbol for the vtable
//!
//! Then verifies:
//! - Emitted instruction is a Call with Operand::MemRipRelSym
//! - No intermediate mov r11 instruction appears before the call
//! - Call operand names the module symbol with correct field offset addend
//!
//! Once #1073 lands (AST→IR lowering for FieldAccess-callee), real source code
//! patterns like `(vops.func)()` will produce this exact IR shape and exercise
//! this code path end-to-end.

use paideia_as_diagnostics::{FileId, Span};
use paideia_as_ir::record_layout::{FieldAccessInfo, FieldLayout, RecordLayout, RecordTypeId};
use paideia_as_ir::{IrArena, IrKind};
use paideia_as_elaborator::emit_walker::EmitWalker;
use paideia_as_ir::instruction::{Mnemonic, Operand};
use paideia_as_ir::abi;
use paideia_as_ir::symbol::{Symbol, SymbolKind};

fn span() -> Span {
    Span::new(FileId::new(1).unwrap(), 0, 1)
}

#[test]
fn emit_indirect_call_via_mem_rip_sym_no_intermediate_mov() {
    // IR-level test: hand-synthesize a lambda containing an App with FieldAccess-callee.
    //
    // Shape:
    //   Lambda {
    //     body: App {
    //       callee: FieldAccess(Var("vops")),  // receiver = module-level "vops"
    //       args: [Literal(0)]                  // placeholder arg
    //     }
    //   }
    //
    // The elaborator should emit:
    //   1. Argument marshalling (mov rdi, 0)
    //   2. Single call [rip + vops + 0]
    //   3. ret
    //
    // NOT:
    //   - mov r11, [rip + vops]
    //   - call r11

    let mut arena = IrArena::new();

    // 1. Build the receiver Var("vops")
    let receiver_var_id = arena.alloc(IrKind::Var, span());
    arena
        .binding_names_mut()
        .insert(receiver_var_id, "vops".to_string());

    // 2. Build the FieldAccess(receiver_var)
    let field_access_id = arena.alloc_with_children(IrKind::FieldAccess, span(), [receiver_var_id]);

    // 3. Populate FieldAccessInfo: type_id = RecordTypeId(100), field_index = 0
    let field_type_id = RecordTypeId(100);
    let field_info = FieldAccessInfo {
        type_id: field_type_id,
        field_index: 0,
    };
    arena
        .field_access_info_mut()
        .insert(field_access_id, field_info);

    // 4. Build the argument: Literal(0)
    let arg_lit_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(arg_lit_id, 0);

    // 5. Build the App(callee, arg0)
    let app_id = arena.alloc_with_children(IrKind::App, span(), [field_access_id, arg_lit_id]);

    // 6. Build the Lambda(app)
    let _lambda_id = arena.alloc_with_children(IrKind::Lambda, span(), [app_id]);

    // 7. Register the module-level symbol "vops" in the arena's symbol table
    // so the elaborator recognizes it as a module-level binding.
    arena.symbols_mut().insert(Symbol::new(
        "vops".to_string(),
        SymbolKind::Object,
        receiver_var_id,
    ));

    // 8. Register the RecordLayout: "vops" is a struct with 1 u64 field at offset 0
    let layout = RecordLayout::new(
        8,  // size
        8,  // align
        vec![FieldLayout {
            offset: 0,
            size: 8,
            signed: false,
        }],
    );

    // 9. Create and configure the walker
    let mut walker = EmitWalker::new();
    walker
        .state_mut()
        .insert_record_layout(field_type_id, layout);

    // 10. Walk the arena to emit instructions
    walker.walk(&mut arena);

    // 11. Verify the emitted instructions
    //
    // Expected: the elaborator should emit multiple instructions for the lambda.
    // The call instruction should be at base*16 + seq_id (determined by argument marshalling).
    //
    // We'll look for the Call instruction and verify it has Operand::MemRipRelSym.

    let instructions = walker.state().instructions();

    // Find the Call instruction among emitted instructions
    let mut found_call_with_mem_rip_rel = false;
    let mut found_intermediate_mov_r11 = false;

    for (node_id, inst) in instructions.iter() {
        if inst.mnemonic == Mnemonic::Call {
            // Check if the call operand is MemRipRelSym
            if inst.operands.len() >= 1 {
                if let Operand::MemRipRelSym { name, addend } = &inst.operands[0] {
                    // Verify symbol name and addend
                    assert_eq!(
                        name, "vops",
                        "Call symbol should be 'vops', got '{}'",
                        name
                    );
                    assert_eq!(
                        *addend, 0,
                        "Call addend should be 0 (field offset), got {}",
                        addend
                    );
                    found_call_with_mem_rip_rel = true;
                    if cfg!(debug_assertions) {
                        eprintln!(
                            "[pa_r17_015 IR test] Found Call with MemRipRelSym at node_id={}: name={}, addend={}",
                            node_id.get(),
                            name,
                            addend
                        );
                    }
                }
            }
        }

        // Check for the intermediate mov r11, ... that should NOT appear
        if inst.mnemonic == Mnemonic::Mov
            && inst.operands.len() >= 2
            && inst.operands[0] == Operand::Reg(abi::R11)
        {
            // Mov to R11 should NOT appear in the mem_rip_sym path
            found_intermediate_mov_r11 = true;
            eprintln!(
                "[pa_r17_015 IR test] ERROR: Found unexpected mov r11 at node_id={}",
                node_id.get()
            );
        }
    }

    assert!(
        found_call_with_mem_rip_rel,
        "Expected Call with Operand::MemRipRelSym, but none was emitted"
    );
    assert!(
        !found_intermediate_mov_r11,
        "Unexpected mov r11 instruction found; emit_indirect_call_via_mem_rip_sym should not save to R11"
    );
}

#[test]
fn emit_indirect_call_via_mem_rip_sym_with_field_offset() {
    // Verify that a non-zero field offset is correctly passed as the addend.
    //
    // IR:
    //   Lambda { body: App { callee: FieldAccess(Var("vtable")), args: [...] } }
    //   where FieldAccess refers to field_index=2 (offset 16 bytes in a 3-field struct)

    let mut arena = IrArena::new();

    // Receiver Var("vtable")
    let receiver_var_id = arena.alloc(IrKind::Var, span());
    arena
        .binding_names_mut()
        .insert(receiver_var_id, "vtable".to_string());

    // FieldAccess(receiver)
    let field_access_id = arena.alloc_with_children(IrKind::FieldAccess, span(), [receiver_var_id]);

    // FieldAccessInfo: field_index=2 (3rd field)
    let field_type_id = RecordTypeId(200);
    let field_info = FieldAccessInfo {
        type_id: field_type_id,
        field_index: 2,
    };
    arena
        .field_access_info_mut()
        .insert(field_access_id, field_info);

    // Argument (no-op, just satisfying the structure)
    let arg_lit_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(arg_lit_id, 42);

    // App(field_access, arg)
    let app_id = arena.alloc_with_children(IrKind::App, span(), [field_access_id, arg_lit_id]);

    // Lambda(app)
    let _lambda_id = arena.alloc_with_children(IrKind::Lambda, span(), [app_id]);

    // Register module symbol
    arena.symbols_mut().insert(Symbol::new(
        "vtable".to_string(),
        SymbolKind::Object,
        receiver_var_id,
    ));

    // Register layout with 3 fields: [u64 at 0, u32 at 8, u64 at 16]
    let layout = RecordLayout::new(
        24,
        8,
        vec![
            FieldLayout {
                offset: 0,
                size: 8,
                signed: false,
            },
            FieldLayout {
                offset: 8,
                size: 4,
                signed: false,
            },
            FieldLayout {
                offset: 16,
                size: 8,
                signed: false,
            },
        ],
    );

    let mut walker = EmitWalker::new();
    walker
        .state_mut()
        .insert_record_layout(field_type_id, layout);
    walker.walk(&mut arena);

    // Find the Call instruction and verify addend matches field offset
    let instructions = walker.state().instructions();
    let mut found_correct_addend = false;

    for (_, inst) in instructions.iter() {
        if inst.mnemonic == Mnemonic::Call && inst.operands.len() >= 1 {
            if let Operand::MemRipRelSym { name, addend } = &inst.operands[0] {
                assert_eq!(name, "vtable");
                assert_eq!(
                    *addend, 16,
                    "Field offset should be 16 (3rd field), got {}",
                    addend
                );
                found_correct_addend = true;
            }
        }
    }

    assert!(
        found_correct_addend,
        "Expected Call with addend=16 (field offset), not found"
    );
}
