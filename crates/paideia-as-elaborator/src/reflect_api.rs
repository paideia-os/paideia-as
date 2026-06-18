//! Reflective AST inspection API for macro bodies.
//!
//! This module provides Q-A4 reflection operations on [`Term`] handles,
//! enabling macros to inspect the structure of their arguments.
//!
//! The four operations exposed are:
//! - [`kind`] — return the `TermHead` of a term (Lambda, App, Quote, ...).
//! - [`children`] — return the immediate sub-terms in source order.
//! - [`type_of`] — return the inferred `TypeId` if elaboration has cached it.
//! - [`span`] — return the source-level `Span`.
//!
//! All four are pure and read-only. Macros can call them freely without
//! triggering re-elaboration. The `elab` builtin (m2-006+) is the only call
//! that re-enters the elaborator.
//!
//! # Phase-2-m4 Honesty
//!
//! Macros do not yet execute in Phase-2-m4. The `reflect` module is exposed
//! at the elaborator crate root and is fully accessible to all code. Restricting
//! it to macro-body context is a m2-006/m2-008 concern when macros become real
//! callable bodies. Unit tests can construct `Term` handles directly and call
//! these inspection functions; the "snapshot test on a macro that introspects"
//! is implemented as a deterministic AST serialization test.

use paideia_as_ast::Term;
use paideia_as_ast::reflect::TermHead;
use paideia_as_diagnostics::Span;
use paideia_as_types::TypeId;
use std::collections::HashMap;

/// Wrapper around a side-table mapping `NodeId` → `TypeId`.
///
/// Built by the elaborator's type inference pass. In m2-004 (this PR),
/// the `TypeCache` is a simple HashMap placeholder. Phase-2-m5 will wire
/// it into the full inference engine when reflective expansion lands.
#[derive(Debug, Clone, Default)]
pub struct TypeCache {
    /// Map from AST NodeId (as u32) to inferred TypeId.
    types: HashMap<u32, TypeId>,
}

impl TypeCache {
    /// Construct a new empty type cache.
    pub fn new() -> Self {
        Self {
            types: HashMap::new(),
        }
    }

    /// Insert a type for a given node ID.
    pub fn insert(&mut self, node_id: paideia_as_ast::NodeId, ty: TypeId) {
        self.types.insert(node_id.get(), ty);
    }

    /// Look up the inferred type of a node.
    pub fn get(&self, node_id: paideia_as_ast::NodeId) -> Option<TypeId> {
        self.types.get(&node_id.get()).copied()
    }
}

/// Return the `TermHead` of the supplied term.
///
/// The `TermHead` discriminant covers all expression kinds, operand kinds,
/// and (in Phase-2+) statement and type variants. This is a thin wrapper
/// over [`Term::head`] from m2-001.
///
/// # Example
///
/// ```ignore
/// let arena = AstArena::new();
/// let term = Term::new(&arena, some_quote_node_id);
/// assert_eq!(kind(&term), TermHead::Quote);
/// ```
#[must_use]
pub fn kind<'a>(t: &Term<'a>) -> TermHead {
    t.head()
}

/// Return the immediate sub-terms of `t`, in source order.
///
/// Returns a `Vec<Term<'a>>` of the direct child nodes. For most expression
/// kinds, this covers all syntactically significant sub-expressions. This is
/// a thin wrapper over [`Term::children`] from m2-001, converting the
/// `SmallVec` to a `Vec` for the public API.
///
/// # Example
///
/// ```ignore
/// let arena = AstArena::new();
/// // ... build an App node with a callee and 2 arguments ...
/// let term = Term::new(&arena, app_node_id);
/// let kids = children(&term);
/// assert_eq!(kids.len(), 3); // callee + 2 args
/// ```
#[must_use]
pub fn children<'a>(t: &Term<'a>) -> Vec<Term<'a>> {
    t.children().to_vec()
}

/// Return the inferred type of `t`, if cached.
///
/// Looks up the term's `NodeId` in the supplied `TypeCache`. Returns `Some(TypeId)`
/// if the elaborator has already inferred and cached the type; returns `None` for
/// un-elaborated terms or when the cache has no entry.
///
/// In m2-004, the cache is a simple placeholder. Phase-2-m5 will integrate this
/// with the full type-inference engine so that cached entries accumulate over
/// the elaboration pass.
///
/// # Example
///
/// ```ignore
/// let cache = TypeCache::new();
/// let term = Term::new(&arena, some_node_id);
/// assert_eq!(type_of(&term, &cache), None); // empty cache
/// ```
#[must_use]
pub fn type_of<'a>(t: &Term<'a>, cache: &TypeCache) -> Option<TypeId> {
    cache.get(t.id())
}

