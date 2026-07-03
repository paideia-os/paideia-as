//! Pure function body shape validation.
//!
//! Validates that pure function bodies (ExprLambda nodes without explicit
//! unsafe declarations) do not contain unsupported control-flow expressions
//! (if/match/while/loop). These expressions require full control-flow lowering
//! and IR branching support, which is not yet available. See issue #913.
//!
//! Emits T0532 diagnostic for each unsupported construct found in a pure lambda body.
//! The diagnostic guides users to wrap unsupported bodies in `unsafe { block: { ... } }`.

use paideia_as_ast::{AstArena, ExprData, NodeId, NodeKind, StmtData};
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, Span};

/// Diagnostic code for unsupported control flow in pure function bodies.
///
/// Error: "pure function body contains {if/match/while/loop} expression which
/// requires full control-flow lowering not yet available. Wrap the body in
/// `unsafe { block: { ... } }` as a workaround."
pub const T_PURE_BODY_UNSUPPORTED_IF: u16 = 532;

/// Construct a DiagnosticCode in the T category.
fn t_code(n: u16) -> DiagnosticCode {
    DiagnosticCode::new(Category::T, Severity::Error, n).expect("valid T code")
}

/// Check all pure function bodies in the AST for unsupported control flow.
///
/// Walks every ExprLambda node in the arena and inspects its body for
/// if/match/while/loop expressions. Stops traversal at ExprUnsafe boundaries
/// (unsafe blocks handle their own lowering).
///
/// # Arguments
///
/// - `arena`: the AST arena to check.
///
/// # Returns
///
/// A vector of T0532 diagnostics, one per unsupported construct found.
#[must_use]
pub fn check_pure_body(arena: &AstArena) -> Vec<Diagnostic> {
    let mut diags = Vec::new();

    // Iterate over all nodes in the arena (1-based indexing).
    for i in 1..=arena.len() as u32 {
        if let Some(node_id) = NodeId::new(i) {
            if let Some(node) = arena.get(node_id) {
                // Only check ExprLambda nodes.
                if node.kind == NodeKind::ExprLambda {
                    // Extract lambda body from ExprData.
                    if let Some(ExprData::Lambda { body, .. }) = arena.expr_data(node_id) {
                        // Walk the lambda's body to look for unsupported constructs.
                        walk_expr(arena, *body, &mut diags);
                    }
                }
            }
        }
    }

    diags
}

