//! Integration test for AST: building and walking expression trees.
//!
//! This test constructs a small expression tree (a Lambda containing a Block
//! containing two Stmts), then walks it with custom visitors to verify the
//! visitor dispatch mechanism.

use paideia_as_ast::{
    AstArena, ExprData, ExprVisitor, LoopKind, NodeKind, PatternData, PatternVisitor, StmtData,
    StmtVisitor, TypeData, TypeVisitor, walk_expr, walk_pattern, walk_stmt, walk_type,
};
use paideia_as_diagnostics::{FileId, Span};

fn span() -> Span {
    Span::new(FileId::new(1).unwrap(), 0, 1)
}

/// Visitor that counts nodes of each kind.
struct CountingVisitor {
    expr_count: usize,
    stmt_count: usize,
    type_count: usize,
    pattern_count: usize,
}

impl ExprVisitor for CountingVisitor {
    fn visit_expr_lambda(&mut self, arena: &paideia_as_ast::AstArena, id: paideia_as_ast::NodeId) {
        self.expr_count += 1;
        // Recurse into body
        if let Some(lambda_data) = arena.expr_data(id)
            && let ExprData::Lambda { body, .. } = lambda_data
        {
            walk_expr(self, arena, *body);
        }
    }

    fn visit_expr_block(&mut self, arena: &paideia_as_ast::AstArena, id: paideia_as_ast::NodeId) {
        self.expr_count += 1;
        // Recurse into statements
        if let Some(block_data) = arena.expr_data(id)
            && let ExprData::Block { stmts, tail, .. } = block_data
        {
            for &stmt_id in stmts {
                walk_stmt(self, arena, stmt_id);
            }
            if let Some(tail_id) = tail {
                walk_expr(self, arena, *tail_id);
            }
        }
    }
}

impl StmtVisitor for CountingVisitor {
    fn visit_stmt_expr(&mut self, arena: &paideia_as_ast::AstArena, id: paideia_as_ast::NodeId) {
        self.stmt_count += 1;
        // Recurse into the expression
        if let Some(stmt_data) = arena.stmt_data(id)
            && let StmtData::Expr { expr } = stmt_data
        {
            walk_expr(self, arena, *expr);
        }
    }

    fn visit_stmt_let(&mut self, arena: &paideia_as_ast::AstArena, id: paideia_as_ast::NodeId) {
        self.stmt_count += 1;
        // Recurse into the value expression
        if let Some(stmt_data) = arena.stmt_data(id)
            && let StmtData::Let { value, .. } = stmt_data
        {
            walk_expr(self, arena, *value);
        }
    }
}

impl TypeVisitor for CountingVisitor {
    fn visit_type_name(&mut self, _arena: &paideia_as_ast::AstArena, _id: paideia_as_ast::NodeId) {
        self.type_count += 1;
    }
}

impl PatternVisitor for CountingVisitor {
    fn visit_pattern_wildcard(
        &mut self,
        _arena: &paideia_as_ast::AstArena,
        _id: paideia_as_ast::NodeId,
    ) {
        self.pattern_count += 1;
    }
}

#[test]
fn build_and_walk_simple_expression_tree() {
    // Build: lambda body contains a block with two statement-expressions
    let mut arena = AstArena::new();

    // Create two literal expressions for the statements
    let lit1 = arena.alloc(NodeKind::Placeholder, span());
    let lit2 = arena.alloc(NodeKind::Placeholder, span());

    // Create two statement-expressions
    let stmt1_id = arena.alloc_stmt(NodeKind::StmtExpr, span(), StmtData::Expr { expr: lit1 });
    let stmt2_id = arena.alloc_stmt(NodeKind::StmtExpr, span(), StmtData::Expr { expr: lit2 });

    // Create a block containing the two statements
    let block_id = arena.alloc_expr(
        NodeKind::ExprBlock,
        span(),
        ExprData::Block {
            stmts: vec![stmt1_id, stmt2_id],
            tail: None,
        },
    );

    // Create a lambda expression with the block as body
    let lambda_id = arena.alloc_expr(
        NodeKind::ExprLambda,
        span(),
        ExprData::Lambda {
            generic_params: vec![],
            params: vec![],
            body: block_id,
            pipe_form: false,
        },
    );

    // Walk the tree with a counting visitor
    let mut visitor = CountingVisitor {
        expr_count: 0,
        stmt_count: 0,
        type_count: 0,
        pattern_count: 0,
    };
    walk_expr(&mut visitor, &arena, lambda_id);

    // Verify counts: 1 lambda + 1 block + 2 stmt-exprs
    assert_eq!(visitor.expr_count, 2, "Should visit lambda and block");
    assert_eq!(
        visitor.stmt_count, 2,
        "Should visit two statement-expressions"
    );
}

#[test]
fn build_and_walk_type_and_pattern() {
    let mut arena = AstArena::new();

    // Build a simple type: i32
    let name_id = arena.alloc(NodeKind::Ident, span());
    let type_id = arena.alloc_type(
        NodeKind::TypeName,
        span(),
        TypeData::Name {
            name: name_id,
            args: vec![],
        },
    );

    // Build a simple pattern: _
    let pattern_id = arena.alloc_pattern(NodeKind::PatWildcard, span(), PatternData::Wildcard);

    // Walk both
    let mut visitor = CountingVisitor {
        expr_count: 0,
        stmt_count: 0,
        type_count: 0,
        pattern_count: 0,
    };
    walk_type(&mut visitor, &arena, type_id);
    walk_pattern(&mut visitor, &arena, pattern_id);

    // Verify counts
    assert_eq!(visitor.type_count, 1, "Should visit one type");
    assert_eq!(visitor.pattern_count, 1, "Should visit one pattern");
}

#[test]
fn complex_expression_with_loop() {
    let mut arena = AstArena::new();

    // Create a loop body (simple literal placeholder)
    let body_id = arena.alloc(NodeKind::Placeholder, span());

    // Create a loop expression (infinite loop)
    let loop_id = arena.alloc_expr(
        NodeKind::ExprLoop,
        span(),
        ExprData::Loop {
            kind: LoopKind::Loop,
            header: None,
            body: body_id,
        },
    );

    // Verify the loop was created
    assert!(arena.expr_data(loop_id).is_some());
    let loop_data = arena.expr_data(loop_id).unwrap();
    match loop_data {
        ExprData::Loop { kind, .. } => {
            assert_eq!(*kind, LoopKind::Loop);
        }
        _ => panic!("expected loop expression"),
    }
}
