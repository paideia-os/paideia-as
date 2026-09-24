//! `Syntax` — the R220.M1 typed reflection surface value.
//!
//! A `Syntax` is an opaque wrapper over a `paideia_as_ast::NodeId` plus
//! the `AstArena` that minted it (via the arena-borrow model already used
//! by `paideia_as_ast::reflect::Term`). Hosted-DSL implementors use it as
//! **their** value type — the elaborator supplies them with a `Syntax`
//! representing the DSL-invocation body, they inspect its structure via
//! [`Syntax::head_kind`] / [`Syntax::children`], build new fragments via
//! [`Syntax::literal`] / [`Syntax::var`] / [`Syntax::app`] / [`Syntax::let_`]
//! / [`Syntax::lambda`] / [`Syntax::match_`], and hand a `Syntax` back
//! for the elaborator to lower.
//!
//! # Scope cap
//!
//! R220.M1 lands the minimum surface R220.M2 (hygiene wiring) and
//! R220.M3 (`@dsl_parser`) need. Structural pattern matching, in-place
//! transformation via the walker's `Replace` action, and quote/antiquote
//! of Types / EffectRows (rather than only Exprs) are FIXME'd for
//! subsequent rounds — none of them block the minimum useful surface.

use paideia_as_ast::reflect::{Term, TermHead};
use paideia_as_ast::{AstArena, ExprData, MatchArm, NodeId, NodeKind};
use paideia_as_diagnostics::Span;

use crate::hygiene::{HYGIENIC_ID_UNTAGGED, HygienicId};

/// Structural-kind discriminant a hosted DSL sees when it inspects a
/// `Syntax` value. Deliberately coarser than [`TermHead`]: `SyntaxKind`
/// exposes only the six constructor categories R220.M1 promises to
/// consumers (`Literal`, `Var`, `App`, `Let`, `Lambda`, `Match`) plus an
/// `Other` catch-all for AST nodes the reflection surface does not yet
/// classify. Keeping the discriminant coarse means R220.M2..M10 can
/// widen the surface without breaking hosted DSLs that pattern-match on
/// `SyntaxKind`.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
#[non_exhaustive]
pub enum SyntaxKind {
    /// A literal value (int / string / bool / …). Constructed via
    /// [`Syntax::literal`]; presented for any underlying `TermHead`
    /// whose category is literal-shaped.
    Literal,
    /// A bare identifier or dotted path in expression position.
    Var,
    /// A function-application shape (`callee(arg, …)`) or infix
    /// operator — both are `App` at the reflection level.
    App,
    /// A `let name = value in body` shape (or a `Block { stmts, tail }`
    /// whose first statement is a `Let`).
    Let,
    /// A lambda (`|x| body` / `fn (x) -> body`).
    Lambda,
    /// A `match scrutinee { arms }` shape.
    Match,
    /// Anything else — quote/antiquote nodes, handler values, operand
    /// nodes, pack/unpack, etc. R220.M2+ may promote some of these to
    /// dedicated variants; consumers should treat this as opaque today.
    Other,
}

/// Head classification returned by [`Syntax::head_kind`]. Bundles the
/// coarse [`SyntaxKind`] discriminant with the underlying AST's
/// [`TermHead`] — hosted DSLs that want raw AST fidelity can drop down;
/// most callers should match on `.kind`.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct SyntaxHead {
    /// Coarse structural category (the API R220.M1 promises to keep
    /// stable across future rounds).
    pub kind: SyntaxKind,
    /// The precise underlying [`TermHead`]. Fine-grained but subject to
    /// churn as new expression kinds land.
    pub term_head: TermHead,
}

/// Opaque `Syntax` handle over a surface-AST subtree.
///
/// Every `Syntax` carries a source [`Span`] (real from the parser, or
/// synthesized by a `Syntax::*` constructor at DSL-fragment build time)
/// and — for identifiers introduced by the DSL — a [`HygienicId`]. See
/// the module-level docs for the design rationale.
#[derive(Copy, Clone)]
pub struct Syntax<'a> {
    inner: Term<'a>,
    hygiene: HygienicId,
}