/// Walk an expression looking for unsupported control-flow constructs.
///
/// Stops at ExprUnsafe (unsafe blocks handle their own lowering).
/// Emits T0532 for each if/match/while/loop found.
fn walk_expr(arena: &AstArena, node_id: NodeId, diags: &mut Vec<Diagnostic>) {
    if let Some(node) = arena.get(node_id) {
        match node.kind {
            // Unsupported control flow: emit diagnostic
            NodeKind::ExprIf => {
                if let Some(span) = arena.get(node_id).map(|n| n.span) {
                    diags.push(emit_t0532(span, "if"));
                }
                // Continue walking to find nested constructs
                if let Some(ExprData::If {
                    cond,
                    then_block,
                    else_block,
                }) = arena.expr_data(node_id)
                {
                    walk_expr(arena, *cond, diags);
                    walk_expr(arena, *then_block, diags);
                    if let Some(else_id) = else_block {
                        walk_expr(arena, *else_id, diags);
                    }
                }
            }

            NodeKind::ExprMatch => {
                if let Some(span) = arena.get(node_id).map(|n| n.span) {
                    diags.push(emit_t0532(span, "match"));
                }
                // Continue walking to find nested constructs
                if let Some(ExprData::Match {
                    scrutinee,
                    arms,
                }) = arena.expr_data(node_id)
                {
                    walk_expr(arena, *scrutinee, diags);
                    for arm in arms {
                        walk_expr(arena, arm.body, diags);
                        // Also check guard if present
                        if let Some(guard_id) = arm.guard {
                            walk_expr(arena, guard_id, diags);
                        }
                    }
                }
            }

            NodeKind::ExprLoop => {
                // ExprLoop covers both `loop { ... }` and `while cond { ... }`
                if let Some(span) = arena.get(node_id).map(|n| n.span) {
                    // Determine construct name (loop or while) from LoopKind
                    let construct_name = if let Some(ExprData::Loop { kind, .. }) =
                        arena.expr_data(node_id)
                    {
                        match kind {
                            paideia_as_ast::LoopKind::Loop => "loop",
                            paideia_as_ast::LoopKind::While => "while",
                        }
                    } else {
                        "loop"
                    };
                    diags.push(emit_t0532(span, construct_name));
                }
                // Continue walking to find nested constructs
                if let Some(ExprData::Loop { header, body, .. }) = arena.expr_data(node_id) {
                    if let Some(header_id) = header {
                        walk_expr(arena, *header_id, diags);
                    }
                    walk_expr(arena, *body, diags);
                }
            }

            // Stop at unsafe blocks (they handle their own elaboration)
            NodeKind::ExprUnsafe => {
                // Do not descend into unsafe blocks
            }

            // Recursively descend through other expressions
            NodeKind::ExprLiteral
            | NodeKind::ExprPath
            | NodeKind::Ident
            | NodeKind::Placeholder => {
                // Leaf nodes or simple idents, no children to walk
            }

            NodeKind::ExprPrefix => {
                if let Some(ExprData::Prefix { expr, .. }) = arena.expr_data(node_id) {
                    walk_expr(arena, *expr, diags);
                }
            }

            NodeKind::ExprPostfix => {
                if let Some(ExprData::Postfix { expr, .. }) = arena.expr_data(node_id) {
                    walk_expr(arena, *expr, diags);
                }
            }

            NodeKind::ExprInfix => {
                if let Some(ExprData::Infix { lhs, rhs, .. }) = arena.expr_data(node_id) {
                    walk_expr(arena, *lhs, diags);
                    walk_expr(arena, *rhs, diags);
                }
            }

            NodeKind::ExprCall => {
                if let Some(ExprData::Call { callee, args }) = arena.expr_data(node_id) {
                    walk_expr(arena, *callee, diags);
                    for arg_id in args {
                        walk_expr(arena, *arg_id, diags);
                    }
                }
            }

            NodeKind::ExprBlock => {
                if let Some(ExprData::Block { stmts, tail, .. }) = arena.expr_data(node_id) {
                    for stmt_id in stmts {
                        walk_stmt(arena, *stmt_id, diags);
                    }
                    if let Some(tail_id) = tail {
                        walk_expr(arena, *tail_id, diags);
                    }
                }
            }

            NodeKind::ExprLambda => {
                // Nested lambda: walk its body but don't double-check as top-level.
                // (The outer loop will handle it separately, but we descend here
                // to catch nested lambdas in argument positions.)
                if let Some(ExprData::Lambda { body, .. }) = arena.expr_data(node_id) {
                    walk_expr(arena, *body, diags);
                }
            }

            NodeKind::ExprCast => {
                if let Some(ExprData::Cast { expr, .. }) = arena.expr_data(node_id) {
                    walk_expr(arena, *expr, diags);
                }
            }

            NodeKind::ExprBorrow => {
                if let Some(ExprData::Borrow { expr, .. }) = arena.expr_data(node_id) {
                    walk_expr(arena, *expr, diags);
                }
            }

            NodeKind::ExprDeref => {
                if let Some(ExprData::Deref { expr }) = arena.expr_data(node_id) {
                    walk_expr(arena, *expr, diags);
                }
            }

            NodeKind::ExprFieldAccess => {
                if let Some(ExprData::FieldAccess { receiver, .. }) = arena.expr_data(node_id) {
                    walk_expr(arena, *receiver, diags);
                }
            }

            NodeKind::ExprArrayLit => {
                if let Some(ExprData::ArrayLit(elements)) = arena.expr_data(node_id) {
                    for elem_id in elements {
                        walk_expr(arena, *elem_id, diags);
                    }
                }
            }

            NodeKind::ExprArrayRepeat => {
                if let Some(ExprData::ArrayRepeat { expr, .. }) = arena.expr_data(node_id) {
                    walk_expr(arena, *expr, diags);
                }
            }

            NodeKind::ExprRecordCons => {
                if let Some(ExprData::RecordCons { fields, .. }) = arena.expr_data(node_id) {
                    for (_, field_expr) in fields {
                        walk_expr(arena, *field_expr, diags);
                    }
                }
            }

            NodeKind::ExprActionBlock => {
                if let Some(ExprData::ActionBlock { body, .. }) = arena.expr_data(node_id) {
                    for stmt_id in body {
                        walk_stmt(arena, *stmt_id, diags);
                    }
                }
            }

            NodeKind::ExprWithHandler => {
                if let Some(ExprData::WithHandler {
                    handler,
                    block,
                    finally,
                    ..
                }) = arena.expr_data(node_id)
                {
                    walk_expr(arena, *handler, diags);
                    walk_expr(arena, *block, diags);
                    if let Some(finally_id) = finally {
                        walk_expr(arena, *finally_id, diags);
                    }
                }
            }

            NodeKind::ExprPerform => {
                if let Some(ExprData::Perform { args, .. }) = arena.expr_data(node_id) {
                    for arg_id in args {
                        walk_expr(arena, *arg_id, diags);
                    }
                }
            }

            NodeKind::ExprResume => {
                if let Some(ExprData::Resume { value }) = arena.expr_data(node_id) {
                    walk_expr(arena, *value, diags);
                }
            }

            NodeKind::ExprHandlerValue => {
                if let Some(ExprData::HandlerValue { arms, .. }) = arena.expr_data(node_id) {
                    for arm in arms {
                        match arm {
                            paideia_as_ast::HandlerArm::Op { handler, .. } => {
                                walk_expr(arena, *handler, diags);
                            }
                            paideia_as_ast::HandlerArm::Finally { cleanup } => {
                                walk_expr(arena, *cleanup, diags);
                            }
                        }
                    }
                }
            }

            NodeKind::ExprQuote => {
                if let Some(ExprData::Quote { body }) = arena.expr_data(node_id) {
                    walk_expr(arena, *body, diags);
                }
            }

            NodeKind::ExprAntiquote => {
                if let Some(ExprData::Antiquote { value }) = arena.expr_data(node_id) {
                    walk_expr(arena, *value, diags);
                }
            }

            NodeKind::ExprFunctorApp => {
                if let Some(ExprData::FunctorApp { arguments, .. }) = arena.expr_data(node_id) {
                    for arg_id in arguments {
                        walk_expr(arena, *arg_id, diags);
                    }
                }
            }

            NodeKind::ExprPack => {
                if let Some(ExprData::Pack { module_path, .. }) = arena.expr_data(node_id) {
                    walk_expr(arena, *module_path, diags);
                }
            }

            NodeKind::ExprUnpack => {
                if let Some(ExprData::Unpack { value }) = arena.expr_data(node_id) {
                    walk_expr(arena, *value, diags);
                }
            }

            NodeKind::ExprLetModule => {
                if let Some(ExprData::LetModule { body, rest, .. }) = arena.expr_data(node_id) {
                    walk_expr(arena, *body, diags);
                    walk_expr(arena, *rest, diags);
                }
            }

            NodeKind::ExprFor => {
                if let Some(ExprData::For {
                    iterable,
                    body,
                    ..
                }) = arena.expr_data(node_id)
                {
                    walk_expr(arena, *iterable, diags);
                    walk_expr(arena, *body, diags);
                }
            }

            NodeKind::ExprString | NodeKind::ExprByteString => {
                // Leaf nodes
            }

            NodeKind::ExprBreak | NodeKind::ExprContinue => {
                // No children
            }

            // Items and other non-expression nodes should not appear here
            // but we defensively skip them.
            _ => {}
        }
    }
}

