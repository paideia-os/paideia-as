//! Visitor trait and dispatch for item traversal.
//!
//! [`ItemVisitor`] is a trait for traversing item nodes. [`walk_item`]
//! dispatches by node kind to the appropriate visitor method.

use crate::{AstArena, NodeId, NodeKind};

/// Visitor trait for traversing item nodes.
///
/// Implement this trait to perform actions on specific item kinds. Each
/// `visit_*` method receives the arena and the node ID for the item being
/// visited. Implement only the methods you need; defaults are no-ops.
pub trait ItemVisitor {
    /// Visit a Module item.
    fn visit_module(&mut self, _arena: &AstArena, _id: NodeId) {}

    /// Visit a Signature item.
    fn visit_signature(&mut self, _arena: &AstArena, _id: NodeId) {}

    /// Visit a Structure item.
    fn visit_structure(&mut self, _arena: &AstArena, _id: NodeId) {}

    /// Visit a Functor item.
    fn visit_functor(&mut self, _arena: &AstArena, _id: NodeId) {}

    /// Visit a FunctorParam item.
    fn visit_functor_param(&mut self, _arena: &AstArena, _id: NodeId) {}

    /// Visit an Effect item.
    fn visit_effect(&mut self, _arena: &AstArena, _id: NodeId) {}

    /// Visit an OpSig item.
    fn visit_op_sig(&mut self, _arena: &AstArena, _id: NodeId) {}

    /// Visit a Capability item.
    fn visit_capability(&mut self, _arena: &AstArena, _id: NodeId) {}

    /// Visit a Let item.
    fn visit_let(&mut self, _arena: &AstArena, _id: NodeId) {}

    /// Visit a Struct item.
    fn visit_struct(&mut self, _arena: &AstArena, _id: NodeId) {}

    /// Visit an Enum item.
    fn visit_enum(&mut self, _arena: &AstArena, _id: NodeId) {}

    /// Visit an UnsafeBlock item.
    fn visit_unsafe_block(&mut self, _arena: &AstArena, _id: NodeId) {}
}

/// Dispatch visitor call by node kind for items.
///
/// Looks up the node in the arena, checks its kind, and calls the
/// appropriate `visit_*` method on the visitor. Does nothing for
/// non-item kinds.
pub fn walk_item<V: ItemVisitor>(visitor: &mut V, arena: &AstArena, id: NodeId) {
    let node_data = match arena.get(id) {
        Some(nd) => nd,
        None => return,
    };

    match node_data.kind {
        NodeKind::Module => visitor.visit_module(arena, id),
        NodeKind::Signature => visitor.visit_signature(arena, id),
        NodeKind::Structure => visitor.visit_structure(arena, id),
        NodeKind::Functor => visitor.visit_functor(arena, id),
        NodeKind::FunctorParam => visitor.visit_functor_param(arena, id),
        NodeKind::Effect => visitor.visit_effect(arena, id),
        NodeKind::OpSig => visitor.visit_op_sig(arena, id),
        NodeKind::Capability => visitor.visit_capability(arena, id),
        NodeKind::Let => visitor.visit_let(arena, id),
        NodeKind::Struct => visitor.visit_struct(arena, id),
        NodeKind::Enum => visitor.visit_enum(arena, id),
        NodeKind::UnsafeBlock => visitor.visit_unsafe_block(arena, id),
        // Non-item kinds: handled by other visitors.
        _ => {}
    }
}