impl<'a> Syntax<'a> {
    /// Wrap an existing arena-resident `NodeId` as a `Syntax`. The
    /// resulting handle inherits the caller's use-site hygiene (i.e., no
    /// dedicated tag — the [`HYGIENIC_ID_UNTAGGED`] sentinel).
    ///
    /// This is the constructor the elaborator uses when it hands a
    /// `Syntax` value to a hosted DSL implementation for the DSL body.
    #[must_use]
    pub fn from_node(arena: &'a AstArena, id: NodeId) -> Self {
        Self {
            inner: Term::new(arena, id),
            hygiene: HYGIENIC_ID_UNTAGGED,
        }
    }

    /// Wrap an arena-resident `NodeId` with an explicit hygienic tag.
    /// Used by [`Syntax::var`] and by walker replacements that want to
    /// mark an identifier as DSL-introduced.
    #[must_use]
    pub fn from_node_with_hygiene(arena: &'a AstArena, id: NodeId, hygiene: HygienicId) -> Self {
        Self {
            inner: Term::new(arena, id),
            hygiene,
        }
    }

    /// The `NodeId` this handle points at inside its arena.
    #[must_use]
    pub fn node_id(&self) -> NodeId {
        self.inner.id()
    }

    /// The `AstArena` this handle points into.  Threaded through from
    /// the underlying [`Term`] so R220.M2's [`crate::hygiene::hygienic_rename`]
    /// can rebuild a `Syntax` wrapper without also carrying `&AstArena`
    /// alongside every handle.
    #[must_use]
    pub fn arena(&self) -> &'a AstArena {
        self.inner.arena()
    }

    /// The source span of this `Syntax` value.
    #[must_use]
    pub fn span(&self) -> Span {
        self.inner.span()
    }

    /// The hygienic tag attached to this handle. Returns
    /// [`HYGIENIC_ID_UNTAGGED`] for use-site-inherited nodes.
    #[must_use]
    pub fn hygiene(&self) -> HygienicId {
        self.hygiene
    }

    /// Classify this handle into a [`SyntaxHead`] — the coarse
    /// [`SyntaxKind`] plus the underlying [`TermHead`].
    ///
    /// Note the R220.M1 mapping is deliberately conservative: only
    /// unambiguous shapes are promoted out of `SyntaxKind::Other`.
    /// R220.M2 can widen the mapping without touching this signature.
    #[must_use]
    pub fn head_kind(&self) -> SyntaxHead {
        let term_head = self.inner.head();
        let kind = match term_head {
            TermHead::Literal
            | TermHead::String
            | TermHead::ByteString
            | TermHead::InlineBytes
            | TermHead::InlineStr => SyntaxKind::Literal,
            TermHead::Path => SyntaxKind::Var,
            TermHead::Call | TermHead::Infix => SyntaxKind::App,
            TermHead::Block => {
                // R220.M2 FIXME: introspect the first statement and
                // classify a block whose head is a Let as SyntaxKind::Let
                // (the ML-style `let x = e; body` desugars to Block +
                // StmtLet at the parser today). Until then, blocks are
                // opaque to the hosted DSL.
                SyntaxKind::Other
            }
            TermHead::Lambda => SyntaxKind::Lambda,
            TermHead::Match => SyntaxKind::Match,
            _ => SyntaxKind::Other,
        };
        SyntaxHead { kind, term_head }
    }

    /// The immediate sub-`Syntax` values in source order.
    ///
    /// Delegates to `Term::children`; every child inherits the parent's
    /// hygienic tag (use-site tags flow through unchanged). Callers that
    /// need finer control (e.g., wanting to attach a fresh tag to a
    /// specific child) should walk the arena directly via
    /// [`Syntax::node_id`].
    #[must_use]
    pub fn children(&self) -> Vec<Syntax<'a>> {
        let hygiene = self.hygiene;
        self.inner
            .children()
            .into_iter()
            .map(|term| Syntax {
                inner: term,
                hygiene,
            })
            .collect()
    }

    // ── Constructors — build new `Syntax` values in `arena`. ──────────
    //
    // Each constructor allocates fresh AST nodes into the supplied
    // arena and returns a `Syntax` handle. Spans are supplied by the
    // caller — the elaborator or the DSL passes them through from the
    // DSL invocation site so diagnostics point at real source text.

    /// Construct a literal-shaped `Syntax` value. `lit_placeholder` is a
    /// pre-allocated `NodeKind::Placeholder` node the caller has minted;
    /// the wrapper `ExprLiteral` is allocated here.
    ///
    /// FIXME(R220.M2): today this mirrors the parser's own literal shape
    /// (a `Placeholder` payload that dedicated variants replace later).
    /// R220.M2 can widen to accept typed literal payloads (Int/Bool/Str)
    /// once the AST grows dedicated variants for them.
    pub fn literal(arena: &mut AstArena, span: Span, lit_placeholder: NodeId) -> NodeId {
        arena.alloc_expr(
            NodeKind::ExprLiteral,
            span,
            ExprData::Literal {
                lit: lit_placeholder,
            },
        )
    }

    /// Construct a bare-identifier `Syntax` value (an `ExprPath` with
    /// one segment). `ident` is a pre-allocated `NodeKind::Ident` node.
    ///
    /// The caller decides the [`HygienicId`] — usually
    /// [`crate::fresh_hygienic_id`] for a DSL-introduced binding, or
    /// [`HYGIENIC_ID_UNTAGGED`] when copying a use-site identifier.
    pub fn var(arena: &mut AstArena, span: Span, ident: NodeId) -> NodeId {
        arena.alloc_expr(
            NodeKind::ExprPath,
            span,
            ExprData::Path {
                segments: vec![ident],
            },
        )
    }

    /// Construct a function-application `Syntax` value.
    pub fn app(arena: &mut AstArena, span: Span, callee: NodeId, args: Vec<NodeId>) -> NodeId {
        arena.alloc_expr(NodeKind::ExprCall, span, ExprData::Call { callee, args })
    }

    /// Construct a `let module N = unpack v in rest` shape.
    ///
    /// R220.M1 exposes only the module-level `let`. Value-level
    /// `let name = e; body` desugars to a `Block { stmts, tail }` at the
    /// parser today; DSL implementors that need the value-level form
    /// today construct that block explicitly via the arena. R220.M2 will
    /// promote a dedicated `SyntaxKind::Let` constructor once the
    /// value-level shape has a single canonical AST representation.
    pub fn let_(arena: &mut AstArena, span: Span, name: String, body: NodeId, rest: NodeId) -> NodeId {
        arena.alloc_expr(
            NodeKind::ExprLetModule,
            span,
            ExprData::LetModule {
                name,
                body,
                rest,
            },
        )
    }

    /// Construct a lambda in pipe form (`|params| body`). `params` are
    /// the parameter nodes (Pattern : Type pairs) — the caller is
    /// responsible for allocating those since patterns have their own
    /// per-parameter shape.
    pub fn lambda(arena: &mut AstArena, span: Span, params: Vec<NodeId>, body: NodeId) -> NodeId {
        arena.alloc_expr(
            NodeKind::ExprLambda,
            span,
            ExprData::Lambda {
                generic_params: Vec::new(),
                params,
                body,
                pipe_form: true,
            },
        )
    }

    /// Construct a `match scrutinee { arms }` shape. The `arms` vector
    /// is a set of pre-built `MatchArm { pattern, guard, body }`
    /// triples; the caller allocates the pattern and body subtrees.
    pub fn match_(arena: &mut AstArena, span: Span, scrutinee: NodeId, arms: Vec<MatchArm>) -> NodeId {
        arena.alloc_expr(
            NodeKind::ExprMatch,
            span,
            ExprData::Match {
                scrutinee,
                arms,
                attrs: Default::default(),
            },
        )
    }
}

