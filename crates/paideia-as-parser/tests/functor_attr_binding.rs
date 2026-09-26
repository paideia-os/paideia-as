//! Tests for `@retain` / `@immediate` → `FunctorAttrTable` round-trip
//! (paideia-as#1504, PAS-DEBT-B2-011).
//!
//! Pre-fix, the standalone `parse_functor_with_attrs` returned a
//! `FunctorDecl` with no arena identity, so the attributes it parsed
//! had no `NodeId` to key `FunctorAttrTable` on and were silently
//! dropped. Post-fix, `parse_functor_with_attrs_into_arena` allocates a
//! `NodeKind::FunctorDecl` item and pushes each attribute keyed on the
//! freshly minted id — a downstream elaborator pass can then read them
//! back via `arena.functor_attr().get(id)`.
//!
//! Fixtures cover the three attribute states (`@retain`, `@immediate`,
//! none), the NodeKind identity of the returned id, the FunctorDecl
//! payload shape, session-binding preservation, and defensive
//! reflect/visit coverage.

use paideia_as_ast::{
    AstArena, FunctorAttr, ItemData, ItemVisitor, NodeId, NodeKind, TermHead, Term, walk_item,
};
use paideia_as_diagnostics::{FileId, Span, VecSink};
use paideia_as_lexer::{Token, TokenKind};
use paideia_as_parser::toolkit_attrs::parse_functor_with_attrs_into_arena;

fn file() -> FileId {
    FileId::new(1).unwrap()
}

fn tok(kind: TokenKind, byte_start: u32, byte_len: u32) -> Token {
    Token::new(kind, Span::new(file(), byte_start, byte_len))
}

/// Canonical functor tail `functor F(In : SigIn) -> SigOut { }` anchored
/// at `start`. Mirrors the helper in `toolkit_attrs::tests`.
fn functor_tail(start: u32) -> Vec<Token> {
    vec![
        tok(TokenKind::KwFunctor, start, 7),
        tok(TokenKind::Ident, start + 8, 1),   // F
        tok(TokenKind::LParen, start + 9, 1),
        tok(TokenKind::Ident, start + 10, 2),  // In
        tok(TokenKind::Colon, start + 13, 1),
        tok(TokenKind::Ident, start + 15, 5),  // SigIn
        tok(TokenKind::RParen, start + 20, 1),
        tok(TokenKind::Arrow, start + 22, 2),
        tok(TokenKind::Ident, start + 25, 6),  // SigOut
        tok(TokenKind::LBrace, start + 32, 1),
        tok(TokenKind::RBrace, start + 34, 1),
        tok(TokenKind::Eof, start + 35, 0),
    ]
}

fn functor_tail_source() -> String {
    "functor F(In : SigIn) -> SigOut { }".to_string()
}

/// `@retain functor F(...)` — the returned id keys a FunctorAttrTable
/// entry containing exactly `[Retain]`.
#[test]
fn retain_prefix_records_attribute_on_arena_id() {
    let src = format!("@retain {}", functor_tail_source());
    let mut toks = vec![
        tok(TokenKind::At, 0, 1),
        tok(TokenKind::Ident, 1, 6), // "retain"
    ];
    toks.extend(functor_tail(8));

    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let (id, attrs, decl) =
        parse_functor_with_attrs_into_arena(&toks, &src, file(), &mut arena, &mut sink)
            .expect("parse should succeed");

    assert!(sink.diagnostics().is_empty(), "diags: {:?}", sink.diagnostics());
    assert_eq!(attrs, vec![FunctorAttr::Retain]);
    assert_eq!(decl.name, "F");

    // Round-trip: the attribute is retrievable from the table by id.
    let stored = arena.functor_attr().get(id).expect("entry present");
    assert_eq!(stored, &[FunctorAttr::Retain]);
    // Only one functor was minted, so exactly one entry in the table.
    assert_eq!(arena.functor_attr().len(), 1);
}

/// `@immediate functor F(...)` — analogous to the `@retain` case.
#[test]
fn immediate_prefix_records_attribute_on_arena_id() {
    let src = format!("@immediate {}", functor_tail_source());
    let mut toks = vec![
        tok(TokenKind::At, 0, 1),
        tok(TokenKind::Ident, 1, 9), // "immediate"
    ];
    toks.extend(functor_tail(11));

    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let (id, attrs, _decl) =
        parse_functor_with_attrs_into_arena(&toks, &src, file(), &mut arena, &mut sink)
            .expect("parse should succeed");

    assert_eq!(attrs, vec![FunctorAttr::Immediate]);
    let stored = arena.functor_attr().get(id).expect("entry present");
    assert_eq!(stored, &[FunctorAttr::Immediate]);
}

/// Bare `functor F(...)` (no attribute prefix) — the id is minted but
/// FunctorAttrTable has no entry for it (sparse convention).
#[test]
fn bare_functor_leaves_attribute_table_empty() {
    let src = functor_tail_source();
    let toks = functor_tail(0);

    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let (id, attrs, _decl) =
        parse_functor_with_attrs_into_arena(&toks, &src, file(), &mut arena, &mut sink)
            .expect("parse should succeed");

    assert!(attrs.is_empty());
    assert!(arena.functor_attr().get(id).is_none());
    assert_eq!(arena.functor_attr().len(), 0);
    // The arena still holds the FunctorDecl node — absence of an entry
    // is a table property, not a missing node.
    assert_eq!(arena.get(id).unwrap().kind, NodeKind::FunctorDecl);
}

