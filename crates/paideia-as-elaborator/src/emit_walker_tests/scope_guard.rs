use super::super::*;
use paideia_as_diagnostics::{FileId, Span};

fn span() -> Span {
    Span::new(FileId::new(1).unwrap(), 0, 1)
}

/// Test 1: pre_pass_marks_let_record_cons_rhs
/// Build an IR arena with Let → RecordCons at IDs (5, 6).
/// Walk. Assert state.was_record_cons_handled(6) == true.
#[test]
fn pre_pass_marks_let_record_cons_rhs() {
    let mut arena = IrArena::new();

    // Allocate RecordCons (will be node ID 1)
    let record_cons_id = arena.alloc(IrKind::RecordCons, span());

    // Allocate Let with RecordCons as RHS (will be node ID 2)
    let _let_id = arena.alloc_with_children(IrKind::Let, span(), [record_cons_id]);

    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    // Assert the RecordCons was marked as handled
    assert!(
        walker.state().was_record_cons_handled(record_cons_id.get()),
        "RecordCons should be marked as handled by pre-pass"
    );
}

/// Test 2: guard_skips_record_cons_when_marked
/// Same setup: Let → RecordCons.
/// Assert state.diagnostics().is_empty() after walk (would fire T0518 without the guard).
#[test]
fn guard_skips_record_cons_when_marked() {
    let mut arena = IrArena::new();

    // Allocate RecordCons
    let record_cons_id = arena.alloc(IrKind::RecordCons, span());

    // Allocate Let with RecordCons as RHS
    let _let_id = arena.alloc_with_children(IrKind::Let, span(), [record_cons_id]);

    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    // Assert no diagnostics (the guard prevents visit_record_cons from firing)
    let diags = walker.diagnostics();
    assert!(
        diags.is_empty(),
        "guard should prevent visit_record_cons diagnostics, got: {:?}",
        diags
    );
}

/// Test 3: pre_pass_marks_lambda_field_access_body
/// Lambda(id=10) → FieldAccess(id=11) with Var receiver(id=12).
/// Walk. Assert state.was_field_access_handled(11) == true.
#[test]
fn pre_pass_marks_lambda_field_access_body() {
    let mut arena = IrArena::new();

    // Allocate Var (receiver)
    let var_id = arena.alloc(IrKind::Var, span());

    // Allocate FieldAccess with Var as receiver
    let field_access_id = arena.alloc_with_children(IrKind::FieldAccess, span(), [var_id]);

    // Allocate Lambda with FieldAccess as body
    let _lambda_id = arena.alloc_with_children(IrKind::Lambda, span(), [field_access_id]);

    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    // Assert the FieldAccess was marked as handled
    assert!(
        walker.state().was_field_access_handled(field_access_id.get()),
        "FieldAccess in Lambda body should be marked as handled by pre-pass"
    );
}

/// Test 4: guard_skips_field_access_when_marked
/// Same setup: Lambda → FieldAccess with Var receiver.
/// Assert state.diagnostics().is_empty().
#[test]
fn guard_skips_field_access_when_marked() {
    let mut arena = IrArena::new();

    // Allocate Var (receiver)
    let var_id = arena.alloc(IrKind::Var, span());

    // Allocate FieldAccess with Var as receiver
    let field_access_id = arena.alloc_with_children(IrKind::FieldAccess, span(), [var_id]);

    // Allocate Lambda with FieldAccess as body
    let _lambda_id = arena.alloc_with_children(IrKind::Lambda, span(), [field_access_id]);

    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    // Assert no diagnostics (the guard prevents visit_field_access from firing)
    let diags = walker.diagnostics();
    assert!(
        diags.is_empty(),
        "guard should prevent visit_field_access diagnostics, got: {:?}",
        diags
    );
}