/// Visitor trait for traversing expression nodes.
///
/// Implement this trait to perform actions on specific expression kinds. Each
/// `visit_*` method receives the arena and the node ID for the expression being
/// visited. Implement only the methods you need; defaults are no-ops.
pub trait ExprVisitor {
    /// Visit a Lambda expression.
    fn visit_expr_lambda(&mut self, _arena: &AstArena, _id: NodeId) {}
    /// Visit an ActionBlock expression.
    fn visit_expr_action_block(&mut self, _arena: &AstArena, _id: NodeId) {}
    /// Visit a WithHandler expression.
    fn visit_expr_with_handler(&mut self, _arena: &AstArena, _id: NodeId) {}
    /// Visit an Unsafe expression.
    fn visit_expr_unsafe(&mut self, _arena: &AstArena, _id: NodeId) {}
    /// Visit an Infix expression.
    fn visit_expr_infix(&mut self, _arena: &AstArena, _id: NodeId) {}
    /// Visit a Prefix expression.
    fn visit_expr_prefix(&mut self, _arena: &AstArena, _id: NodeId) {}
    /// Visit a Postfix expression.
    fn visit_expr_postfix(&mut self, _arena: &AstArena, _id: NodeId) {}
    /// Visit a Literal expression.
    fn visit_expr_literal(&mut self, _arena: &AstArena, _id: NodeId) {}
    /// Visit a Path expression.
    fn visit_expr_path(&mut self, _arena: &AstArena, _id: NodeId) {}
    /// Visit a Call expression.
    fn visit_expr_call(&mut self, _arena: &AstArena, _id: NodeId) {}
    /// Visit a Block expression.
    fn visit_expr_block(&mut self, _arena: &AstArena, _id: NodeId) {}
    /// Visit a Match expression.
    fn visit_expr_match(&mut self, _arena: &AstArena, _id: NodeId) {}
    /// Visit an If expression.
    fn visit_expr_if(&mut self, _arena: &AstArena, _id: NodeId) {}
    /// Visit a Loop expression.
    fn visit_expr_loop(&mut self, _arena: &AstArena, _id: NodeId) {}
    /// Visit a Perform expression.
    fn visit_expr_perform(&mut self, _arena: &AstArena, _id: NodeId) {}
    /// Visit a Resume expression.
    fn visit_expr_resume(&mut self, _arena: &AstArena, _id: NodeId) {}
    /// Visit a HandlerValue expression.
    fn visit_expr_handler_value(&mut self, _arena: &AstArena, _id: NodeId) {}
    /// Visit a Quote expression.
    fn visit_expr_quote(&mut self, _arena: &AstArena, _id: NodeId) {}
    /// Visit an Antiquote expression.
    fn visit_expr_antiquote(&mut self, _arena: &AstArena, _id: NodeId) {}
    /// Visit a FunctorApp expression.
    fn visit_expr_functor_app(&mut self, _arena: &AstArena, _id: NodeId) {}
    /// Visit a Pack expression.
    fn visit_expr_pack(&mut self, _arena: &AstArena, _id: NodeId) {}
    /// Visit an Unpack expression.
    fn visit_expr_unpack(&mut self, _arena: &AstArena, _id: NodeId) {}
    /// Visit a LetModule expression.
    fn visit_expr_let_module(&mut self, _arena: &AstArena, _id: NodeId) {}
    /// Visit a RecordCons expression.
    fn visit_expr_record_cons(&mut self, _arena: &AstArena, _id: NodeId) {}
    /// Visit a FieldAccess expression.
    fn visit_expr_field_access(&mut self, _arena: &AstArena, _id: NodeId) {}
}

