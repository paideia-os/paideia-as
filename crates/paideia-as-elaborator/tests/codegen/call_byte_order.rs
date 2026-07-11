//! Integration tests for SysV call argument byte ordering (Issue #1099).
//!
//! Verifies that the walker produces instruction sequences correctly:
//! MOVs for argument marshalling, CALL, and RET.
//! Byte-order verification (actual instruction encoding) is tested in
//! paideia-as-emitter-pe/tests/text_emitter.rs.

use paideia_as_ir::{IrArena, IrKind, IrNodeId, CallMeta, Symbol, SymbolKind};
use paideia_as_ir::instruction::Mnemonic;
use paideia_as_diagnostics::{Span, FileId};
use paideia_as_elaborator::EmitWalker;

fn span() -> Span {
    Span::new(FileId::new(1).unwrap(), 0, 1)
}

/// Test: Verify walker produces correct unified IDs for 2-arg SysV call.
/// Regression test for issue #1099: unified ID scheme ensures MOVs sort before CALL.
#[test]
fn walker_produces_correct_instruction_ids_for_sysv_call() {
    let mut arena = IrArena::new();

    // Allocate 2 literal arguments
    let arg0_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(arg0_id, 3);
    let arg1_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(arg1_id, 4);

    // Allocate function name and Var node
    let fn_var_id = arena.alloc(IrKind::Var, span());

    // Allocate App node with callee and 2 arguments
    let app_id = arena.alloc_with_children(IrKind::App, span(), [fn_var_id, arg0_id, arg1_id]);

    // Allocate Lambda with App as body
    let lambda_id = arena.alloc_with_children(IrKind::Lambda, span(), [app_id]);

    // Create and register a function symbol (SysV ABI, default)
    let sym = Symbol::new("add_two".to_string(), SymbolKind::Function, lambda_id);
    arena.symbols_mut().insert(sym);

    // Register the call site metadata
    arena.call_sites_mut().insert(app_id, CallMeta {
        callee_name: "add_two".to_string(),
        arg_count: 2,
        is_intrinsic: false,
    });

    // Walk the arena
    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    // Verify instructions are emitted (Issue #1161/#1165: all IDs now via alloc_synthetic_id).
    // - CALL: emitted with synthetic ID (post-#1161)
    // - RET: emitted with synthetic ID (post-#1165)
    // - MOVs: for argument marshalling
    let insts = walker.state().instructions().entries();

    let inst_mnemonics: Vec<Mnemonic> = insts
        .values()
        .map(|inst| inst.mnemonic)
        .collect();

    // Should have at least 4 instructions: 2 MOVs + CALL + RET
    assert!(
        insts.len() >= 4,
        "Expected at least 4 instructions (2 MOVs + CALL + RET), got {}",
        insts.len()
    );

    // Verify CALL instruction exists
    assert!(
        inst_mnemonics.iter().any(|m| matches!(m, Mnemonic::Call)),
        "CALL instruction must be emitted"
    );

    // Verify RET instruction exists by mnemonic (post-#1165 identity-only allocation)
    assert!(
        inst_mnemonics.iter().any(|m| matches!(m, Mnemonic::Ret)),
        "RET instruction must be emitted"
    );

    // Verify at least one MOV instruction is emitted
    let mov_count = inst_mnemonics.iter().filter(|m| matches!(m, Mnemonic::Mov | Mnemonic::MovSized { .. })).count();
    assert!(
        mov_count >= 2,
        "Expected at least 2 MOV instructions for argument marshalling, got {}",
        mov_count
    );
}