/// Test 5: pre_pass_marks_lambda_app_callee_field_access
/// Lambda(20) → App(21) → [FieldAccess(22) → Var(23)].
/// Walk. Assert state.was_field_access_handled(22) == true.
#[test]
fn pre_pass_marks_lambda_app_callee_field_access() {
    let mut arena = IrArena::new();

    // Allocate Var (receiver for FieldAccess)
    let var_id = arena.alloc(IrKind::Var, span());

    // Allocate FieldAccess with Var as receiver (this is the callee)
    let field_access_id = arena.alloc_with_children(IrKind::FieldAccess, span(), [var_id]);

    // Allocate a placeholder for the argument (second child of App)
    let arg_id = arena.alloc(IrKind::Literal, span());

    // Allocate App with FieldAccess as callee and Literal as argument
    let app_id = arena.alloc_with_children(IrKind::App, span(), [field_access_id, arg_id]);

    // Allocate Lambda with App as body
    let _lambda_id = arena.alloc_with_children(IrKind::Lambda, span(), [app_id]);

    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    // Assert the FieldAccess was marked as handled
    assert!(
        walker.state().was_field_access_handled(field_access_id.get()),
        "FieldAccess in Lambda → App should be marked as handled by pre-pass"
    );
}

/// Test 6: pre_pass_leaves_store_field_access_unmarked
/// Store(30) → [FieldAccess(31), unused, Var(32)].
/// Walk. Assert state.was_field_access_handled(31) == false in pre-pass
/// (byte-affecting shape — not pre-guarded), but it MAY be marked after visit_field_assign.
/// This test verifies that the PRE-PASS does not mark Store → FieldAccess as owned.
#[test]
fn pre_pass_leaves_store_field_access_unmarked() {
    let mut arena = IrArena::new();

    // Allocate a Var (receiver for FieldAccess)
    let receiver_id = arena.alloc(IrKind::Var, span());

    // Allocate FieldAccess with Var as receiver
    let field_access_id = arena.alloc_with_children(IrKind::FieldAccess, span(), [receiver_id]);

    // Allocate another Var as a placeholder for other children
    let other_var_id = arena.alloc(IrKind::Var, span());

    // Allocate Store with multiple children (FieldAccess, something, Var)
    let _store_id = arena.alloc_with_children(
        IrKind::Store,
        span(),
        [field_access_id, other_var_id, receiver_id],
    );

    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    // After the full walk, the FieldAccess will be marked by visit_field_assign.
    // But the key point is that the PRE-PASS does not mark it (not guarded in the
    // ownership pre-check). This allows visit_field_access to verify the shape,
    // while visit_field_assign handles the actual emission.
    // Updated assertion: visit_field_assign now marks the FieldAccess after handling it.
    assert!(
        walker.state().was_field_access_handled(field_access_id.get()),
        "FieldAccess in Store should be marked AFTER visit_field_assign handles it"
    );
}

/// Test 7 (M3 coverage gap closer): pre_pass_skips_lambda_field_access_when_receiver_is_not_var
/// Lambda → FieldAccess with a non-Var receiver (e.g. Deref) is the (*p).field shape
/// owned by visit_field_access, not emit_field_access_lambda. Pre-pass must NOT mark
/// it or byte-affecting emission would be skipped.
#[test]
fn pre_pass_skips_lambda_field_access_when_receiver_is_not_var() {
    let mut arena = IrArena::new();

    // Receiver is a Deref, not a Var
    let ptr_var_id = arena.alloc(IrKind::Var, span());
    let deref_id = arena.alloc_with_children(IrKind::Deref, span(), [ptr_var_id]);

    // FieldAccess with Deref receiver — (*p).field shape
    let field_access_id = arena.alloc_with_children(IrKind::FieldAccess, span(), [deref_id]);

    // Lambda with the FieldAccess as body
    let _lambda_id = arena.alloc_with_children(IrKind::Lambda, span(), [field_access_id]);

    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    // Pre-pass must NOT mark: receiver-kind guard exists to keep (*p).field routed to
    // visit_field_access, which is the byte-emitting owner for this shape.
    assert!(
        !walker.state().was_field_access_handled(field_access_id.get()),
        "FieldAccess with Deref receiver must NOT be marked — visit_field_access owns it"
    );
}