/// Dispatch visitor call by node kind for expressions.
pub fn walk_expr<V: ExprVisitor>(visitor: &mut V, arena: &AstArena, id: NodeId) {
    let node_data = match arena.get(id) {
        Some(nd) => nd,
        None => return,
    };

    match node_data.kind {
        NodeKind::ExprLambda => visitor.visit_expr_lambda(arena, id),
        NodeKind::ExprActionBlock => visitor.visit_expr_action_block(arena, id),
        NodeKind::ExprWithHandler => visitor.visit_expr_with_handler(arena, id),
        NodeKind::ExprUnsafe => visitor.visit_expr_unsafe(arena, id),
        NodeKind::ExprInfix => visitor.visit_expr_infix(arena, id),
        NodeKind::ExprPrefix => visitor.visit_expr_prefix(arena, id),
        NodeKind::ExprPostfix => visitor.visit_expr_postfix(arena, id),
        NodeKind::ExprLiteral => visitor.visit_expr_literal(arena, id),
        NodeKind::ExprPath => visitor.visit_expr_path(arena, id),
        NodeKind::ExprCall => visitor.visit_expr_call(arena, id),
        NodeKind::ExprBlock => visitor.visit_expr_block(arena, id),
        NodeKind::ExprMatch => visitor.visit_expr_match(arena, id),
        NodeKind::ExprIf => visitor.visit_expr_if(arena, id),
        NodeKind::ExprLoop => visitor.visit_expr_loop(arena, id),
        NodeKind::ExprPerform => visitor.visit_expr_perform(arena, id),
        NodeKind::ExprResume => visitor.visit_expr_resume(arena, id),
        NodeKind::ExprHandlerValue => visitor.visit_expr_handler_value(arena, id),
        NodeKind::ExprQuote => visitor.visit_expr_quote(arena, id),
        NodeKind::ExprAntiquote => visitor.visit_expr_antiquote(arena, id),
        NodeKind::ExprFunctorApp => visitor.visit_expr_functor_app(arena, id),
        NodeKind::ExprPack => visitor.visit_expr_pack(arena, id),
        NodeKind::ExprUnpack => visitor.visit_expr_unpack(arena, id),
        NodeKind::ExprLetModule => visitor.visit_expr_let_module(arena, id),
        NodeKind::ExprRecordCons => visitor.visit_expr_record_cons(arena, id),
        NodeKind::ExprFieldAccess => visitor.visit_expr_field_access(arena, id),
        _ => {}
    }
}

/// Visitor trait for traversing statement nodes.
pub trait StmtVisitor {
    /// Visit a Let statement.
    fn visit_stmt_let(&mut self, _arena: &AstArena, _id: NodeId) {}
    /// Visit an Expr statement.
    fn visit_stmt_expr(&mut self, _arena: &AstArena, _id: NodeId) {}
    /// Visit a Return statement.
    fn visit_stmt_return(&mut self, _arena: &AstArena, _id: NodeId) {}
    /// Visit an Instruction statement.
    fn visit_stmt_instruction(&mut self, _arena: &AstArena, _id: NodeId) {}
}

/// Dispatch visitor call by node kind for statements.
pub fn walk_stmt<V: StmtVisitor>(visitor: &mut V, arena: &AstArena, id: NodeId) {
    let node_data = match arena.get(id) {
        Some(nd) => nd,
        None => return,
    };

    match node_data.kind {
        NodeKind::StmtLet => visitor.visit_stmt_let(arena, id),
        NodeKind::StmtExpr => visitor.visit_stmt_expr(arena, id),
        NodeKind::StmtReturn => visitor.visit_stmt_return(arena, id),
        NodeKind::StmtInstruction => visitor.visit_stmt_instruction(arena, id),
        _ => {}
    }
}

/// Visitor trait for traversing type nodes.
pub trait TypeVisitor {
    /// Visit a Name type.
    fn visit_type_name(&mut self, _arena: &AstArena, _id: NodeId) {}
    /// Visit an Arrow type.
    fn visit_type_arrow(&mut self, _arena: &AstArena, _id: NodeId) {}
    /// Visit a Tuple type.
    fn visit_type_tuple(&mut self, _arena: &AstArena, _id: NodeId) {}
    /// Visit a LinearClass type.
    fn visit_type_linear_class(&mut self, _arena: &AstArena, _id: NodeId) {}
    /// Visit an EffectRow type.
    fn visit_type_effect_row(&mut self, _arena: &AstArena, _id: NodeId) {}
    /// Visit a Ptr type.
    fn visit_type_ptr(&mut self, _arena: &AstArena, _id: NodeId) {}
    /// Visit a Record type.
    fn visit_type_record(&mut self, _arena: &AstArena, _id: NodeId) {}
}

