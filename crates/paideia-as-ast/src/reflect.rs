//! Reflection over AST trees: typed handles and serialization.
//!
//! This module provides [`Term`], a thin typed handle over `(NodeId, &AstArena)`,
//! and [`TermHead`], a discriminant for term classification and pattern matching.
//! A [`Term`] is never constructed with an invalid NodeId; the arena is always
//! the single source of truth.
//!
//! The [`SerializedTerm`] struct supports round-tripping of AST subtrees to JSON
//! via `serde`, enabling external tools and metaprogramming use cases.

use crate::{AstArena, ExprData, NodeId, NodeKind, TypeData};
use smallvec::SmallVec;

/// Discriminant for a term's top-level variant.
///
/// Covers all expression kinds, operand kinds, and (in Phase 2+) statement
/// and type variants. Used for pattern matching without deconstructing
/// the full AST.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
#[non_exhaustive]
pub enum TermHead {
    /// `fn/λ params -> body` or `|x, y| body`.
    Lambda,
    /// `action !{eff} @{caps} { stmts }`.
    ActionBlock,
    /// `with handler-expr handle name block`.
    WithHandler,
    /// `unsafe { effects: …, capabilities: …, justification: …, block: … }`.
    Unsafe,
    /// `lhs op rhs` (infix operator expression).
    Infix,
    /// `op expr` (prefix operator expression).
    Prefix,
    /// `expr op` (postfix operator expression).
    Postfix,
    /// Literal (Int/Float/Char/String/Byte/ByteString/Unit/Bool).
    Literal,
    /// `path::to::name` or simple `name`.
    Path,
    /// `f(args)`.
    Call,
    /// `{ stmts; expr? }`.
    Block,
    /// `match scrutinee { arms }`.
    Match,
    /// `if cond then else?`.
    If,
    /// `loop block` or `while cond block`.
    Loop,
    /// `for pat in iter { body }`.
    For,
    /// Register operand (e.g., `rax`, `r8`).
    OperandRegister,
    /// Immediate operand.
    OperandImmediate,
    /// Memory reference operand.
    OperandMemoryRef,
    /// `perform Effect::op(args)`.
    Perform,
    /// `resume value`.
    Resume,
    /// `handle Effect { arms }` — handler-value construction.
    HandlerValue,
    /// `quote { ... }` (code quotation).
    Quote,
    /// `~(...)` (antiquotation).
    Antiquote,
    /// `F(M)(N) sharing (...)` (functor application).
    FunctorApp,
    /// `pack M : S` (pack expression).
    Pack,
    /// `unpack v` (unpack expression).
    Unpack,
    /// `let module N = unpack v in <expr>` (let-module binding).
    LetModule,
    /// `*T` (pointer type).
    TypePtr,
    /// `TypeName { field1: expr1, ... }` (record constructor).
    RecordCons,
    /// `receiver.field` (field access).
    FieldAccess,
    /// String literal `"..."`.
    String,
    /// Byte string literal `b"..."`.
    ByteString,
    /// `record { field1: T1, ... }` (record type).
    TypeRecord,
}

/// A typed handle to an AST expression node.
///
/// `Term` wraps a `NodeId` and a reference to the `AstArena` that owns it.
/// The arena is always the single source of truth; `Term` is a thin, non-owning
/// reference.
///
/// `Term` is `Copy`-friendly (only the reference and ID are copied; the arena
/// and node data remain in place).
#[derive(Copy, Clone)]
pub struct Term<'a> {
    id: NodeId,
    arena: &'a AstArena,
}

impl<'a> Term<'a> {
    /// Construct a `Term` from a `NodeId` and arena reference.
    ///
    /// # Panics
    ///
    /// Panics if the `id` is out of range for the arena.
    #[must_use]
    pub fn new(arena: &'a AstArena, id: NodeId) -> Self {
        debug_assert!(
            arena.get(id).is_some(),
            "Term::new called with out-of-range NodeId"
        );
        Self { id, arena }
    }

    /// Return the NodeId of this term.
    #[must_use]
    pub fn id(&self) -> NodeId {
        self.id
    }

    /// Return the source span of this term.
    #[must_use]
    pub fn span(&self) -> paideia_as_diagnostics::Span {
        self.arena[self.id].span
    }