impl<'a> core::fmt::Debug for Syntax<'a> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let head = self.head_kind();
        f.debug_struct("Syntax")
            .field("node_id", &self.inner.id())
            .field("kind", &head.kind)
            .field("term_head", &head.term_head)
            .field("hygiene", &self.hygiene)
            .finish()
    }
}

/// Recursively count the maximum quote nesting depth beneath `id` in
/// `arena`.
///
/// Used by the parser's depth-3 guardrail and by the walker to reject
/// pathological hosted-DSL bodies. A leaf or non-quote node returns 0;
/// each `ExprQuote` node contributes +1 to the depth of its body.
#[must_use]
pub fn quote_depth(arena: &AstArena, id: NodeId) -> u32 {
    let node = match arena.get(id) {
        Some(nd) => nd,
        None => return 0,
    };
    let self_bump = if node.kind == NodeKind::ExprQuote { 1 } else { 0 };

    // The parser's antiquote nodes do NOT reduce the depth counter —
    // they are the antiquote position, not a fresh scope. R220.M2 can
    // refine this if the hygiene pass wants finer-grained accounting.
    let mut max_child = 0u32;
    let term = Term::new(arena, id);
    for child in term.children() {
        let d = quote_depth(arena, child.id());
        if d > max_child {
            max_child = d;
        }
    }
    self_bump + max_child
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::FileId;

    fn span() -> Span {
        Span::new(FileId::new(1).unwrap(), 0, 1)
    }

    #[test]
    fn from_node_carries_untagged_hygiene() {
        let mut arena = AstArena::new();
        let lit_placeholder = arena.alloc(NodeKind::Placeholder, span());
        let lit_id = arena.alloc_expr(
            NodeKind::ExprLiteral,
            span(),
            ExprData::Literal { lit: lit_placeholder },
        );
        let s = Syntax::from_node(&arena, lit_id);
        assert!(s.hygiene().is_untagged());
        assert_eq!(s.node_id(), lit_id);
    }

    #[test]
    fn head_kind_maps_literal_to_literal() {
        let mut arena = AstArena::new();
        let lit_placeholder = arena.alloc(NodeKind::Placeholder, span());
        let lit_id = arena.alloc_expr(
            NodeKind::ExprLiteral,
            span(),
            ExprData::Literal { lit: lit_placeholder },
        );
        let s = Syntax::from_node(&arena, lit_id);
        let head = s.head_kind();
        assert_eq!(head.kind, SyntaxKind::Literal);
        assert_eq!(head.term_head, TermHead::Literal);
    }

    #[test]
    fn head_kind_maps_path_to_var() {
        let mut arena = AstArena::new();
        let ident = arena.alloc(NodeKind::Ident, span());
        let path_id = Syntax::var(&mut arena, span(), ident);
        let s = Syntax::from_node(&arena, path_id);
        assert_eq!(s.head_kind().kind, SyntaxKind::Var);
    }

    #[test]
    fn head_kind_maps_call_to_app() {
        let mut arena = AstArena::new();
        let ident = arena.alloc(NodeKind::Ident, span());
        let callee = Syntax::var(&mut arena, span(), ident);
        let call_id = Syntax::app(&mut arena, span(), callee, Vec::new());
        let s = Syntax::from_node(&arena, call_id);
        assert_eq!(s.head_kind().kind, SyntaxKind::App);
    }

    #[test]
    fn children_of_call_returns_callee_plus_args() {
        let mut arena = AstArena::new();
        let ident = arena.alloc(NodeKind::Ident, span());
        let callee = Syntax::var(&mut arena, span(), ident);
        let lit_placeholder = arena.alloc(NodeKind::Placeholder, span());
        let arg = Syntax::literal(&mut arena, span(), lit_placeholder);
        let call_id = Syntax::app(&mut arena, span(), callee, vec![arg]);
        let s = Syntax::from_node(&arena, call_id);
        let kids = s.children();
        assert_eq!(kids.len(), 2);
        assert_eq!(kids[0].node_id(), callee);
        assert_eq!(kids[1].node_id(), arg);
    }

    #[test]
    fn lambda_constructor_creates_pipe_form() {
        let mut arena = AstArena::new();
        let body_placeholder = arena.alloc(NodeKind::Placeholder, span());
        let body = Syntax::literal(&mut arena, span(), body_placeholder);
        let lambda_id = Syntax::lambda(&mut arena, span(), Vec::new(), body);
        match arena.expr_data(lambda_id).unwrap() {
            ExprData::Lambda { pipe_form, .. } => assert!(*pipe_form),
            _ => panic!("expected Lambda"),
        }
    }

    #[test]
    fn match_constructor_carries_arms() {
        let mut arena = AstArena::new();
        let scrut_placeholder = arena.alloc(NodeKind::Placeholder, span());
        let scrutinee = Syntax::literal(&mut arena, span(), scrut_placeholder);
        let pat = arena.alloc_pattern(
            NodeKind::PatWildcard,
            span(),
            paideia_as_ast::PatternData::Wildcard,
        );
        let body_placeholder = arena.alloc(NodeKind::Placeholder, span());
        let body = Syntax::literal(&mut arena, span(), body_placeholder);
        let arms = vec![MatchArm {
            pattern: pat,
            guard: None,
            body,
        }];
        let match_id = Syntax::match_(&mut arena, span(), scrutinee, arms);
        let s = Syntax::from_node(&arena, match_id);
        assert_eq!(s.head_kind().kind, SyntaxKind::Match);
        assert_eq!(s.children().len(), 3); // scrutinee + pattern + body
    }

    #[test]
    fn quote_depth_zero_for_non_quote() {
        let mut arena = AstArena::new();
        let lit_placeholder = arena.alloc(NodeKind::Placeholder, span());
        let lit_id = arena.alloc_expr(
            NodeKind::ExprLiteral,
            span(),
            ExprData::Literal { lit: lit_placeholder },
        );
        assert_eq!(quote_depth(&arena, lit_id), 0);
    }

    #[test]
    fn quote_depth_counts_nested_quotes() {
        // Build: Quote { Quote { Quote { lit } } } — depth 3
        let mut arena = AstArena::new();
        let lit_placeholder = arena.alloc(NodeKind::Placeholder, span());
        let lit_id = arena.alloc_expr(
            NodeKind::ExprLiteral,
            span(),
            ExprData::Literal { lit: lit_placeholder },
        );
        let q1 = arena.alloc_expr(NodeKind::ExprQuote, span(), ExprData::Quote { body: lit_id });
        let q2 = arena.alloc_expr(NodeKind::ExprQuote, span(), ExprData::Quote { body: q1 });
        let q3 = arena.alloc_expr(NodeKind::ExprQuote, span(), ExprData::Quote { body: q2 });
        assert_eq!(quote_depth(&arena, q3), 3);
        assert_eq!(quote_depth(&arena, q2), 2);
        assert_eq!(quote_depth(&arena, q1), 1);
    }

    #[test]
    fn debug_format_includes_kind_and_hygiene() {
        let mut arena = AstArena::new();
        let lit_placeholder = arena.alloc(NodeKind::Placeholder, span());
        let lit_id = arena.alloc_expr(
            NodeKind::ExprLiteral,
            span(),
            ExprData::Literal { lit: lit_placeholder },
        );
        let s = Syntax::from_node(&arena, lit_id);
        let dbg = format!("{:?}", s);
        assert!(dbg.contains("Syntax"));
        assert!(dbg.contains("Literal"));
        assert!(dbg.contains("hygiene"));
    }
}