/// The returned id points at a `NodeKind::FunctorDecl` node whose
/// `ItemData::FunctorDecl` payload carries per-field Ident children.
#[test]
fn returned_id_has_functor_decl_kind_and_payload() {
    let src = format!("@retain {}", functor_tail_source());
    let mut toks = vec![
        tok(TokenKind::At, 0, 1),
        tok(TokenKind::Ident, 1, 6),
    ];
    toks.extend(functor_tail(8));

    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let (id, _attrs, _decl) =
        parse_functor_with_attrs_into_arena(&toks, &src, file(), &mut arena, &mut sink).unwrap();

    assert_eq!(arena.get(id).unwrap().kind, NodeKind::FunctorDecl);
    match arena.item_data(id) {
        Some(ItemData::FunctorDecl {
            name,
            param_name,
            param_sig,
            return_sig,
            session_var,
            doc,
        }) => {
            for child in [*name, *param_name, *param_sig, *return_sig] {
                assert_eq!(arena.get(child).unwrap().kind, NodeKind::Ident);
            }
            assert!(session_var.is_none(), "no `with` clause in this fixture");
            assert!(doc.is_none());
        }
        other => panic!("expected ItemData::FunctorDecl, got {:?}", other),
    }
}

/// A functor with `with S : session` — the session-var Ident is
/// allocated and stored in ItemData::FunctorDecl.session_var.
#[test]
fn session_binding_allocates_ident_child() {
    // "functor F(In : SigIn) -> SigOut with S : session { }"
    let src = "functor F(In : SigIn) -> SigOut with S : session { }".to_string();
    let toks = vec![
        tok(TokenKind::KwFunctor, 0, 7),
        tok(TokenKind::Ident, 8, 1),     // F
        tok(TokenKind::LParen, 9, 1),
        tok(TokenKind::Ident, 10, 2),    // In
        tok(TokenKind::Colon, 13, 1),
        tok(TokenKind::Ident, 15, 5),    // SigIn
        tok(TokenKind::RParen, 20, 1),
        tok(TokenKind::Arrow, 22, 2),
        tok(TokenKind::Ident, 25, 6),    // SigOut
        tok(TokenKind::KwWith, 32, 4),   // with
        tok(TokenKind::Ident, 37, 1),    // S
        tok(TokenKind::Colon, 39, 1),
        tok(TokenKind::Ident, 41, 7),    // session
        tok(TokenKind::LBrace, 49, 1),
        tok(TokenKind::RBrace, 51, 1),
        tok(TokenKind::Eof, 52, 0),
    ];

    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let (id, _attrs, decl) =
        parse_functor_with_attrs_into_arena(&toks, &src, file(), &mut arena, &mut sink)
            .expect("parse should succeed");

    assert_eq!(decl.session_binding.as_ref().map(|s| s.var.as_str()), Some("S"));
    match arena.item_data(id).unwrap() {
        ItemData::FunctorDecl { session_var: Some(sv), .. } => {
            assert_eq!(arena.get(*sv).unwrap().kind, NodeKind::Ident);
        }
        _ => panic!("session_var should be Some(Ident)"),
    }
}

/// `Term::head()` reflects `NodeKind::FunctorDecl` as
/// `TermHead::FunctorDecl` — guards the reflect head arm against the
/// silent-miss wildcard the Wave-3/4 debugger warned about.
#[test]
fn term_head_reports_functor_decl() {
    let src = functor_tail_source();
    let toks = functor_tail(0);
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let (id, _attrs, _decl) =
        parse_functor_with_attrs_into_arena(&toks, &src, file(), &mut arena, &mut sink).unwrap();

    let term = Term::new(&arena, id);
    assert_eq!(term.head(), TermHead::FunctorDecl);
}

/// `walk_item` dispatches `NodeKind::FunctorDecl` to
/// `visit_functor_decl` — guards the visit arm against the wildcard.
#[test]
fn walk_item_dispatches_functor_decl_visitor() {
    struct Counter {
        seen: usize,
        last: Option<NodeId>,
    }
    impl ItemVisitor for Counter {
        fn visit_functor_decl(&mut self, _arena: &AstArena, id: NodeId) {
            self.seen += 1;
            self.last = Some(id);
        }
    }

    let src = functor_tail_source();
    let toks = functor_tail(0);
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let (id, _attrs, _decl) =
        parse_functor_with_attrs_into_arena(&toks, &src, file(), &mut arena, &mut sink).unwrap();

    let mut c = Counter { seen: 0, last: None };
    walk_item(&mut c, &arena, id);
    assert_eq!(c.seen, 1);
    assert_eq!(c.last, Some(id));
}

/// `pretty::print_item` prints `FunctorDecl` for our new arm — guards
/// the pretty match against the wildcard.
#[test]
fn pretty_print_includes_functor_decl_line() {
    let src = format!("@retain {}", functor_tail_source());
    let mut toks = vec![
        tok(TokenKind::At, 0, 1),
        tok(TokenKind::Ident, 1, 6),
    ];
    toks.extend(functor_tail(8));

    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let (id, _attrs, _decl) =
        parse_functor_with_attrs_into_arena(&toks, &src, file(), &mut arena, &mut sink).unwrap();

    let printed = paideia_as_ast::pretty::print_item(&arena, id);
    assert!(printed.contains("FunctorDecl"), "pretty output: {}", printed);
    assert!(printed.contains("param_name"), "pretty output: {}", printed);
}