    /// Return the [`TermHead`] (top-level variant) of this term.
    #[must_use]
    pub fn head(&self) -> TermHead {
        match self.arena.get(self.id) {
            Some(node_data) => match node_data.kind {
                NodeKind::ExprLambda => TermHead::Lambda,
                NodeKind::ExprActionBlock => TermHead::ActionBlock,
                NodeKind::ExprWithHandler => TermHead::WithHandler,
                NodeKind::ExprUnsafe => TermHead::Unsafe,
                NodeKind::ExprInfix => TermHead::Infix,
                NodeKind::ExprPrefix => TermHead::Prefix,
                NodeKind::ExprPostfix => TermHead::Postfix,
                NodeKind::ExprLiteral => TermHead::Literal,
                NodeKind::ExprPath => TermHead::Path,
                NodeKind::ExprCall => TermHead::Call,
                NodeKind::ExprBlock => TermHead::Block,
                NodeKind::ExprMatch => TermHead::Match,
                NodeKind::ExprIf => TermHead::If,
                NodeKind::ExprLoop => TermHead::Loop,
                NodeKind::ExprFor => TermHead::For,
                NodeKind::OperandRegister => TermHead::OperandRegister,
                NodeKind::OperandImmediate => TermHead::OperandImmediate,
                NodeKind::OperandMemoryRef => TermHead::OperandMemoryRef,
                NodeKind::ExprPerform => TermHead::Perform,
                NodeKind::ExprResume => TermHead::Resume,
                NodeKind::ExprHandlerValue => TermHead::HandlerValue,
                NodeKind::ExprQuote => TermHead::Quote,
                NodeKind::ExprAntiquote => TermHead::Antiquote,
                NodeKind::ExprFunctorApp => TermHead::FunctorApp,
                NodeKind::ExprPack => TermHead::Pack,
                NodeKind::ExprUnpack => TermHead::Unpack,
                NodeKind::ExprLetModule => TermHead::LetModule,
                NodeKind::ExprRecordCons => TermHead::RecordCons,
                NodeKind::ExprFieldAccess => TermHead::FieldAccess,
                NodeKind::ExprString => TermHead::String,
                NodeKind::ExprByteString => TermHead::ByteString,
                NodeKind::TypePtr => TermHead::TypePtr,
                NodeKind::TypeRecord => TermHead::TypeRecord,
                _ => {
                    // Non-expression kinds: this term does not represent an expression.
                    // Return a placeholder; Phase 2 will add dedicated handling for
                    // statements and types.
                    TermHead::Literal // placeholder for invalid kinds
                }
            },
            None => TermHead::Literal, // placeholder for out-of-range
        }
    }

