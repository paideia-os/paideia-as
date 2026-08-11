//! Regression test for #1162: App RHS handling in emit_block_body_arm.
//!
//! Verifies that Let bindings with App (function call) RHS are properly
//! handled within a match arm body, emitting CALL + MOV for result materialization.
//!
//! Structure:
//! - Lambda (outer)
//!   - Action (outer block, walked by emit_block_body)
//!     - [0] Branch (final expression)
//!       - [0] cond_literal (condition)
//!       - [1] Action (then arm body, walked by emit_block_body_arm)
//!         - [0] Let (binding x = add_two(3, 4))
//!           - [0] name_var (Var for "x")
//!           - [1] App (inner_app_id)
//!             - [0] callee (Var "callee_add_two")
//!             - [1] arg0 (Literal 3)
//!             - [2] arg1 (Literal 4)
//!         - [1] tail_var_id (Var "x" — final expression)

use paideia_as_ir::{IrArena, IrKind, CallMeta, Symbol, SymbolKind};
use paideia_as_ir::instruction::Mnemonic;
use paideia_as_diagnostics::Span;
use paideia_as_elaborator::EmitWalker;

fn span() -> Span {
    Span::new(paideia_as_diagnostics::FileId::new(1).unwrap(), 0, 1)
}

#[test]
fn let_app_in_arm_emits_call_and_result_materialization() {
    let mut arena = IrArena::new();

    // 1. Two Literal arg nodes
    let arg0_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(arg0_id, 3);
    let arg1_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(arg1_id, 4);

    // 2. Var callee node named "callee_add_two"
    let callee_id = arena.alloc(IrKind::Var, span());
    arena
        .binding_names_mut()
        .insert(callee_id, "callee_add_two".to_string());

    // 3. App node with children [callee, arg0, arg1]
    let inner_app_id = arena.alloc_with_children(
        IrKind::App,
        span(),
        [callee_id, arg0_id, arg1_id],
    );

    // Register call_sites for the App
    arena.call_sites_mut().insert(
        inner_app_id,
        CallMeta {
            callee_name: "callee_add_two".to_string(),
            arg_count: 2,
            is_intrinsic: false,
        },
    );

    // 4. Let node with binding_names[let_id] = "x"
    // Children: [name_var, inner_app_id]
    let name_var_id = arena.alloc(IrKind::Var, span());
    let let_id = arena.alloc_with_children(IrKind::Let, span(), [name_var_id, inner_app_id]);
    arena.binding_names_mut().insert(let_id, "x".to_string());

    // 5. Tail Var referencing "x"
    let tail_var_id = arena.alloc(IrKind::Var, span());
    arena
        .binding_names_mut()
        .insert(tail_var_id, "x".to_string());

    // 6. Action node (then arm) with children [let_id, tail_var_id]
    let then_arm_id = arena.alloc_with_children(IrKind::Action, span(), [let_id, tail_var_id]);

    // 7. Condition literal for Branch
    let cond_literal_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(cond_literal_id, 1);

    // 8. Branch with children [cond_literal, then_arm_id]
    let branch_id = arena.alloc_with_children(
        IrKind::Branch,
        span(),
        [cond_literal_id, then_arm_id],
    );

    // 9. Outer Action node with Branch as final expression
    let outer_action_id = arena.alloc_with_children(IrKind::Action, span(), [branch_id]);

    // 10. Wrap in outer Lambda; register Lambda as Function symbol
    let lambda_id = arena.alloc_with_children(IrKind::Lambda, span(), [outer_action_id]);
    let sym = Symbol::new("test_fn".to_string(), SymbolKind::Function, lambda_id);
    arena.symbols_mut().insert(sym);

    // #1260: emit_call_expr now validates that callees resolve to a module
    // symbol, stdlib recipe, or local binding — this test's synthetic
    // arena needs to register callee_add_two so the T0553 undefined-
    // identifier check doesn't reject the call before it emits. Prior to
    // the check the arena was permitted to synthesize a callee name
    // out of thin air; that indulgence is what let #1260's real bug
    // ship as a silent success.
    let callee_sym = Symbol::new("callee_add_two".to_string(), SymbolKind::Function, callee_id);
    arena.symbols_mut().insert(callee_sym);

    // Walk the arena
    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    let state = walker.state();
    let insts = state.instructions().entries();

    // Assertion 1: Call must exist
    assert!(
        insts.values().any(|i| matches!(i.mnemonic, Mnemonic::Call)),
        "CALL instruction must be emitted in arm body with App RHS"
    );

    // Assertion 2: Mov count ≥ 3
    // (arg marshalling to RDI/RSI + materialize rax → scratch + final tail var move to rax)
    let mov_count = insts
        .values()
        .filter(|i| matches!(i.mnemonic, Mnemonic::Mov | Mnemonic::MovSized { .. }))
        .count();
    assert!(
        mov_count >= 3,
        "Expected at least 3 MOV instructions (arg marshalling + result materialization + tail), got {}",
        mov_count
    );

    // Assertion 3: Walker produces zero diagnostics
    let diags = walker.diagnostics();
    assert!(
        diags.is_empty(),
        "Walker must produce zero diagnostics, got {} errors/warnings",
        diags.len()
    );
}