/// Walk a statement looking for unsupported control-flow constructs.
fn walk_stmt(arena: &AstArena, node_id: NodeId, diags: &mut Vec<Diagnostic>) {
    if let Some(node) = arena.get(node_id) {
        match node.kind {
            NodeKind::StmtLet => {
                // Let bindings: walk the initializer expression
                if let Some(StmtData::Let { value, .. }) = arena.stmt_data(node_id) {
                    walk_expr(arena, *value, diags);
                }
            }

            NodeKind::StmtExpr => {
                // Expression statement: walk the expression
                if let Some(StmtData::Expr { expr }) = arena.stmt_data(node_id) {
                    walk_expr(arena, *expr, diags);
                }
            }

            NodeKind::StmtReturn => {
                // Return statement: walk the return value
                if let Some(StmtData::Return { value }) = arena.stmt_data(node_id) {
                    if let Some(value_id) = value {
                        walk_expr(arena, *value_id, diags);
                    }
                }
            }

            NodeKind::StmtInstruction => {
                // Instruction statements: no expressions to walk
            }

            NodeKind::StmtLabel => {
                // Label statements: no expressions to walk
            }

            _ => {
                // Defensively skip other statement types
            }
        }
    }
}

fn emit_t0532(span: Span, construct: &str) -> Diagnostic {
    Diagnostic::error(t_code(T_PURE_BODY_UNSUPPORTED_IF))
        .message(format!(
            "pure function body contains `{construct}` expression which requires \
             full control-flow lowering not yet available. Wrap the body in \
             `unsafe {{ block: {{ ... }} }}` as a workaround."
        ))
        .with_span(span)
        .finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::FileId;

    fn span(start: u32) -> Span {
        Span::new(FileId::new(1).unwrap(), start, 1)
    }

    /// Test 1: pure fn with if → 1 diagnostic
    #[test]
    fn pure_fn_with_if_emits_t0532() {
        let mut arena = AstArena::new();

        // Create if expression node
        let if_id = arena.alloc(NodeKind::ExprIf, span(20));
        let if_data = ExprData::If {
            cond: if_id,
            then_block: if_id,
            else_block: None,
        };
        arena.alloc_expr(NodeKind::ExprIf, span(20), if_data);

        // Create lambda with if body
        arena.alloc_expr(
            NodeKind::ExprLambda,
            span(10),
            ExprData::Lambda {
                generic_params: vec![],
                params: vec![],
                body: if_id,
                pipe_form: false,
            },
        );

        let diags = check_pure_body(&arena);
        assert!(
            diags.len() >= 1,
            "Expected at least 1 diagnostic, got {}",
            diags.len()
        );
        assert_eq!(diags[0].code().number(), T_PURE_BODY_UNSUPPORTED_IF);
    }

    /// Test 2: unsafe { block: { if ... } } → 0 diagnostics (stops at unsafe)
    #[test]
    fn unsafe_block_with_if_ok() {
        let mut arena = AstArena::new();

        // Create if expression node
        let if_id = arena.alloc(NodeKind::ExprIf, span(30));

        // Create unsafe block as lambda body
        let unsafe_data = ExprData::Unsafe {
            effects: vec![],
            capabilities: vec![],
            justification: if_id, // reuse as placeholder
            block: vec![if_id], // if is in block, should be ignored
        };
        let unsafe_id = arena.alloc_expr(NodeKind::ExprUnsafe, span(20), unsafe_data);

        // Create lambda with unsafe body
        arena.alloc_expr(
            NodeKind::ExprLambda,
            span(10),
            ExprData::Lambda {
                generic_params: vec![],
                params: vec![],
                body: unsafe_id,
                pipe_form: false,
            },
        );

        let diags = check_pure_body(&arena);
        assert_eq!(
            diags.len(),
            0,
            "unsafe blocks should not trigger T0532, but got {} diagnostics",
            diags.len()
        );
    }

    /// Test 3: match also fires
    #[test]
    fn match_triggers_t0532() {
        let mut arena = AstArena::new();

        // Create match node as lambda body
        let match_id = arena.alloc(NodeKind::ExprMatch, span(20));
        let match_data = ExprData::Match {
            scrutinee: match_id,
            arms: vec![],
        };
        arena.alloc_expr(NodeKind::ExprMatch, span(20), match_data);

        // Create lambda with match body
        arena.alloc_expr(
            NodeKind::ExprLambda,
            span(10),
            ExprData::Lambda {
                generic_params: vec![],
                params: vec![],
                body: match_id,
                pipe_form: false,
            },
        );

        let diags = check_pure_body(&arena);
        assert!(
            diags.len() >= 1,
            "Expected at least 1 diagnostic for match, got {}",
            diags.len()
        );
        assert_eq!(diags[0].code().number(), T_PURE_BODY_UNSUPPORTED_IF);
    }

    /// Test 4: loop also fires
    #[test]
    fn loop_triggers_t0532() {
        let mut arena = AstArena::new();

        // Create loop node as lambda body
        let loop_id = arena.alloc(NodeKind::ExprLoop, span(20));
        let loop_data = ExprData::Loop {
            kind: paideia_as_ast::LoopKind::Loop,
            header: None,
            body: loop_id,
        };
        arena.alloc_expr(NodeKind::ExprLoop, span(20), loop_data);

        // Create lambda with loop body
        arena.alloc_expr(
            NodeKind::ExprLambda,
            span(10),
            ExprData::Lambda {
                generic_params: vec![],
                params: vec![],
                body: loop_id,
                pipe_form: false,
            },
        );

        let diags = check_pure_body(&arena);
        assert!(
            diags.len() >= 1,
            "Expected at least 1 diagnostic for loop, got {}",
            diags.len()
        );
        assert_eq!(diags[0].code().number(), T_PURE_BODY_UNSUPPORTED_IF);
    }

    /// Test 5: while also fires
    #[test]
    fn while_triggers_t0532() {
        let mut arena = AstArena::new();

        // Create while loop node as lambda body
        let loop_id = arena.alloc(NodeKind::ExprLoop, span(20));
        let loop_data = ExprData::Loop {
            kind: paideia_as_ast::LoopKind::While,
            header: Some(loop_id), // condition
            body: loop_id,
        };
        arena.alloc_expr(NodeKind::ExprLoop, span(20), loop_data);

        // Create lambda with while body
        arena.alloc_expr(
            NodeKind::ExprLambda,
            span(10),
            ExprData::Lambda {
                generic_params: vec![],
                params: vec![],
                body: loop_id,
                pipe_form: false,
            },
        );

        let diags = check_pure_body(&arena);
        assert!(
            diags.len() >= 1,
            "Expected at least 1 diagnostic for while, got {}",
            diags.len()
        );
        assert_eq!(diags[0].code().number(), T_PURE_BODY_UNSUPPORTED_IF);
    }
}