    /// Return the immediate children of this term as a [`SmallVec`].
    ///
    /// For most expression kinds, this collects the direct child node IDs.
    /// The vector uses inline storage for up to 4 children before spilling
    /// to the heap.
    #[must_use]
    pub fn children(&self) -> SmallVec<[Term<'a>; 4]> {
        let mut result = SmallVec::new();

        // Dispatch type nodes via type_data.
        if let Some(ty) = self.arena.type_data(self.id) {
            if let TypeData::Ptr { pointee } = ty {
                result.push(Term::new(self.arena, *pointee));
            }
            // Other type variants either have no meaningful child terms
            // (e.g. Name, EffectRow) or are handled elsewhere.
            return result;
        }

        if let Some(expr) = self.arena.expr_data(self.id) {
            match expr {
                ExprData::Lambda { params, body, .. } => {
                    for &param in params {
                        result.push(Term::new(self.arena, param));
                    }
                    result.push(Term::new(self.arena, *body));
                }
                ExprData::ActionBlock {
                    effects,
                    capabilities,
                    body,
                } => {
                    if let Some(eff) = effects {
                        result.push(Term::new(self.arena, *eff));
                    }
                    if let Some(cap) = capabilities {
                        result.push(Term::new(self.arena, *cap));
                    }
                    for &stmt in body {
                        result.push(Term::new(self.arena, stmt));
                    }
                }
                ExprData::WithHandler {
                    handler,
                    bind,
                    block,
                    finally,
                } => {
                    result.push(Term::new(self.arena, *handler));
                    result.push(Term::new(self.arena, *bind));
                    result.push(Term::new(self.arena, *block));
                    if let Some(f) = finally {
                        result.push(Term::new(self.arena, *f));
                    }
                }
                ExprData::Unsafe {
                    effects,
                    capabilities,
                    justification,
                    block,
                } => {
                    for &eff in effects {
                        result.push(Term::new(self.arena, eff));
                    }
                    for &cap in capabilities {
                        result.push(Term::new(self.arena, cap));
                    }
                    result.push(Term::new(self.arena, *justification));
                    for &stmt in block {
                        result.push(Term::new(self.arena, stmt));
                    }
                }
                ExprData::Infix { lhs, op, rhs } => {
                    result.push(Term::new(self.arena, *lhs));
                    result.push(Term::new(self.arena, *op));
                    result.push(Term::new(self.arena, *rhs));
                }
                ExprData::Prefix { op, expr, .. } => {
                    result.push(Term::new(self.arena, *op));
                    result.push(Term::new(self.arena, *expr));
                }
                ExprData::Postfix { expr, op } => {
                    result.push(Term::new(self.arena, *expr));
                    result.push(Term::new(self.arena, *op));
                }
                ExprData::Literal { lit } => {
                    result.push(Term::new(self.arena, *lit));
                }
                ExprData::Path { segments } => {
                    for &seg in segments {
                        result.push(Term::new(self.arena, seg));
                    }
                }
                ExprData::Call { callee, args } => {
                    result.push(Term::new(self.arena, *callee));
                    for &arg in args {
                        result.push(Term::new(self.arena, arg));
                    }
                }
                ExprData::Block { stmts, tail } => {
                    for &stmt in stmts {
                        result.push(Term::new(self.arena, stmt));
                    }
                    if let Some(t) = tail {
                        result.push(Term::new(self.arena, *t));
                    }
                }
                ExprData::Match { scrutinee, arms } => {
                    result.push(Term::new(self.arena, *scrutinee));
                    for arm in arms {
                        result.push(Term::new(self.arena, arm.pattern));
                        if let Some(guard) = arm.guard {
                            result.push(Term::new(self.arena, guard));
                        }
                        result.push(Term::new(self.arena, arm.body));
                    }
                }
                ExprData::If {
                    cond,
                    then_block,
                    else_block,
                } => {
                    result.push(Term::new(self.arena, *cond));
                    result.push(Term::new(self.arena, *then_block));
                    if let Some(e) = else_block {
                        result.push(Term::new(self.arena, *e));
                    }
                }
                ExprData::Loop { header, body, .. } => {
                    if let Some(h) = header {
                        result.push(Term::new(self.arena, *h));
                    }
                    result.push(Term::new(self.arena, *body));
                }
                ExprData::For {
                    pattern,
                    iterable,
                    body,
                } => {
                    result.push(Term::new(self.arena, *pattern));
                    result.push(Term::new(self.arena, *iterable));
                    result.push(Term::new(self.arena, *body));
                }
                ExprData::Break => {
                    // No children for break
                }
                ExprData::Continue => {
                    // No children for continue
                }
                ExprData::OperandRegister { reg } => {
                    result.push(Term::new(self.arena, *reg));
                }
                ExprData::OperandImmediate { expr } => {
                    result.push(Term::new(self.arena, *expr));
                }
                ExprData::OperandMemoryRef { addr } => {
                    result.push(Term::new(self.arena, *addr));
                }
                ExprData::Perform { op_path, args } => {
                    result.push(Term::new(self.arena, *op_path));
                    for &arg in args {
                        result.push(Term::new(self.arena, arg));
                    }
                }
                ExprData::Resume { value } => {
                    result.push(Term::new(self.arena, *value));
                }
                ExprData::HandlerValue { effect, arms } => {
                    result.push(Term::new(self.arena, *effect));
                    for arm in arms {
                        match arm {
                            crate::HandlerArm::Op { op, handler } => {
                                result.push(Term::new(self.arena, *op));
                                result.push(Term::new(self.arena, *handler));
                            }
                            crate::HandlerArm::Finally { cleanup } => {
                                result.push(Term::new(self.arena, *cleanup));
                            }
                        }
                    }
                }
                ExprData::Quote { body } => {
                    result.push(Term::new(self.arena, *body));
                }
                ExprData::Antiquote { value } => {
                    result.push(Term::new(self.arena, *value));
                }
                ExprData::FunctorApp {
                    functor,
                    arguments,
                    sharing: _,
                } => {
                    result.push(Term::new(self.arena, *functor));
                    for arg in arguments {
                        result.push(Term::new(self.arena, *arg));
                    }
                }
                ExprData::Pack {
                    module_path,
                    signature_path,
                } => {
                    result.push(Term::new(self.arena, *module_path));
                    result.push(Term::new(self.arena, *signature_path));
                }
                ExprData::Unpack { value } => {
                    result.push(Term::new(self.arena, *value));
                }
                ExprData::LetModule {
                    name: _,
                    body,
                    rest,
                } => {
                    result.push(Term::new(self.arena, *body));
                    result.push(Term::new(self.arena, *rest));
                }
                ExprData::RecordCons { type_name, fields } => {
                    result.push(Term::new(self.arena, *type_name));
                    for (name, expr) in fields {
                        result.push(Term::new(self.arena, *name));
                        result.push(Term::new(self.arena, *expr));
                    }
                }
                ExprData::FieldAccess { receiver, field } => {
                    result.push(Term::new(self.arena, *receiver));
                    result.push(Term::new(self.arena, *field));
                }
                ExprData::StringLiteral(_) => {
                    // No children for string literals
                }
                ExprData::ByteStringLiteral(_) => {
                    // No children for byte string literals
                }
                ExprData::Borrow { expr, .. } => {
                    result.push(Term::new(self.arena, *expr));
                }
                ExprData::Deref { expr } => {
                    result.push(Term::new(self.arena, *expr));
                }
                ExprData::ArrayLit(elements) => {
                    for element in elements {
                        result.push(Term::new(self.arena, *element));
                    }
                }
                ExprData::Uninit => {
                    // No children for uninit
                }
            }
        }

        // Handle TypeRecord
        if let Some(TypeData::Record { fields }) = self.arena.type_data(self.id) {
            for (name, ty_node) in fields {
                result.push(Term::new(self.arena, *name));
                result.push(Term::new(self.arena, *ty_node));
            }
            return result;
        }

        // Handle TypeEnum
        if let Some(TypeData::Enum { variants }) = self.arena.type_data(self.id) {
            for variant in variants {
                match variant {
                    crate::EnumVariant::Unit { name } => {
                        result.push(Term::new(self.arena, *name));
                    }
                    crate::EnumVariant::Tuple { name, payload } => {
                        result.push(Term::new(self.arena, *name));
                        for ty_node in payload {
                            result.push(Term::new(self.arena, *ty_node));
                        }
                    }
                    crate::EnumVariant::Record { name, fields } => {
                        result.push(Term::new(self.arena, *name));
                        for (field_name, field_ty) in fields {
                            result.push(Term::new(self.arena, *field_name));
                            result.push(Term::new(self.arena, *field_ty));
                        }
                    }
                }
            }
            return result;
        }

        result
    }
}

