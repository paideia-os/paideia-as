//! #1260: Undefined identifier at let-RHS Call must produce T0553.
//!
//! Prior behavior: `let x = does_not_exist(1, 2)` compiled silently and
//! produced a broken relocation the caller saw as a load-time SIGSEGV.
//! This regression pin drives emit_call_expr with a synthetic arena
//! whose "does_not_exist" callee is NOT registered as a module symbol
//! (nor as a stdlib recipe nor as a local closure binding), and asserts
//! that a T0553 diagnostic surfaces and no CALL instruction is emitted.
//!
//! Mirror-structured with let_app_in_arm.rs (the compatibility sibling
//! that DOES register the callee), so the delta between the two tests
//! is exactly one Symbol registration — the sole compile-time
//! precondition that separates a legal call from a silent miscompile.

use paideia_as_ir::{IrArena, IrKind, CallMeta, Symbol, SymbolKind};
use paideia_as_ir::instruction::Mnemonic;
use paideia_as_diagnostics::{Category, Span};
use paideia_as_elaborator::EmitWalker;

fn span() -> Span {
    Span::new(paideia_as_diagnostics::FileId::new(1).unwrap(), 0, 1)
}

#[test]
fn undefined_callee_at_let_rhs_emits_t0553_and_no_call_instruction() {
    let mut arena = IrArena::new();

    // App: `does_not_exist(3, 4)` where `does_not_exist` is undefined.
    let arg0_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(arg0_id, 3);
    let arg1_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(arg1_id, 4);

    let callee_id = arena.alloc(IrKind::Var, span());
    arena
        .binding_names_mut()
        .insert(callee_id, "does_not_exist".to_string());

    let inner_app_id = arena.alloc_with_children(
        IrKind::App,
        span(),
        [callee_id, arg0_id, arg1_id],
    );

    arena.call_sites_mut().insert(
        inner_app_id,
        CallMeta {
            callee_name: "does_not_exist".to_string(),
            arg_count: 2,
            is_intrinsic: false,
        },
    );

    // Let: `let x = does_not_exist(3, 4)` inside a block, followed by `x`.
    let name_var_id = arena.alloc(IrKind::Var, span());
    let let_id = arena.alloc_with_children(IrKind::Let, span(), [name_var_id, inner_app_id]);
    arena.binding_names_mut().insert(let_id, "x".to_string());

    let tail_var_id = arena.alloc(IrKind::Var, span());
    arena
        .binding_names_mut()
        .insert(tail_var_id, "x".to_string());

    let then_arm_id = arena.alloc_with_children(IrKind::Action, span(), [let_id, tail_var_id]);

    let cond_literal_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(cond_literal_id, 1);

    let branch_id = arena.alloc_with_children(
        IrKind::Branch,
        span(),
        [cond_literal_id, then_arm_id],
    );

    let outer_action_id = arena.alloc_with_children(IrKind::Action, span(), [branch_id]);

    let lambda_id = arena.alloc_with_children(IrKind::Lambda, span(), [outer_action_id]);
    // Register ONLY the enclosing lambda; do NOT register the callee — that's
    // the point of the test.
    let sym = Symbol::new("test_fn".to_string(), SymbolKind::Function, lambda_id);
    arena.symbols_mut().insert(sym);

    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    // Assertion 1: T0553 diagnostic must be emitted (via structured diagnostics).
    let typed_diags = walker.take_typed_diagnostics();
    let t0553_hits: Vec<_> = typed_diags
        .iter()
        .filter(|d| d.code().category() == Category::T && d.code().number() == 553)
        .collect();
    assert_eq!(
        t0553_hits.len(),
        1,
        "expected exactly one T0553 for undefined callee, got {} (all diags: {:?})",
        t0553_hits.len(),
        typed_diags.iter().map(|d| d.code()).collect::<Vec<_>>(),
    );

    // Assertion 2: Diagnostic message names the offending identifier.
    let msg = t0553_hits[0].message();
    assert!(
        msg.contains("does_not_exist"),
        "T0553 message must name the undefined identifier, got: {}",
        msg,
    );

    // Assertion 3: No CALL instruction is emitted for the undefined callee.
    // The check aborts the recipe emission early, so the broken relocation
    // that #1260 originally produced never enters the instruction stream.
    let state = walker.state();
    let insts = state.instructions().entries();
    let has_call = insts.values().any(|i| matches!(i.mnemonic, Mnemonic::Call));
    assert!(
        !has_call,
        "CALL instruction must NOT be emitted when callee is undefined; a T0553 abort was expected",
    );
}