/// Dispatch visitor call by node kind for types.
pub fn walk_type<V: TypeVisitor>(visitor: &mut V, arena: &AstArena, id: NodeId) {
    let node_data = match arena.get(id) {
        Some(nd) => nd,
        None => return,
    };

    match node_data.kind {
        NodeKind::TypeName => visitor.visit_type_name(arena, id),
        NodeKind::TypeArrow => visitor.visit_type_arrow(arena, id),
        NodeKind::TypeTuple => visitor.visit_type_tuple(arena, id),
        NodeKind::TypeLinearClass => visitor.visit_type_linear_class(arena, id),
        NodeKind::TypeEffectRow => visitor.visit_type_effect_row(arena, id),
        NodeKind::TypePtr => visitor.visit_type_ptr(arena, id),
        NodeKind::TypeRecord => visitor.visit_type_record(arena, id),
        _ => {}
    }
}

/// Visitor trait for traversing pattern nodes.
pub trait PatternVisitor {
    /// Visit a Wildcard pattern.
    fn visit_pattern_wildcard(&mut self, _arena: &AstArena, _id: NodeId) {}
    /// Visit an Ident pattern.
    fn visit_pattern_ident(&mut self, _arena: &AstArena, _id: NodeId) {}
    /// Visit a Literal pattern.
    fn visit_pattern_literal(&mut self, _arena: &AstArena, _id: NodeId) {}
    /// Visit a Tuple pattern.
    fn visit_pattern_tuple(&mut self, _arena: &AstArena, _id: NodeId) {}
    /// Visit a Struct pattern.
    fn visit_pattern_struct(&mut self, _arena: &AstArena, _id: NodeId) {}
    /// Visit an EnumVariant pattern.
    fn visit_pattern_enum_variant(&mut self, _arena: &AstArena, _id: NodeId) {}
    /// Visit an Or pattern.
    fn visit_pattern_or(&mut self, _arena: &AstArena, _id: NodeId) {}
    /// Visit a Binding pattern.
    fn visit_pattern_binding(&mut self, _arena: &AstArena, _id: NodeId) {}
}