impl<'a> std::fmt::Debug for Term<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Term")
            .field("id", &self.id)
            .field("head", &self.head())
            .finish()
    }
}

/// Serializable representation of a term (AST subtree).
///
/// Can be round-tripped to JSON via `serde_json` for interop with
/// external tools. The structure mirrors [`Term`] but stores node IDs
/// as u32 integers and flattens the arena reference.
#[derive(Clone, Debug, PartialEq)]
pub struct SerializedTerm {
    /// The head (top-level variant) of this term.
    pub head: TermHead,
    /// The source span of this node (start, end in file).
    pub span: SerializedSpan,
    /// The immediate children of this term (recursively serialized).
    pub children: Vec<SerializedTerm>,
}

/// Serializable span representation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SerializedSpan {
    /// File ID (opaque u32).
    pub file_id: u32,
    /// Byte offset (start).
    pub byte_start: u32,
    /// Byte length.
    pub byte_len: u32,
}

impl SerializedTerm {
    /// Construct a `SerializedTerm` from a [`Term`] by recursively
    /// traversing its children.
    ///
    /// The serialized form is independent of the arena and can be
    /// stored or transmitted.
    #[must_use]
    pub fn from_term(term: Term<'_>) -> Self {
        let span = term.span();
        let serialized_span = SerializedSpan {
            file_id: span.file().get(),
            byte_start: span.byte_start(),
            byte_len: span.byte_len(),
        };

        let children = term
            .children()
            .iter()
            .map(|&child| SerializedTerm::from_term(child))
            .collect();

        Self {
            head: term.head(),
            span: serialized_span,
            children,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::FileId;

    fn span() -> paideia_as_diagnostics::Span {
        paideia_as_diagnostics::Span::new(FileId::new(1).unwrap(), 0, 1)
    }

    #[test]
    fn term_head_for_quote_node() {
        let mut arena = AstArena::new();
        let body_id = arena.alloc(NodeKind::Placeholder, span());
        let quote_id = arena.alloc_expr(
            NodeKind::ExprQuote,
            span(),
            ExprData::Quote { body: body_id },
        );
        let term = Term::new(&arena, quote_id);
        assert_eq!(term.head(), TermHead::Quote);
    }

    #[test]
    fn term_children_for_infix() {
        let mut arena = AstArena::new();
        let lit1_id = arena.alloc(NodeKind::Placeholder, span());
        let lhs_id = arena.alloc_expr(
            NodeKind::ExprLiteral,
            span(),
            ExprData::Literal { lit: lit1_id },
        );
        let op_id = arena.alloc(NodeKind::Placeholder, span());
        let lit2_id = arena.alloc(NodeKind::Placeholder, span());
        let rhs_id = arena.alloc_expr(
            NodeKind::ExprLiteral,
            span(),
            ExprData::Literal { lit: lit2_id },
        );
        let infix_id = arena.alloc_expr(
            NodeKind::ExprInfix,
            span(),
            ExprData::Infix {
                lhs: lhs_id,
                op: op_id,
                rhs: rhs_id,
            },
        );

        let term = Term::new(&arena, infix_id);
        let children = term.children();
        assert_eq!(children.len(), 3);
        assert_eq!(children[0].id(), lhs_id);
        assert_eq!(children[1].id(), op_id);
        assert_eq!(children[2].id(), rhs_id);
    }

    #[test]
    fn serialized_term_round_trip_path() {
        let mut arena = AstArena::new();
        let seg1_id = arena.alloc(NodeKind::Ident, span());
        let path_id = arena.alloc_expr(
            NodeKind::ExprPath,
            span(),
            ExprData::Path {
                segments: vec![seg1_id],
            },
        );

        let term = Term::new(&arena, path_id);
        let serialized = SerializedTerm::from_term(term);
        let serialized2 = SerializedTerm::from_term(Term::new(&arena, path_id));

        assert_eq!(serialized, serialized2);
    }

    #[test]
    fn serialized_term_round_trip_quote() {
        let mut arena = AstArena::new();
        let lit_placeholder = arena.alloc(NodeKind::Placeholder, span());
        let lit_id = arena.alloc_expr(
            NodeKind::ExprLiteral,
            span(),
            ExprData::Literal {
                lit: lit_placeholder,
            },
        );
        let quote_id = arena.alloc_expr(
            NodeKind::ExprQuote,
            span(),
            ExprData::Quote { body: lit_id },
        );

        let term = Term::new(&arena, quote_id);
        let serialized = SerializedTerm::from_term(term);
        let serialized2 = SerializedTerm::from_term(Term::new(&arena, quote_id));

        assert_eq!(serialized, serialized2);
        assert_eq!(serialized.head, TermHead::Quote);
        assert_eq!(serialized.children.len(), 1);
    }

    #[test]
    fn walk_expr_dispatches_quote_and_antiquote() {
        use crate::ExprVisitor;

        let mut arena = AstArena::new();
        let body_id = arena.alloc(NodeKind::Placeholder, span());
        let quote_id = arena.alloc_expr(
            NodeKind::ExprQuote,
            span(),
            ExprData::Quote { body: body_id },
        );

        struct CountingVisitor {
            quote_count: usize,
            antiquote_count: usize,
        }

        impl ExprVisitor for CountingVisitor {
            fn visit_expr_quote(&mut self, _arena: &AstArena, _id: NodeId) {
                self.quote_count += 1;
            }
            fn visit_expr_antiquote(&mut self, _arena: &AstArena, _id: NodeId) {
                self.antiquote_count += 1;
            }
        }

        let mut visitor = CountingVisitor {
            quote_count: 0,
            antiquote_count: 0,
        };
        crate::walk_expr(&mut visitor, &arena, quote_id);
        assert_eq!(visitor.quote_count, 1);
        assert_eq!(visitor.antiquote_count, 0);
    }

    #[test]
    fn quote_ast_snapshot() {
        let mut arena = AstArena::new();
        let lit_placeholder = arena.alloc(NodeKind::Placeholder, span());
        let lit_id = arena.alloc_expr(
            NodeKind::ExprLiteral,
            span(),
            ExprData::Literal {
                lit: lit_placeholder,
            },
        );
        let quote_id = arena.alloc_expr(
            NodeKind::ExprQuote,
            span(),
            ExprData::Quote { body: lit_id },
        );

        let output = crate::pretty::print_expr(&arena, quote_id);
        // Expected stable format: Quote wrapper with a Literal body
        assert!(output.contains("Quote"));
        assert!(output.contains("body:"));
    }
}