/// Return the source span of `t`.
///
/// The span indicates the byte range in the source file where this term
/// appears. Macros can use this to attribute diagnostics back to the
/// call site. This is a thin wrapper over [`Term::span`] from m2-001.
///
/// # Example
///
/// ```ignore
/// let term = Term::new(&arena, some_node_id);
/// let sp = span(&term);
/// println!("term at {}..{}", sp.byte_start(), sp.byte_start() + sp.byte_len());
/// ```
#[must_use]
pub fn span<'a>(t: &Term<'a>) -> Span {
    t.span()
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_ast::{AstArena, ExprData, NodeKind};
    use paideia_as_diagnostics::FileId;

    fn test_span() -> Span {
        Span::new(FileId::new(1).unwrap(), 0, 1)
    }

    #[test]
    fn kind_of_literal_is_literal() {
        let mut arena = AstArena::new();
        let lit_placeholder = arena.alloc(NodeKind::Placeholder, test_span());
        let lit_id = arena.alloc_expr(
            NodeKind::ExprLiteral,
            test_span(),
            ExprData::Literal {
                lit: lit_placeholder,
            },
        );

        let term = Term::new(&arena, lit_id);
        assert_eq!(kind(&term), TermHead::Literal);
    }

    #[test]
    fn children_of_app_returns_callee_plus_args() {
        let mut arena = AstArena::new();

        // Build callee (a simple literal)
        let callee_lit = arena.alloc(NodeKind::Placeholder, test_span());
        let callee_id = arena.alloc_expr(
            NodeKind::ExprLiteral,
            test_span(),
            ExprData::Literal { lit: callee_lit },
        );

        // Build arg1 (a simple literal)
        let arg1_lit = arena.alloc(NodeKind::Placeholder, test_span());
        let arg1_id = arena.alloc_expr(
            NodeKind::ExprLiteral,
            test_span(),
            ExprData::Literal { lit: arg1_lit },
        );

        // Build arg2 (a simple literal)
        let arg2_lit = arena.alloc(NodeKind::Placeholder, test_span());
        let arg2_id = arena.alloc_expr(
            NodeKind::ExprLiteral,
            test_span(),
            ExprData::Literal { lit: arg2_lit },
        );

        // Build the Call expression
        let call_id = arena.alloc_expr(
            NodeKind::ExprCall,
            test_span(),
            ExprData::Call {
                callee: callee_id,
                args: vec![arg1_id, arg2_id],
            },
        );

        let term = Term::new(&arena, call_id);
        let kids = children(&term);

        // Expect: callee + 2 args = 3 children
        assert_eq!(kids.len(), 3);
        assert_eq!(kids[0].id(), callee_id);
        assert_eq!(kids[1].id(), arg1_id);
        assert_eq!(kids[2].id(), arg2_id);

        // Check kinds
        assert_eq!(kind(&kids[0]), TermHead::Literal);
        assert_eq!(kind(&kids[1]), TermHead::Literal);
        assert_eq!(kind(&kids[2]), TermHead::Literal);
    }

    #[test]
    fn type_of_returns_none_when_uncached() {
        let mut arena = AstArena::new();
        let lit_placeholder = arena.alloc(NodeKind::Placeholder, test_span());
        let lit_id = arena.alloc_expr(
            NodeKind::ExprLiteral,
            test_span(),
            ExprData::Literal {
                lit: lit_placeholder,
            },
        );

        let term = Term::new(&arena, lit_id);
        let cache = TypeCache::new();

        assert_eq!(type_of(&term, &cache), None);
    }

    #[test]
    fn type_of_returns_some_when_cached() {
        let mut arena = AstArena::new();
        let lit_placeholder = arena.alloc(NodeKind::Placeholder, test_span());
        let lit_id = arena.alloc_expr(
            NodeKind::ExprLiteral,
            test_span(),
            ExprData::Literal {
                lit: lit_placeholder,
            },
        );

        let mut cache = TypeCache::new();
        // Create a dummy TypeId (TypeId::new(1) is the smallest valid ID)
        let dummy_type_id = TypeId::new(1).unwrap();
        cache.insert(lit_id, dummy_type_id);

        let term = Term::new(&arena, lit_id);
        assert_eq!(type_of(&term, &cache), Some(dummy_type_id));
    }

    #[test]
    fn span_returns_correct_byte_range() {
        let mut arena = AstArena::new();
        let sp = Span::new(FileId::new(1).unwrap(), 10, 5); // byte_start=10, byte_len=5
        let lit_placeholder = arena.alloc(NodeKind::Placeholder, sp);
        let lit_id = arena.alloc_expr(
            NodeKind::ExprLiteral,
            sp,
            ExprData::Literal {
                lit: lit_placeholder,
            },
        );

        let term = Term::new(&arena, lit_id);
        let sp_result = span(&term);

        assert_eq!(sp_result.byte_start(), 10);
        assert_eq!(sp_result.byte_len(), 5);
    }

    #[test]
    fn snapshot_of_introspected_term() {
        let mut arena = AstArena::new();

        // Build: 1 + 2
        // - Literal(1)
        let lit1_placeholder = arena.alloc(
            NodeKind::Placeholder,
            Span::new(FileId::new(1).unwrap(), 0, 1),
        );
        let lit1_id = arena.alloc_expr(
            NodeKind::ExprLiteral,
            Span::new(FileId::new(1).unwrap(), 0, 1),
            ExprData::Literal {
                lit: lit1_placeholder,
            },
        );

        // - Plus operator
        let op_id = arena.alloc(
            NodeKind::Placeholder,
            Span::new(FileId::new(1).unwrap(), 2, 1),
        );

        // - Literal(2)
        let lit2_placeholder = arena.alloc(
            NodeKind::Placeholder,
            Span::new(FileId::new(1).unwrap(), 4, 1),
        );
        let lit2_id = arena.alloc_expr(
            NodeKind::ExprLiteral,
            Span::new(FileId::new(1).unwrap(), 4, 1),
            ExprData::Literal {
                lit: lit2_placeholder,
            },
        );

        // - Infix(lit1, op, lit2)
        let infix_id = arena.alloc_expr(
            NodeKind::ExprInfix,
            Span::new(FileId::new(1).unwrap(), 0, 5),
            ExprData::Infix {
                lhs: lit1_id,
                op: op_id,
                rhs: lit2_id,
            },
        );

        let term = Term::new(&arena, infix_id);

        // Introspect: walk the Infix node
        let kind_str = format!("{:?}", kind(&term));
        assert!(kind_str.contains("Infix"));

        let kids = children(&term);
        assert_eq!(kids.len(), 3);

        // Build snapshot string: "Infix [Literal, Literal, Literal]"
        let kid_kinds: Vec<String> = kids.iter().map(|k| format!("{:?}", kind(k))).collect();

        let snapshot = format!("{}[{}]", kind_str, kid_kinds.join(", "));

        assert_eq!(snapshot, "Infix[Literal, Literal, Literal]");
    }

    #[test]
    fn type_cache_multiple_entries() {
        let mut arena = AstArena::new();

        let lit1_placeholder = arena.alloc(NodeKind::Placeholder, test_span());
        let lit1_id = arena.alloc_expr(
            NodeKind::ExprLiteral,
            test_span(),
            ExprData::Literal {
                lit: lit1_placeholder,
            },
        );

        let lit2_placeholder = arena.alloc(NodeKind::Placeholder, test_span());
        let lit2_id = arena.alloc_expr(
            NodeKind::ExprLiteral,
            test_span(),
            ExprData::Literal {
                lit: lit2_placeholder,
            },
        );

        let mut cache = TypeCache::new();
        let ty1 = TypeId::new(1).unwrap();
        let ty2 = TypeId::new(2).unwrap();

        cache.insert(lit1_id, ty1);
        cache.insert(lit2_id, ty2);

        let term1 = Term::new(&arena, lit1_id);
        let term2 = Term::new(&arena, lit2_id);

        assert_eq!(type_of(&term1, &cache), Some(ty1));
        assert_eq!(type_of(&term2, &cache), Some(ty2));
    }
}