/// Dispatch visitor call by node kind for patterns.
pub fn walk_pattern<V: PatternVisitor>(visitor: &mut V, arena: &AstArena, id: NodeId) {
    let node_data = match arena.get(id) {
        Some(nd) => nd,
        None => return,
    };

    match node_data.kind {
        NodeKind::PatWildcard => visitor.visit_pattern_wildcard(arena, id),
        NodeKind::PatIdent => visitor.visit_pattern_ident(arena, id),
        NodeKind::PatLiteral => visitor.visit_pattern_literal(arena, id),
        NodeKind::PatTuple => visitor.visit_pattern_tuple(arena, id),
        NodeKind::PatStruct => visitor.visit_pattern_struct(arena, id),
        NodeKind::PatEnumVariant => visitor.visit_pattern_enum_variant(arena, id),
        NodeKind::PatOr => visitor.visit_pattern_or(arena, id),
        NodeKind::PatBinding => visitor.visit_pattern_binding(arena, id),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::{FileId, Span};

    fn span() -> Span {
        Span::new(FileId::new(1).unwrap(), 0, 1)
    }

    #[test]
    fn visit_module_dispatches() {
        use crate::ItemData;

        let mut arena = AstArena::new();
        let name_id = arena.alloc(NodeKind::Ident, span());
        let body_id = arena.alloc(NodeKind::Structure, span());

        let module_id = arena.alloc_item(
            NodeKind::Module,
            span(),
            ItemData::Module {
                name: name_id,
                sig: None,
                body: body_id,
                doc: None,
            },
        );

        struct CountingVisitor {
            module_count: usize,
        }

        impl ItemVisitor for CountingVisitor {
            fn visit_module(&mut self, _arena: &AstArena, _id: NodeId) {
                self.module_count += 1;
            }
        }

        let mut visitor = CountingVisitor { module_count: 0 };
        walk_item(&mut visitor, &arena, module_id);
        assert_eq!(visitor.module_count, 1);
    }

    #[test]
    fn visit_non_item_is_no_op() {
        struct CountingVisitor {
            call_count: usize,
        }

        impl ItemVisitor for CountingVisitor {
            fn visit_let(&mut self, _arena: &AstArena, _id: NodeId) {
                self.call_count += 1;
            }
        }

        let mut arena = AstArena::new();
        let placeholder_id = arena.alloc(NodeKind::Placeholder, span());

        let mut visitor = CountingVisitor { call_count: 0 };
        walk_item(&mut visitor, &arena, placeholder_id);
        assert_eq!(visitor.call_count, 0); // No dispatch to visit_let
    }

    #[test]
    fn walk_item_ignores_out_of_range_ids() {
        struct CountingVisitor {
            call_count: usize,
        }

        impl ItemVisitor for CountingVisitor {
            fn visit_module(&mut self, _arena: &AstArena, _id: NodeId) {
                self.call_count += 1;
            }
        }

        let arena = AstArena::new();
        let stray_id = NodeId::new(999).unwrap();

        let mut visitor = CountingVisitor { call_count: 0 };
        walk_item(&mut visitor, &arena, stray_id);
        assert_eq!(visitor.call_count, 0); // No panic, no dispatch
    }

    #[test]
    fn walk_expr_dispatches() {
        use crate::ExprData;

        let mut arena = AstArena::new();
        let path_id = arena.alloc(NodeKind::Ident, span());
        let expr_id = arena.alloc_expr(
            NodeKind::ExprPath,
            span(),
            ExprData::Path {
                segments: vec![path_id],
            },
        );

        struct CountingVisitor {
            path_count: usize,
        }

        impl ExprVisitor for CountingVisitor {
            fn visit_expr_path(&mut self, _arena: &AstArena, _id: NodeId) {
                self.path_count += 1;
            }
        }

        let mut visitor = CountingVisitor { path_count: 0 };
        walk_expr(&mut visitor, &arena, expr_id);
        assert_eq!(visitor.path_count, 1);
    }

    #[test]
    fn walk_stmt_dispatches() {
        use crate::StmtData;

        let mut arena = AstArena::new();
        let expr_id = arena.alloc(NodeKind::Placeholder, span());
        let stmt_id =
            arena.alloc_stmt(NodeKind::StmtExpr, span(), StmtData::Expr { expr: expr_id });

        struct CountingVisitor {
            expr_count: usize,
        }

        impl StmtVisitor for CountingVisitor {
            fn visit_stmt_expr(&mut self, _arena: &AstArena, _id: NodeId) {
                self.expr_count += 1;
            }
        }

        let mut visitor = CountingVisitor { expr_count: 0 };
        walk_stmt(&mut visitor, &arena, stmt_id);
        assert_eq!(visitor.expr_count, 1);
    }

    #[test]
    fn walk_type_dispatches() {
        use crate::TypeData;

        let mut arena = AstArena::new();
        let name_id = arena.alloc(NodeKind::Ident, span());
        let type_id = arena.alloc_type(
            NodeKind::TypeName,
            span(),
            TypeData::Name {
                name: name_id,
                args: vec![],
            },
        );

        struct CountingVisitor {
            name_count: usize,
        }

        impl TypeVisitor for CountingVisitor {
            fn visit_type_name(&mut self, _arena: &AstArena, _id: NodeId) {
                self.name_count += 1;
            }
        }

        let mut visitor = CountingVisitor { name_count: 0 };
        walk_type(&mut visitor, &arena, type_id);
        assert_eq!(visitor.name_count, 1);
    }

    #[test]
    fn walk_pattern_dispatches() {
        use crate::PatternData;

        let mut arena = AstArena::new();
        let pat_id = arena.alloc_pattern(NodeKind::PatWildcard, span(), PatternData::Wildcard);

        struct CountingVisitor {
            wildcard_count: usize,
        }

        impl PatternVisitor for CountingVisitor {
            fn visit_pattern_wildcard(&mut self, _arena: &AstArena, _id: NodeId) {
                self.wildcard_count += 1;
            }
        }

        let mut visitor = CountingVisitor { wildcard_count: 0 };
        walk_pattern(&mut visitor, &arena, pat_id);
        assert_eq!(visitor.wildcard_count, 1);
    }
}
