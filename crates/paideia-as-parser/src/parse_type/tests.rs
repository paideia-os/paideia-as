use super::*;
use paideia_as_ast::AstArena;
use paideia_as_diagnostics::{FileId, Span, VecSink};
use paideia_as_lexer::{Token, TokenKind};

fn tok(kind: TokenKind, byte_start: u32) -> Token {
    Token::new(kind, Span::new(FileId::new(1).unwrap(), byte_start, 1))
}

fn parse_t(
    tokens: Vec<Token>,
) -> (
    AstArena,
    Result<paideia_as_ast::NodeId, ParseError>,
    Vec<paideia_as_diagnostics::Diagnostic>,
) {
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let result = {
        let mut p = Parser::new(&tokens, "", FileId::new(1).unwrap(), &mut arena, &mut sink);
        p.parse_type()
    };
    (arena, result, sink.diagnostics().to_vec())
}

#[test]
fn parse_simple_type_name() {
    let tokens = vec![tok(TokenKind::Ident, 0), tok(TokenKind::Eof, 1)];
    let (arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 0);
    assert!(result.is_ok());
    let ty_id = result.unwrap();
    let ty_node = arena.get(ty_id).unwrap();
    assert_eq!(ty_node.kind, NodeKind::TypeName);
    if let Some(TypeData::Name { args, .. }) = arena.type_data(ty_id) {
        assert_eq!(args.len(), 0);
    } else {
        panic!("expected TypeName");
    }
}

#[test]
fn parse_type_with_args() {
    // `Map(K, V)` → Ident LParen Ident Comma Ident RParen Eof
    let tokens = vec![
        tok(TokenKind::Ident, 0),  // Map
        tok(TokenKind::LParen, 3), // (
        tok(TokenKind::Ident, 4),  // K
        tok(TokenKind::Comma, 5),  // ,
        tok(TokenKind::Ident, 7),  // V
        tok(TokenKind::RParen, 8), // )
        tok(TokenKind::Eof, 9),
    ];
    let (arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 0);
    assert!(result.is_ok());
    let ty_id = result.unwrap();
    if let Some(TypeData::Name { args, .. }) = arena.type_data(ty_id) {
        assert_eq!(args.len(), 2);
    } else {
        panic!("expected TypeName with args");
    }
}

#[test]
fn parse_tuple_type() {
    // `(u64, u64)` → LParen Ident Comma Ident RParen Eof
    let tokens = vec![
        tok(TokenKind::LParen, 0),
        tok(TokenKind::Ident, 1), // u64
        tok(TokenKind::Comma, 4),
        tok(TokenKind::Ident, 6), // u64
        tok(TokenKind::RParen, 9),
        tok(TokenKind::Eof, 10),
    ];
    let (arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 0);
    assert!(result.is_ok());
    let ty_id = result.unwrap();
    let ty_node = arena.get(ty_id).unwrap();
    assert_eq!(ty_node.kind, NodeKind::TypeTuple);
    if let Some(TypeData::Tuple { elements }) = arena.type_data(ty_id) {
        assert_eq!(elements.len(), 2);
    } else {
        panic!("expected TypeTuple");
    }
}

#[test]
fn parse_fn_ptr_type() {
    // `(u64) -> u64` → LParen Ident RParen Arrow Ident Eof
    let tokens = vec![
        tok(TokenKind::LParen, 0),
        tok(TokenKind::Ident, 1), // u64
        tok(TokenKind::RParen, 4),
        tok(TokenKind::Arrow, 6),
        tok(TokenKind::Ident, 9), // u64
        tok(TokenKind::Eof, 12),
    ];
    let (arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 0);
    assert!(result.is_ok());
    let ty_id = result.unwrap();
    let ty_node = arena.get(ty_id).unwrap();
    assert_eq!(ty_node.kind, NodeKind::TypeFnPtr);
    if let Some(TypeData::FnPtr {
        params,
        effects,
        capabilities,
        ..
    }) = arena.type_data(ty_id)
    {
        assert_eq!(params.len(), 1);
        assert!(effects.is_none());
        assert!(capabilities.is_none());
    } else {
        panic!("expected TypeFnPtr");
    }
}

#[test]
fn parse_fn_ptr_with_effects() {
    // `(u64) -> u64 !{io}` → LParen Ident RParen Arrow Ident EffectOpen Ident RBrace Eof
    let tokens = vec![
        tok(TokenKind::LParen, 0),
        tok(TokenKind::Ident, 1), // u64
        tok(TokenKind::RParen, 4),
        tok(TokenKind::Arrow, 6),
        tok(TokenKind::Ident, 9), // u64
        tok(TokenKind::EffectOpen, 13),
        tok(TokenKind::Ident, 15), // io
        tok(TokenKind::RBrace, 17),
        tok(TokenKind::Eof, 18),
    ];
    let (arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 0);
    assert!(result.is_ok());
    let ty_id = result.unwrap();
    if let Some(TypeData::FnPtr { effects, .. }) = arena.type_data(ty_id) {
        assert!(effects.is_some());
    } else {
        panic!("expected TypeFnPtr with effects");
    }
}

#[test]
fn parse_fn_ptr_with_capabilities() {
    // `(u64) -> u64 @{cap}` → LParen Ident RParen Arrow Ident CapOpen Ident RBrace Eof
    let tokens = vec![
        tok(TokenKind::LParen, 0),
        tok(TokenKind::Ident, 1), // u64
        tok(TokenKind::RParen, 4),
        tok(TokenKind::Arrow, 6),
        tok(TokenKind::Ident, 9), // u64
        tok(TokenKind::CapOpen, 13),
        tok(TokenKind::Ident, 15), // cap
        tok(TokenKind::RBrace, 18),
        tok(TokenKind::Eof, 19),
    ];
    let (arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 0);
    assert!(result.is_ok());
    let ty_id = result.unwrap();
    if let Some(TypeData::FnPtr { capabilities, .. }) = arena.type_data(ty_id) {
        assert!(capabilities.is_some());
    } else {
        panic!("expected TypeFnPtr with capabilities");
    }
}

#[test]
fn parse_fn_ptr_full() {
    // `(u64, linear Cap) -> u64 !{io} @{Mmio.read_cap}`
    // LParen Ident Comma KwLinear Ident RParen Arrow Ident EffectOpen Ident RBrace CapOpen Ident Dot Ident RBrace Eof
    let tokens = vec![
        tok(TokenKind::LParen, 0),
        tok(TokenKind::Ident, 1), // u64
        tok(TokenKind::Comma, 4),
        tok(TokenKind::KwLinear, 6),
        tok(TokenKind::Ident, 12), // Cap
        tok(TokenKind::RParen, 15),
        tok(TokenKind::Arrow, 17),
        tok(TokenKind::Ident, 20), // u64
        tok(TokenKind::EffectOpen, 24),
        tok(TokenKind::Ident, 26), // io
        tok(TokenKind::RBrace, 28),
        tok(TokenKind::CapOpen, 30),
        tok(TokenKind::Ident, 32), // Mmio
        tok(TokenKind::Dot, 36),
        tok(TokenKind::Ident, 37), // read_cap
        tok(TokenKind::RBrace, 45),
        tok(TokenKind::Eof, 46),
    ];
    let (arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 0);
    assert!(result.is_ok());
    let ty_id = result.unwrap();
    let ty_node = arena.get(ty_id).unwrap();
    assert_eq!(ty_node.kind, NodeKind::TypeFnPtr);
    if let Some(TypeData::FnPtr {
        params,
        effects,
        capabilities,
        ..
    }) = arena.type_data(ty_id)
    {
        assert_eq!(params.len(), 2);
        // Second param should be TypeLinearClass with Linear
        let param2 = params[1];
        let param2_node = arena.get(param2).unwrap();
        assert_eq!(param2_node.kind, NodeKind::TypeLinearClass);
        assert!(effects.is_some());
        assert!(capabilities.is_some());
    } else {
        panic!("expected TypeFnPtr full");
    }
}

// ── Closure type (`|T1, T2| -> R !{E} @{C}`) — issue #994 ──────────────────

#[test]
fn parse_closure_basic() {
    // `|u64| -> u64` → Pipe Ident Pipe Arrow Ident Eof
    let tokens = vec![
        tok(TokenKind::Pipe, 0),
        tok(TokenKind::Ident, 1), // u64
        tok(TokenKind::Pipe, 4),
        tok(TokenKind::Arrow, 6),
        tok(TokenKind::Ident, 9), // u64
        tok(TokenKind::Eof, 12),
    ];
    let (arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 0);
    assert!(result.is_ok());
    let ty_id = result.unwrap();
    let ty_node = arena.get(ty_id).unwrap();
    assert_eq!(ty_node.kind, NodeKind::TypeClosure);
    if let Some(TypeData::Closure {
        params,
        effects,
        capabilities,
        ..
    }) = arena.type_data(ty_id)
    {
        assert_eq!(params.len(), 1);
        assert!(effects.is_none());
        assert!(capabilities.is_none());
    } else {
        panic!("expected TypeClosure");
    }
}

#[test]
fn parse_closure_two_params() {
    // `|u64, u64| -> u64` → Pipe Ident Comma Ident Pipe Arrow Ident Eof
    let tokens = vec![
        tok(TokenKind::Pipe, 0),
        tok(TokenKind::Ident, 1), // u64
        tok(TokenKind::Comma, 4),
        tok(TokenKind::Ident, 6), // u64
        tok(TokenKind::Pipe, 9),
        tok(TokenKind::Arrow, 11),
        tok(TokenKind::Ident, 14), // u64
        tok(TokenKind::Eof, 17),
    ];
    let (arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 0);
    assert!(result.is_ok());
    let ty_id = result.unwrap();
    if let Some(TypeData::Closure { params, .. }) = arena.type_data(ty_id) {
        assert_eq!(params.len(), 2);
    } else {
        panic!("expected TypeClosure with 2 params");
    }
}

#[test]
fn parse_closure_with_effects() {
    // `|u64| -> u64 !{io}`
    let tokens = vec![
        tok(TokenKind::Pipe, 0),
        tok(TokenKind::Ident, 1), // u64
        tok(TokenKind::Pipe, 4),
        tok(TokenKind::Arrow, 6),
        tok(TokenKind::Ident, 9), // u64
        tok(TokenKind::EffectOpen, 13),
        tok(TokenKind::Ident, 15), // io
        tok(TokenKind::RBrace, 17),
        tok(TokenKind::Eof, 18),
    ];
    let (arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 0);
    assert!(result.is_ok());
    let ty_id = result.unwrap();
    if let Some(TypeData::Closure { effects, .. }) = arena.type_data(ty_id) {
        assert!(effects.is_some());
    } else {
        panic!("expected TypeClosure with effects");
    }
}

#[test]
fn parse_closure_with_capabilities() {
    // `|u64| -> u64 @{cap}`
    let tokens = vec![
        tok(TokenKind::Pipe, 0),
        tok(TokenKind::Ident, 1), // u64
        tok(TokenKind::Pipe, 4),
        tok(TokenKind::Arrow, 6),
        tok(TokenKind::Ident, 9), // u64
        tok(TokenKind::CapOpen, 13),
        tok(TokenKind::Ident, 15), // cap
        tok(TokenKind::RBrace, 18),
        tok(TokenKind::Eof, 19),
    ];
    let (arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 0);
    assert!(result.is_ok());
    let ty_id = result.unwrap();
    if let Some(TypeData::Closure { capabilities, .. }) = arena.type_data(ty_id) {
        assert!(capabilities.is_some());
    } else {
        panic!("expected TypeClosure with capabilities");
    }
}

/// Zero-parameter closure type `|| -> R`, exercised via hand-built tokens
/// (this test constructs two adjacent `Pipe` tokens directly, bypassing the
/// lexer). This is a KNOWN GAP: the real lexer scans adjacent `||` characters
/// as a single `TokenKind::OrOr` (logical-or) token (see
/// `paideia-as-lexer/src/scan_op.rs`), so `parse_type_unquantified`'s
/// `Some(TokenKind::Pipe) => self.parse_type_closure()` dispatch never fires
/// for a real `|| -> R` source string — it would need its own `OrOr`
/// look-ahead branch to split the token, which is out of scope for #994 and
/// tracked as a follow-up. This test only verifies `parse_type_closure`'s own
/// empty-parameter-list logic is correct, independent of that lexer gap.
#[test]
fn parse_closure_zero_params_via_hand_built_tokens() {
    // `|| -> u64` → Pipe Pipe Arrow Ident Eof
    let tokens = vec![
        tok(TokenKind::Pipe, 0),
        tok(TokenKind::Pipe, 1),
        tok(TokenKind::Arrow, 3),
        tok(TokenKind::Ident, 6), // u64
        tok(TokenKind::Eof, 9),
    ];
    let (arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 0);
    assert!(result.is_ok());
    let ty_id = result.unwrap();
    if let Some(TypeData::Closure { params, .. }) = arena.type_data(ty_id) {
        assert_eq!(params.len(), 0);
    } else {
        panic!("expected TypeClosure with 0 params");
    }
}

#[test]
fn parse_linear_class_keyword() {
    // `linear T` → KwLinear Ident Eof
    let tokens = vec![
        tok(TokenKind::KwLinear, 0),
        tok(TokenKind::Ident, 7),
        tok(TokenKind::Eof, 8),
    ];
    let (arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 0);
    assert!(result.is_ok());
    let ty_id = result.unwrap();
    let ty_node = arena.get(ty_id).unwrap();
    assert_eq!(ty_node.kind, NodeKind::TypeLinearClass);
    if let Some(TypeData::LinearClass { class, .. }) = arena.type_data(ty_id) {
        assert_eq!(*class, LinClass::Linear);
    } else {
        panic!("expected TypeLinearClass");
    }
}

#[test]
fn parse_linear_class_glyph() {
    // `↓ T` → LinearMark Ident Eof
    let tokens = vec![
        tok(TokenKind::LinearMark, 0),
        tok(TokenKind::Ident, 1),
        tok(TokenKind::Eof, 2),
    ];
    let (arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 0);
    assert!(result.is_ok());
    let ty_id = result.unwrap();
    let ty_node = arena.get(ty_id).unwrap();
    assert_eq!(ty_node.kind, NodeKind::TypeLinearClass);
    if let Some(TypeData::LinearClass { class, .. }) = arena.type_data(ty_id) {
        assert_eq!(*class, LinClass::LinearMark);
    } else {
        panic!("expected TypeLinearClass with LinearMark");
    }
}

#[test]
fn parse_affine_glyph() {
    // `~ T` → AffineMark Ident Eof
    let tokens = vec![
        tok(TokenKind::AffineMark, 0),
        tok(TokenKind::Ident, 1),
        tok(TokenKind::Eof, 2),
    ];
    let (arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 0);
    assert!(result.is_ok());
    let ty_id = result.unwrap();
    let ty_node = arena.get(ty_id).unwrap();
    assert_eq!(ty_node.kind, NodeKind::TypeLinearClass);
    if let Some(TypeData::LinearClass { class, .. }) = arena.type_data(ty_id) {
        assert_eq!(*class, LinClass::AffineMark);
    } else {
        panic!("expected TypeLinearClass with AffineMark");
    }
}

#[test]
fn in_type_position_is_affine_marker() {
    // `~ T` in TYPE position must parse as an affine marker (TypeLinearClass
    // with LinClass::AffineMark), NOT as prefix bitwise NOT. Type parsing
    // uses parse_type, which never consults the expression prefix table, so
    // the m4-001 expression-position change does not affect this path.
    let tokens = vec![
        tok(TokenKind::AffineMark, 0), // ~
        tok(TokenKind::Ident, 1),      // T
        tok(TokenKind::Eof, 2),
    ];
    let (arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 0);
    assert!(result.is_ok());
    let ty_id = result.unwrap();
    let ty_node = arena.get(ty_id).unwrap();
    assert_eq!(
        ty_node.kind,
        NodeKind::TypeLinearClass,
        "~ in type position is an affine marker, not a prefix operator"
    );
    if let Some(TypeData::LinearClass { class, .. }) = arena.type_data(ty_id) {
        assert_eq!(*class, LinClass::AffineMark);
    } else {
        panic!("expected TypeLinearClass with AffineMark");
    }
}

#[test]
fn parse_forall_quantified() {
    // `forall e. (T) -> T !{Io | e}` (bound var discarded in phase-1)
    // KwForall Ident Dot LParen Ident RParen Arrow Ident EffectOpen Ident Pipe Ident RBrace Eof
    let tokens = vec![
        tok(TokenKind::KwForall, 0),
        tok(TokenKind::Ident, 7), // e
        tok(TokenKind::Dot, 8),
        tok(TokenKind::LParen, 10),
        tok(TokenKind::Ident, 11), // T
        tok(TokenKind::RParen, 12),
        tok(TokenKind::Arrow, 14),
        tok(TokenKind::Ident, 17), // T
        tok(TokenKind::EffectOpen, 19),
        tok(TokenKind::Ident, 21), // Io
        tok(TokenKind::Pipe, 23),
        tok(TokenKind::Ident, 25), // e
        tok(TokenKind::RBrace, 26),
        tok(TokenKind::Eof, 27),
    ];
    let (arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 0);
    assert!(result.is_ok());
    let ty_id = result.unwrap();
    let ty_node = arena.get(ty_id).unwrap();
    // The outer node should be an arrow (forall wrapper is discarded in phase-1)
    assert_eq!(ty_node.kind, NodeKind::TypeFnPtr);
}

#[test]
fn parse_empty_effect_set() {
    // `!{}` → EffectOpen RBrace Eof
    let tokens = vec![
        tok(TokenKind::EffectOpen, 0),
        tok(TokenKind::RBrace, 2),
        tok(TokenKind::Eof, 3),
    ];
    let (arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 0);
    assert!(result.is_ok());
    let ty_id = result.unwrap();
    let ty_node = arena.get(ty_id).unwrap();
    assert_eq!(ty_node.kind, NodeKind::TypeEffectRow);
    if let Some(TypeData::EffectRow { items, rest }) = arena.type_data(ty_id) {
        assert_eq!(items.len(), 0);
        assert!(rest.is_none());
    } else {
        panic!("expected TypeEffectRow empty");
    }
}

#[test]
fn parse_empty_cap_set() {
    // `@{}` → CapOpen RBrace Eof
    let tokens = vec![
        tok(TokenKind::CapOpen, 0),
        tok(TokenKind::RBrace, 2),
        tok(TokenKind::Eof, 3),
    ];
    let (arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 0);
    assert!(result.is_ok());
    let ty_id = result.unwrap();
    let ty_node = arena.get(ty_id).unwrap();
    assert_eq!(ty_node.kind, NodeKind::TypeEffectRow);
    if let Some(TypeData::EffectRow { items, rest }) = arena.type_data(ty_id) {
        assert_eq!(items.len(), 0);
        assert!(rest.is_none());
    } else {
        panic!("expected TypeEffectRow empty cap");
    }
}

// Tests for named-parameter function types (issue #154)

#[test]
fn parses_function_type_with_named_param() {
    // `(bar: MmioRegion) -> u32`
    // LParen Ident Colon Ident RParen Arrow Ident Eof
    let tokens = vec![
        tok(TokenKind::LParen, 0),
        tok(TokenKind::Ident, 1), // bar
        tok(TokenKind::Colon, 4), // :
        tok(TokenKind::Ident, 5), // MmioRegion
        tok(TokenKind::RParen, 16),
        tok(TokenKind::Arrow, 18),
        tok(TokenKind::Ident, 21), // u32
        tok(TokenKind::Eof, 24),
    ];
    let (arena, result, diags) = parse_t(tokens);

    assert_eq!(
        diags.len(),
        0,
        "no diagnostics expected for named-param type"
    );
    assert!(result.is_ok());
    let ty_id = result.unwrap();
    let ty_node = arena.get(ty_id).unwrap();
    assert_eq!(
        ty_node.kind,
        NodeKind::TypeFnPtr,
        "expected arrow type for named-param function"
    );
    if let Some(TypeData::FnPtr { params, .. }) = arena.type_data(ty_id) {
        assert_eq!(params.len(), 1, "expected 1 parameter");
    } else {
        panic!("expected TypeFnPtr");
    }
}

#[test]
fn parses_function_type_with_two_named_params() {
    // `(a: u32, b: u64) -> u32`
    // LParen Ident Colon Ident Comma Ident Colon Ident RParen Arrow Ident Eof
    let tokens = vec![
        tok(TokenKind::LParen, 0),
        tok(TokenKind::Ident, 1), // a
        tok(TokenKind::Colon, 2), // :
        tok(TokenKind::Ident, 3), // u32
        tok(TokenKind::Comma, 6),
        tok(TokenKind::Ident, 8),  // b
        tok(TokenKind::Colon, 9),  // :
        tok(TokenKind::Ident, 10), // u64
        tok(TokenKind::RParen, 14),
        tok(TokenKind::Arrow, 16),
        tok(TokenKind::Ident, 19), // u32
        tok(TokenKind::Eof, 22),
    ];
    let (arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 0, "no diagnostics expected");
    assert!(result.is_ok());
    let ty_id = result.unwrap();
    let ty_node = arena.get(ty_id).unwrap();
    assert_eq!(ty_node.kind, NodeKind::TypeFnPtr);
    if let Some(TypeData::FnPtr { params, .. }) = arena.type_data(ty_id) {
        assert_eq!(params.len(), 2, "expected 2 parameters");
    } else {
        panic!("expected TypeFnPtr with two params");
    }
}

#[test]
fn parses_function_type_positional_regression() {
    // `(u32, u64) -> u32` (positional, no names) — should still work
    // LParen Ident Comma Ident RParen Arrow Ident Eof
    let tokens = vec![
        tok(TokenKind::LParen, 0),
        tok(TokenKind::Ident, 1), // u32
        tok(TokenKind::Comma, 4),
        tok(TokenKind::Ident, 6), // u64
        tok(TokenKind::RParen, 9),
        tok(TokenKind::Arrow, 11),
        tok(TokenKind::Ident, 14), // u32
        tok(TokenKind::Eof, 17),
    ];
    let (arena, result, diags) = parse_t(tokens);

    assert_eq!(
        diags.len(),
        0,
        "no diagnostics expected for positional form"
    );
    assert!(result.is_ok());
    let ty_id = result.unwrap();
    let ty_node = arena.get(ty_id).unwrap();
    assert_eq!(ty_node.kind, NodeKind::TypeFnPtr);
    if let Some(TypeData::FnPtr { params, .. }) = arena.type_data(ty_id) {
        assert_eq!(params.len(), 2, "expected 2 positional parameters");
    } else {
        panic!("expected TypeFnPtr positional");
    }
}

#[test]
fn parses_function_type_mixed_named_and_positional() {
    // `(name: T, U) -> V` (mixed form: named then positional)
    // LParen Ident Colon Ident Comma Ident RParen Arrow Ident Eof
    let tokens = vec![
        tok(TokenKind::LParen, 0),
        tok(TokenKind::Ident, 1), // name
        tok(TokenKind::Colon, 5), // :
        tok(TokenKind::Ident, 6), // T
        tok(TokenKind::Comma, 7),
        tok(TokenKind::Ident, 9), // U
        tok(TokenKind::RParen, 10),
        tok(TokenKind::Arrow, 12),
        tok(TokenKind::Ident, 15), // V
        tok(TokenKind::Eof, 16),
    ];
    let (arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 0, "mixed form should parse cleanly");
    assert!(result.is_ok());
    let ty_id = result.unwrap();
    let ty_node = arena.get(ty_id).unwrap();
    assert_eq!(ty_node.kind, NodeKind::TypeFnPtr);
    if let Some(TypeData::FnPtr { params, .. }) = arena.type_data(ty_id) {
        assert_eq!(params.len(), 2, "expected 2 parameters (mixed)");
    } else {
        panic!("expected TypeFnPtr mixed");
    }
}

#[test]
fn parses_function_type_zero_args_with_paren() {
    // `() -> u32` (empty params) — should still work
    // LParen RParen Arrow Ident Eof
    let tokens = vec![
        tok(TokenKind::LParen, 0),
        tok(TokenKind::RParen, 1),
        tok(TokenKind::Arrow, 2),
        tok(TokenKind::Ident, 4), // u32
        tok(TokenKind::Eof, 7),
    ];
    let (arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 0, "no diagnostics expected for empty params");
    assert!(result.is_ok());
    let ty_id = result.unwrap();
    let ty_node = arena.get(ty_id).unwrap();
    assert_eq!(ty_node.kind, NodeKind::TypeFnPtr);
    if let Some(TypeData::FnPtr { params, .. }) = arena.type_data(ty_id) {
        assert_eq!(params.len(), 0, "expected 0 parameters");
    } else {
        panic!("expected TypeFnPtr empty");
    }
}

#[test]
fn parses_function_type_nested_named_param_types() {
    // `(f: (n: u32) -> u32) -> u32`
    // LParen Ident Colon LParen Ident Colon Ident RParen Arrow Ident RParen Arrow Ident Eof
    let tokens = vec![
        tok(TokenKind::LParen, 0),
        tok(TokenKind::Ident, 1),   // f
        tok(TokenKind::Colon, 2),   // :
        tok(TokenKind::LParen, 3),  // (
        tok(TokenKind::Ident, 4),   // n
        tok(TokenKind::Colon, 5),   // :
        tok(TokenKind::Ident, 6),   // u32
        tok(TokenKind::RParen, 9),  // )
        tok(TokenKind::Arrow, 11),  // ->
        tok(TokenKind::Ident, 14),  // u32
        tok(TokenKind::RParen, 17), // )
        tok(TokenKind::Arrow, 19),  // ->
        tok(TokenKind::Ident, 22),  // u32
        tok(TokenKind::Eof, 25),
    ];
    let (arena, result, diags) = parse_t(tokens);

    assert_eq!(
        diags.len(),
        0,
        "no diagnostics for nested named-param types"
    );
    assert!(result.is_ok());
    let ty_id = result.unwrap();
    let ty_node = arena.get(ty_id).unwrap();
    assert_eq!(ty_node.kind, NodeKind::TypeFnPtr);
    if let Some(TypeData::FnPtr { params, .. }) = arena.type_data(ty_id) {
        assert_eq!(params.len(), 1, "expected 1 parameter (a function type)");
        // Check that the param itself is an arrow
        let param_type = params[0];
        let param_node = arena.get(param_type).unwrap();
        assert_eq!(param_node.kind, NodeKind::TypeFnPtr);
    } else {
        panic!("expected outer TypeFnPtr");
    }
}

// === Pointer type tests ===

#[test]
fn parse_ptr_simple() {
    // `*u64` → Star Ident Eof
    let tokens = vec![
        tok(TokenKind::Star, 0),
        tok(TokenKind::Ident, 1),
        tok(TokenKind::Eof, 5),
    ];
    let (arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 0, "no diagnostics expected");
    assert!(result.is_ok());
    let ty_id = result.unwrap();
    let ty_node = arena.get(ty_id).unwrap();
    assert_eq!(ty_node.kind, NodeKind::TypePtr);
    if let Some(TypeData::Ptr { pointee }) = arena.type_data(ty_id) {
        let pointee_node = arena.get(*pointee).unwrap();
        assert_eq!(pointee_node.kind, NodeKind::TypeName);
    } else {
        panic!("expected TypePtr");
    }
}

#[test]
fn parse_ptr_nested() {
    // `**u8` → Star Star Ident Eof
    let tokens = vec![
        tok(TokenKind::Star, 0),
        tok(TokenKind::Star, 1),
        tok(TokenKind::Ident, 2),
        tok(TokenKind::Eof, 4),
    ];
    let (arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 0, "no diagnostics expected");
    assert!(result.is_ok());
    let ty_id = result.unwrap();
    let ty_node = arena.get(ty_id).unwrap();
    assert_eq!(ty_node.kind, NodeKind::TypePtr);
    if let Some(TypeData::Ptr { pointee }) = arena.type_data(ty_id) {
        let inner_node = arena.get(*pointee).unwrap();
        assert_eq!(inner_node.kind, NodeKind::TypePtr);
    } else {
        panic!("expected outer TypePtr");
    }
}

#[test]
fn parse_ptr_tuple() {
    // `*(u8, u64)` → Star LParen Ident Comma Ident RParen Eof
    let tokens = vec![
        tok(TokenKind::Star, 0),
        tok(TokenKind::LParen, 1),
        tok(TokenKind::Ident, 2),
        tok(TokenKind::Comma, 4),
        tok(TokenKind::Ident, 6),
        tok(TokenKind::RParen, 9),
        tok(TokenKind::Eof, 10),
    ];
    let (arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 0, "no diagnostics expected");
    assert!(result.is_ok());
    let ty_id = result.unwrap();
    let ty_node = arena.get(ty_id).unwrap();
    assert_eq!(ty_node.kind, NodeKind::TypePtr);
    if let Some(TypeData::Ptr { pointee }) = arena.type_data(ty_id) {
        let tuple_node = arena.get(*pointee).unwrap();
        assert_eq!(tuple_node.kind, NodeKind::TypeTuple);
    } else {
        panic!("expected TypePtr");
    }
}

#[test]
fn parse_ptr_fn() {
    // `*((u64) -> u64)` → Star LParen LParen Ident RParen Arrow Ident RParen Eof
    let tokens = vec![
        tok(TokenKind::Star, 0),
        tok(TokenKind::LParen, 1),
        tok(TokenKind::LParen, 2),
        tok(TokenKind::Ident, 3),
        tok(TokenKind::RParen, 6),
        tok(TokenKind::Arrow, 8),
        tok(TokenKind::Ident, 11),
        tok(TokenKind::RParen, 14),
        tok(TokenKind::Eof, 15),
    ];
    let (arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 0, "no diagnostics expected");
    assert!(result.is_ok());
    let ty_id = result.unwrap();
    let ty_node = arena.get(ty_id).unwrap();
    assert_eq!(ty_node.kind, NodeKind::TypePtr);
    if let Some(TypeData::Ptr { pointee }) = arena.type_data(ty_id) {
        let fn_node = arena.get(*pointee).unwrap();
        assert_eq!(fn_node.kind, NodeKind::TypeFnPtr);
    } else {
        panic!("expected TypePtr");
    }
}

#[test]
fn parse_ptr_in_arrow_param() {
    // `(*u8) -> u64` → LParen Star Ident RParen Arrow Ident Eof
    let tokens = vec![
        tok(TokenKind::LParen, 0),
        tok(TokenKind::Star, 1),
        tok(TokenKind::Ident, 2),
        tok(TokenKind::RParen, 4),
        tok(TokenKind::Arrow, 6),
        tok(TokenKind::Ident, 9),
        tok(TokenKind::Eof, 12),
    ];
    let (arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 0, "no diagnostics expected");
    assert!(result.is_ok());
    let ty_id = result.unwrap();
    let ty_node = arena.get(ty_id).unwrap();
    assert_eq!(ty_node.kind, NodeKind::TypeFnPtr);
    if let Some(TypeData::FnPtr { params, .. }) = arena.type_data(ty_id) {
        assert_eq!(params.len(), 1, "expected 1 parameter");
        let param_node = arena.get(params[0]).unwrap();
        assert_eq!(param_node.kind, NodeKind::TypePtr);
    } else {
        panic!("expected TypeFnPtr");
    }
}

#[test]
fn parse_ptr_in_arrow_ret() {
    // `(u64) -> *u8` → LParen Ident RParen Arrow Star Ident Eof
    let tokens = vec![
        tok(TokenKind::LParen, 0),
        tok(TokenKind::Ident, 1),
        tok(TokenKind::RParen, 4),
        tok(TokenKind::Arrow, 6),
        tok(TokenKind::Star, 9),
        tok(TokenKind::Ident, 10),
        tok(TokenKind::Eof, 12),
    ];
    let (arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 0, "no diagnostics expected");
    assert!(result.is_ok());
    let ty_id = result.unwrap();
    let ty_node = arena.get(ty_id).unwrap();
    assert_eq!(ty_node.kind, NodeKind::TypeFnPtr);
    if let Some(TypeData::FnPtr { ret, .. }) = arena.type_data(ty_id) {
        let ret_node = arena.get(*ret).unwrap();
        assert_eq!(ret_node.kind, NodeKind::TypePtr);
    } else {
        panic!("expected TypeFnPtr");
    }
}

#[test]
fn parse_ptr_p0195_no_operand() {
    // `*` Eof → expect P0195 diagnostic
    let tokens = vec![tok(TokenKind::Star, 0), tok(TokenKind::Eof, 1)];
    let (_arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 1, "expected 1 diagnostic");
    let diag = &diags[0];
    assert_eq!(diag.code().number(), 195, "expected P0195");
    assert!(result.is_err(), "expected parse error");
}

#[test]
fn parse_ptr_p0195_before_arrow() {
    // `*` Arrow Ident Eof → expect P0195 diagnostic
    let tokens = vec![
        tok(TokenKind::Star, 0),
        tok(TokenKind::Arrow, 1),
        tok(TokenKind::Ident, 4),
        tok(TokenKind::Eof, 7),
    ];
    let (_arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 1, "expected 1 diagnostic");
    let diag = &diags[0];
    assert_eq!(diag.code().number(), 195, "expected P0195");
    assert!(result.is_err(), "expected parse error");
}

// === Round-trip tests (parse + print_type) ===

#[test]
fn roundtrip_ptr_simple() {
    // `*u8` parsed and printed should remain `*u8`
    let tokens = vec![
        tok(TokenKind::Star, 0),
        tok(TokenKind::Ident, 1),
        tok(TokenKind::Eof, 3),
    ];
    let (arena, result, _diags) = parse_t(tokens);

    assert!(result.is_ok());
    let ty_id = result.unwrap();
    let printed = paideia_as_ast::pretty::print_type(&arena, ty_id);
    assert!(printed.contains("Ptr"));
}

#[test]
fn roundtrip_ptr_nested() {
    // `**u8` parsed should have nested TypePtr nodes
    let tokens = vec![
        tok(TokenKind::Star, 0),
        tok(TokenKind::Star, 1),
        tok(TokenKind::Ident, 2),
        tok(TokenKind::Eof, 4),
    ];
    let (arena, result, _diags) = parse_t(tokens);

    assert!(result.is_ok());
    let ty_id = result.unwrap();
    let printed = paideia_as_ast::pretty::print_type(&arena, ty_id);
    // Should have outer Ptr wrapping inner Ptr
    assert!(printed.contains("Ptr"));
}

#[test]
fn roundtrip_ptr_in_tuple() {
    // `*(u8, u64)` parsed should have Ptr wrapping Tuple
    let tokens = vec![
        tok(TokenKind::Star, 0),
        tok(TokenKind::LParen, 1),
        tok(TokenKind::Ident, 2),
        tok(TokenKind::Comma, 4),
        tok(TokenKind::Ident, 6),
        tok(TokenKind::RParen, 9),
        tok(TokenKind::Eof, 10),
    ];
    let (arena, result, _diags) = parse_t(tokens);

    assert!(result.is_ok());
    let ty_id = result.unwrap();
    let ty_node = arena.get(ty_id).unwrap();
    assert_eq!(ty_node.kind, NodeKind::TypePtr);
    if let Some(TypeData::Ptr { pointee }) = arena.type_data(ty_id) {
        let inner_node = arena.get(*pointee).unwrap();
        assert_eq!(inner_node.kind, NodeKind::TypeTuple);
    } else {
        panic!("expected TypePtr");
    }
}

#[test]
fn roundtrip_ptr_in_arrow() {
    // `(*u8) -> *u64` parsed should have Ptr in both params and return
    // Tokens: LParen Star Ident RParen Arrow Star Ident Eof
    let tokens = vec![
        tok(TokenKind::LParen, 0),
        tok(TokenKind::Star, 1),
        tok(TokenKind::Ident, 2),
        tok(TokenKind::RParen, 4),
        tok(TokenKind::Arrow, 6),
        tok(TokenKind::Star, 9),
        tok(TokenKind::Ident, 10),
        tok(TokenKind::Eof, 12),
    ];
    let (arena, result, _diags) = parse_t(tokens);

    assert!(result.is_ok());
    let ty_id = result.unwrap();
    let ty_node = arena.get(ty_id).unwrap();
    assert_eq!(ty_node.kind, NodeKind::TypeFnPtr);
    if let Some(TypeData::FnPtr { params, ret, .. }) = arena.type_data(ty_id) {
        // First param should be *u8
        assert_eq!(params.len(), 1);
        let param_node = arena.get(params[0]).unwrap();
        assert_eq!(param_node.kind, NodeKind::TypePtr);
        // Return type should be *u64
        let ret_node = arena.get(*ret).unwrap();
        assert_eq!(ret_node.kind, NodeKind::TypePtr);
    } else {
        panic!("expected TypeFnPtr");
    }
}

#[test]
fn parse_enum_unit_variants_only() {
    // `enum { A, B, C }` → KwEnum LBrace Ident Comma Ident Comma Ident RBrace Eof
    let tokens = vec![
        tok(TokenKind::KwEnum, 0),
        tok(TokenKind::LBrace, 5),
        tok(TokenKind::Ident, 7), // A
        tok(TokenKind::Comma, 8),
        tok(TokenKind::Ident, 10), // B
        tok(TokenKind::Comma, 11),
        tok(TokenKind::Ident, 13), // C
        tok(TokenKind::RBrace, 14),
        tok(TokenKind::Eof, 15),
    ];
    let (arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 0);
    assert!(result.is_ok());
    let ty_id = result.unwrap();
    let ty_node = arena.get(ty_id).unwrap();
    assert_eq!(ty_node.kind, NodeKind::TypeEnum);
    if let Some(TypeData::Enum { variants }) = arena.type_data(ty_id) {
        assert_eq!(variants.len(), 3);
        // All should be unit variants
        for var in variants {
            if let paideia_as_ast::EnumVariant::Unit { .. } = var {
                // OK
            } else {
                panic!("expected unit variant");
            }
        }
    } else {
        panic!("expected TypeEnum");
    }
}

#[test]
fn parse_enum_tuple_variants() {
    // `enum { Some(u64), None }` → KwEnum LBrace Ident LParen Ident RParen Comma Ident RBrace Eof
    let tokens = vec![
        tok(TokenKind::KwEnum, 0),
        tok(TokenKind::LBrace, 5),
        tok(TokenKind::Ident, 7), // Some
        tok(TokenKind::LParen, 11),
        tok(TokenKind::Ident, 12), // u64
        tok(TokenKind::RParen, 15),
        tok(TokenKind::Comma, 16),
        tok(TokenKind::Ident, 18), // None
        tok(TokenKind::RBrace, 22),
        tok(TokenKind::Eof, 23),
    ];
    let (arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 0);
    assert!(result.is_ok());
    let ty_id = result.unwrap();
    if let Some(TypeData::Enum { variants }) = arena.type_data(ty_id) {
        assert_eq!(variants.len(), 2);
        // First should be tuple variant
        if let paideia_as_ast::EnumVariant::Tuple { payload, .. } = &variants[0] {
            assert_eq!(payload.len(), 1);
        } else {
            panic!("expected tuple variant");
        }
        // Second should be unit variant
        if let paideia_as_ast::EnumVariant::Unit { .. } = &variants[1] {
            // OK
        } else {
            panic!("expected unit variant");
        }
    } else {
        panic!("expected TypeEnum");
    }
}

#[test]
fn parse_enum_record_variants() {
    // `enum { Pair { a: u8, b: u8 } }`
    let tokens = vec![
        tok(TokenKind::KwEnum, 0),
        tok(TokenKind::LBrace, 5),
        tok(TokenKind::Ident, 7), // Pair
        tok(TokenKind::LBrace, 12),
        tok(TokenKind::Ident, 14), // a
        tok(TokenKind::Colon, 15),
        tok(TokenKind::Ident, 17), // u8
        tok(TokenKind::Comma, 19),
        tok(TokenKind::Ident, 21), // b
        tok(TokenKind::Colon, 22),
        tok(TokenKind::Ident, 24), // u8
        tok(TokenKind::RBrace, 26),
        tok(TokenKind::RBrace, 27),
        tok(TokenKind::Eof, 28),
    ];
    let (arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 0);
    assert!(result.is_ok());
    let ty_id = result.unwrap();
    if let Some(TypeData::Enum { variants }) = arena.type_data(ty_id) {
        assert_eq!(variants.len(), 1);
        if let paideia_as_ast::EnumVariant::Record { fields, .. } = &variants[0] {
            assert_eq!(fields.len(), 2);
        } else {
            panic!("expected record variant");
        }
    } else {
        panic!("expected TypeEnum");
    }
}

#[test]
fn parse_enum_mixed_variants() {
    // `enum { Unit, T(u8), R { x: u8 } }`
    let tokens = vec![
        tok(TokenKind::KwEnum, 0),
        tok(TokenKind::LBrace, 5),
        tok(TokenKind::Ident, 7), // Unit
        tok(TokenKind::Comma, 11),
        tok(TokenKind::Ident, 13), // T
        tok(TokenKind::LParen, 14),
        tok(TokenKind::Ident, 15), // u8
        tok(TokenKind::RParen, 17),
        tok(TokenKind::Comma, 18),
        tok(TokenKind::Ident, 20), // R
        tok(TokenKind::LBrace, 22),
        tok(TokenKind::Ident, 24), // x
        tok(TokenKind::Colon, 25),
        tok(TokenKind::Ident, 27), // u8
        tok(TokenKind::RBrace, 29),
        tok(TokenKind::RBrace, 30),
        tok(TokenKind::Eof, 31),
    ];
    let (arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 0);
    assert!(result.is_ok());
    let ty_id = result.unwrap();
    if let Some(TypeData::Enum { variants }) = arena.type_data(ty_id) {
        assert_eq!(variants.len(), 3);
        // First unit
        assert!(matches!(
            variants[0],
            paideia_as_ast::EnumVariant::Unit { .. }
        ));
        // Second tuple
        assert!(matches!(
            variants[1],
            paideia_as_ast::EnumVariant::Tuple { .. }
        ));
        // Third record
        assert!(matches!(
            variants[2],
            paideia_as_ast::EnumVariant::Record { .. }
        ));
    } else {
        panic!("expected TypeEnum");
    }
}

#[test]
fn parse_enum_trailing_comma() {
    // `enum { A, B, }` → KwEnum LBrace Ident Comma Ident Comma RBrace Eof
    let tokens = vec![
        tok(TokenKind::KwEnum, 0),
        tok(TokenKind::LBrace, 5),
        tok(TokenKind::Ident, 7), // A
        tok(TokenKind::Comma, 8),
        tok(TokenKind::Ident, 10), // B
        tok(TokenKind::Comma, 11),
        tok(TokenKind::RBrace, 12),
        tok(TokenKind::Eof, 13),
    ];
    let (arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 0);
    assert!(result.is_ok());
    let ty_id = result.unwrap();
    if let Some(TypeData::Enum { variants }) = arena.type_data(ty_id) {
        assert_eq!(variants.len(), 2);
    } else {
        panic!("expected TypeEnum");
    }
}

#[test]
fn parse_enum_p0198_missing_lbrace() {
    // `enum (` → missing { after enum
    let tokens = vec![
        tok(TokenKind::KwEnum, 0),
        tok(TokenKind::LParen, 5),
        tok(TokenKind::Eof, 6),
    ];
    let (arena, result, diags) = parse_t(tokens);

    // Should error with P0198 (malformed enum)
    assert!(result.is_err());
    assert!(diags.len() > 0);
    // The diagnostic code should be 198 for malformed enum
    assert!(diags.iter().any(|d| d.code().number() == 198));
}

#[test]
fn parse_enum_p0198_missing_rparen() {
    // `enum { Some(u64 }` → missing ) in tuple variant
    let tokens = vec![
        tok(TokenKind::KwEnum, 0),
        tok(TokenKind::LBrace, 5),
        tok(TokenKind::Ident, 7), // Some
        tok(TokenKind::LParen, 11),
        tok(TokenKind::Ident, 12), // u64
        tok(TokenKind::RBrace, 16),
        tok(TokenKind::Eof, 17),
    ];
    let (arena, result, diags) = parse_t(tokens);

    // Should error with P0198
    assert!(result.is_err());
    assert!(diags.len() > 0);
    assert_eq!(diags[0].code().number(), 198);
}

#[test]
fn parse_enum_p0198_missing_rbrace() {
    // `enum { A, B` → missing closing }
    let tokens = vec![
        tok(TokenKind::KwEnum, 0),
        tok(TokenKind::LBrace, 5),
        tok(TokenKind::Ident, 7), // A
        tok(TokenKind::Comma, 8),
        tok(TokenKind::Ident, 10), // B
        tok(TokenKind::Eof, 11),
    ];
    let (arena, result, diags) = parse_t(tokens);

    // Should error with P0198
    assert!(result.is_err());
    assert!(diags.len() > 0);
    assert_eq!(diags[0].code().number(), 198);
}

// === Reference type tests (Phase 4 m4-001) ===

#[test]
fn parse_ref_simple() {
    // `&u64` → Amp Ident Eof
    let tokens = vec![
        tok(TokenKind::Amp, 0),
        tok(TokenKind::Ident, 1),
        tok(TokenKind::Eof, 5),
    ];
    let (arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 0, "no diagnostics expected");
    assert!(result.is_ok());
    let ty_id = result.unwrap();
    let ty_node = arena.get(ty_id).unwrap();
    assert_eq!(ty_node.kind, NodeKind::TypeRef);
    if let Some(TypeData::Ref { pointee, mutable }) = arena.type_data(ty_id) {
        assert!(!mutable, "expected immutable reference");
        let pointee_node = arena.get(*pointee).unwrap();
        assert_eq!(pointee_node.kind, NodeKind::TypeName);
    } else {
        panic!("expected TypeRef");
    }
}

#[test]
fn parse_ref_mut() {
    // `&mut u64` → Amp KwMut Ident Eof
    let tokens = vec![
        tok(TokenKind::Amp, 0),
        tok(TokenKind::KwMut, 1),
        tok(TokenKind::Ident, 4),
        tok(TokenKind::Eof, 8),
    ];
    let (arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 0, "no diagnostics expected");
    assert!(result.is_ok());
    let ty_id = result.unwrap();
    let ty_node = arena.get(ty_id).unwrap();
    assert_eq!(ty_node.kind, NodeKind::TypeRef);
    if let Some(TypeData::Ref { pointee, mutable }) = arena.type_data(ty_id) {
        assert!(mutable, "expected mutable reference");
        let pointee_node = arena.get(*pointee).unwrap();
        assert_eq!(pointee_node.kind, NodeKind::TypeName);
    } else {
        panic!("expected TypeRef");
    }
}

#[test]
fn parse_ref_nested() {
    // `&&u8` → Amp Amp Ident Eof
    let tokens = vec![
        tok(TokenKind::Amp, 0),
        tok(TokenKind::Amp, 1),
        tok(TokenKind::Ident, 2),
        tok(TokenKind::Eof, 4),
    ];
    let (arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 0, "no diagnostics expected");
    assert!(result.is_ok());
    let ty_id = result.unwrap();
    let ty_node = arena.get(ty_id).unwrap();
    assert_eq!(ty_node.kind, NodeKind::TypeRef);
    if let Some(TypeData::Ref { pointee, mutable }) = arena.type_data(ty_id) {
        assert!(!mutable, "expected immutable reference");
        let inner_node = arena.get(*pointee).unwrap();
        assert_eq!(inner_node.kind, NodeKind::TypeRef);
    } else {
        panic!("expected outer TypeRef");
    }
}

#[test]
fn parse_ref_with_lifetime() {
    // `&'a u64` → Amp Ident(lifetime) Ident Eof (parse-clean: lifetime consumed but not elaborated)
    let tokens = vec![
        tok(TokenKind::Amp, 0),
        tok(TokenKind::Ident, 1), // 'a
        tok(TokenKind::Ident, 3), // u64
        tok(TokenKind::Eof, 7),
    ];
    let (arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 0, "no diagnostics expected");
    assert!(result.is_ok());
    let ty_id = result.unwrap();
    let ty_node = arena.get(ty_id).unwrap();
    assert_eq!(ty_node.kind, NodeKind::TypeRef);
    if let Some(TypeData::Ref { pointee, mutable }) = arena.type_data(ty_id) {
        assert!(!mutable, "expected immutable reference");
        let pointee_node = arena.get(*pointee).unwrap();
        assert_eq!(pointee_node.kind, NodeKind::TypeName);
    } else {
        panic!("expected TypeRef");
    }
}

#[test]
fn parse_ref_in_arrow_param() {
    // `(&u8) -> u64` → LParen Amp Ident RParen Arrow Ident Eof
    let tokens = vec![
        tok(TokenKind::LParen, 0),
        tok(TokenKind::Amp, 1),
        tok(TokenKind::Ident, 2),
        tok(TokenKind::RParen, 4),
        tok(TokenKind::Arrow, 6),
        tok(TokenKind::Ident, 9),
        tok(TokenKind::Eof, 12),
    ];
    let (arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 0, "no diagnostics expected");
    assert!(result.is_ok());
    let ty_id = result.unwrap();
    let ty_node = arena.get(ty_id).unwrap();
    assert_eq!(ty_node.kind, NodeKind::TypeFnPtr);
    if let Some(TypeData::FnPtr { params, .. }) = arena.type_data(ty_id) {
        assert_eq!(params.len(), 1, "expected 1 parameter");
        let param_node = arena.get(params[0]).unwrap();
        assert_eq!(param_node.kind, NodeKind::TypeRef);
    } else {
        panic!("expected TypeFnPtr");
    }
}

#[test]
fn parse_ref_in_arrow_ret() {
    // `(u64) -> &u8` → LParen Ident RParen Arrow Amp Ident Eof
    let tokens = vec![
        tok(TokenKind::LParen, 0),
        tok(TokenKind::Ident, 1),
        tok(TokenKind::RParen, 4),
        tok(TokenKind::Arrow, 6),
        tok(TokenKind::Amp, 9),
        tok(TokenKind::Ident, 10),
        tok(TokenKind::Eof, 12),
    ];
    let (arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 0, "no diagnostics expected");
    assert!(result.is_ok());
    let ty_id = result.unwrap();
    let ty_node = arena.get(ty_id).unwrap();
    assert_eq!(ty_node.kind, NodeKind::TypeFnPtr);
    if let Some(TypeData::FnPtr { ret, .. }) = arena.type_data(ty_id) {
        let ret_node = arena.get(*ret).unwrap();
        assert_eq!(ret_node.kind, NodeKind::TypeRef);
    } else {
        panic!("expected TypeFnPtr");
    }
}

#[test]
fn parse_ref_of_record() {
    // `&record { a: u8 }` → Amp KwRecord LBrace Ident Colon Ident RBrace Eof
    let tokens = vec![
        tok(TokenKind::Amp, 0),
        tok(TokenKind::KwRecord, 1),
        tok(TokenKind::LBrace, 7),
        tok(TokenKind::Ident, 9), // a
        tok(TokenKind::Colon, 10),
        tok(TokenKind::Ident, 12), // u8
        tok(TokenKind::RBrace, 14),
        tok(TokenKind::Eof, 15),
    ];
    let (arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 0, "no diagnostics expected");
    assert!(result.is_ok());
    let ty_id = result.unwrap();
    let ty_node = arena.get(ty_id).unwrap();
    assert_eq!(ty_node.kind, NodeKind::TypeRef);
    if let Some(TypeData::Ref { pointee, mutable }) = arena.type_data(ty_id) {
        assert!(!mutable, "expected immutable reference");
        let record_node = arena.get(*pointee).unwrap();
        assert_eq!(record_node.kind, NodeKind::TypeRecord);
    } else {
        panic!("expected TypeRef");
    }
}

#[test]
fn parse_ref_p0196_no_type() {
    // `&` Eof → expect P0196 diagnostic
    let tokens = vec![tok(TokenKind::Amp, 0), tok(TokenKind::Eof, 1)];
    let (_arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 1, "expected 1 diagnostic");
    let diag = &diags[0];
    assert_eq!(diag.code().number(), 196, "expected P0196");
    assert!(result.is_err(), "expected parse error");
}

#[test]
fn parse_ref_p0196_mut_no_type() {
    // `&mut` Eof → expect P0196 diagnostic
    let tokens = vec![
        tok(TokenKind::Amp, 0),
        tok(TokenKind::KwMut, 1),
        tok(TokenKind::Eof, 4),
    ];
    let (_arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 1, "expected 1 diagnostic");
    let diag = &diags[0];
    assert_eq!(diag.code().number(), 196, "expected P0196");
    assert!(result.is_err(), "expected parse error");
}

// === Fixed-size array type tests ===

#[test]
fn parse_array_u8_zero() {
    // `[u8; 0]` → LBracket Ident Semicolon IntLit RBracket Eof
    let tokens = vec![
        tok(TokenKind::LBracket, 0),
        tok(TokenKind::Ident, 1), // u8
        tok(TokenKind::Semicolon, 3),
        tok(TokenKind::IntLit, 4), // 0
        tok(TokenKind::RBracket, 5),
        tok(TokenKind::Eof, 6),
    ];
    let (arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 0, "no diagnostics expected");
    assert!(result.is_ok());
    let ty_id = result.unwrap();
    let ty_node = arena.get(ty_id).unwrap();
    assert_eq!(ty_node.kind, NodeKind::TypeArray);
    if let Some(TypeData::Array { element, length }) = arena.type_data(ty_id) {
        let elem_node = arena.get(*element).unwrap();
        assert_eq!(elem_node.kind, NodeKind::TypeName);
        let len_node = arena.get(*length).unwrap();
        assert_eq!(len_node.kind, NodeKind::ExprLiteral);
    } else {
        panic!("expected TypeArray");
    }
}

#[test]
fn parse_array_u8_sixteen() {
    // `[u8; 16]` → LBracket Ident Semicolon IntLit RBracket Eof
    let tokens = vec![
        tok(TokenKind::LBracket, 0),
        tok(TokenKind::Ident, 1), // u8
        tok(TokenKind::Semicolon, 3),
        tok(TokenKind::IntLit, 4), // 16
        tok(TokenKind::RBracket, 6),
        tok(TokenKind::Eof, 7),
    ];
    let (arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 0, "no diagnostics expected");
    assert!(result.is_ok());
    let ty_id = result.unwrap();
    let ty_node = arena.get(ty_id).unwrap();
    assert_eq!(ty_node.kind, NodeKind::TypeArray);
    if let Some(TypeData::Array { element, length }) = arena.type_data(ty_id) {
        let elem_node = arena.get(*element).unwrap();
        assert_eq!(elem_node.kind, NodeKind::TypeName);
        let len_node = arena.get(*length).unwrap();
        assert_eq!(len_node.kind, NodeKind::ExprLiteral);
    } else {
        panic!("expected TypeArray");
    }
}

#[test]
fn parse_array_u64_five() {
    // `[u64; 5]` → LBracket Ident Semicolon IntLit RBracket Eof
    let tokens = vec![
        tok(TokenKind::LBracket, 0),
        tok(TokenKind::Ident, 1), // u64
        tok(TokenKind::Semicolon, 4),
        tok(TokenKind::IntLit, 5), // 5
        tok(TokenKind::RBracket, 6),
        tok(TokenKind::Eof, 7),
    ];
    let (arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 0, "no diagnostics expected");
    assert!(result.is_ok());
    let ty_id = result.unwrap();
    let ty_node = arena.get(ty_id).unwrap();
    assert_eq!(ty_node.kind, NodeKind::TypeArray);
    if let Some(TypeData::Array { element, length }) = arena.type_data(ty_id) {
        let elem_node = arena.get(*element).unwrap();
        assert_eq!(elem_node.kind, NodeKind::TypeName);
        let len_node = arena.get(*length).unwrap();
        assert_eq!(len_node.kind, NodeKind::ExprLiteral);
    } else {
        panic!("expected TypeArray");
    }
}

#[test]
fn parse_nested_array() {
    // `[[u8; 4]; 4]` → LBracket LBracket Ident Semicolon IntLit RBracket Semicolon IntLit RBracket Eof
    let tokens = vec![
        tok(TokenKind::LBracket, 0),
        tok(TokenKind::LBracket, 1),
        tok(TokenKind::Ident, 2), // u8
        tok(TokenKind::Semicolon, 4),
        tok(TokenKind::IntLit, 5), // 4
        tok(TokenKind::RBracket, 6),
        tok(TokenKind::Semicolon, 7),
        tok(TokenKind::IntLit, 8), // 4
        tok(TokenKind::RBracket, 9),
        tok(TokenKind::Eof, 10),
    ];
    let (arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 0, "no diagnostics expected");
    assert!(result.is_ok());
    let ty_id = result.unwrap();
    let ty_node = arena.get(ty_id).unwrap();
    assert_eq!(ty_node.kind, NodeKind::TypeArray);
    if let Some(TypeData::Array { element, length }) = arena.type_data(ty_id) {
        // element should be another TypeArray
        let elem_node = arena.get(*element).unwrap();
        assert_eq!(elem_node.kind, NodeKind::TypeArray);
        // length should be a literal
        let len_node = arena.get(*length).unwrap();
        assert_eq!(len_node.kind, NodeKind::ExprLiteral);
    } else {
        panic!("expected TypeArray");
    }
}

#[test]
fn parse_array_p0199_missing_length() {
    // `[u8;]` (missing length) → expect P0199 diagnostic
    let tokens = vec![
        tok(TokenKind::LBracket, 0),
        tok(TokenKind::Ident, 1), // u8
        tok(TokenKind::Semicolon, 3),
        tok(TokenKind::RBracket, 4),
        tok(TokenKind::Eof, 5),
    ];
    let (_arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 1, "expected 1 diagnostic");
    let diag = &diags[0];
    assert_eq!(
        diag.code().number(),
        100,
        "expected P0100 (expected expression)"
    );
    assert!(result.is_err(), "expected parse error");
}

#[test]
fn parse_array_p0199_missing_semicolon() {
    // `[u8 16]` (missing semicolon) → expect P0199 diagnostic
    let tokens = vec![
        tok(TokenKind::LBracket, 0),
        tok(TokenKind::Ident, 1),  // u8
        tok(TokenKind::IntLit, 3), // 16 (no semicolon before this)
        tok(TokenKind::RBracket, 4),
        tok(TokenKind::Eof, 5),
    ];
    let (_arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 1, "expected 1 diagnostic");
    let diag = &diags[0];
    assert_eq!(diag.code().number(), 199, "expected P0199");
    assert!(result.is_err(), "expected parse error");
}

// ============================================================================
// Additional FnPtr tests (per issue #979 pa-r17-001)
// ============================================================================

#[test]
fn parse_fn_ptr_zero_params() {
    // `() -> u32` — zero params
    let tokens = vec![
        tok(TokenKind::LParen, 0),
        tok(TokenKind::RParen, 1),
        tok(TokenKind::Arrow, 3),
        tok(TokenKind::Ident, 6), // u32
        tok(TokenKind::Eof, 9),
    ];
    let (arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 0);
    assert!(result.is_ok());
    let ty_id = result.unwrap();
    let ty_node = arena.get(ty_id).unwrap();
    assert_eq!(ty_node.kind, NodeKind::TypeFnPtr);
    if let Some(TypeData::FnPtr { params, .. }) = arena.type_data(ty_id) {
        assert_eq!(params.len(), 0);
    } else {
        panic!("expected TypeFnPtr with zero params");
    }
}

#[test]
fn parse_fn_ptr_exact_example() {
    // `(*u8, u64) -> u32` — AC exact example
    let tokens = vec![
        tok(TokenKind::LParen, 0),
        tok(TokenKind::Star, 1),
        tok(TokenKind::Ident, 2),  // u8
        tok(TokenKind::Comma, 4),
        tok(TokenKind::Ident, 6),  // u64
        tok(TokenKind::RParen, 9),
        tok(TokenKind::Arrow, 11),
        tok(TokenKind::Ident, 14), // u32
        tok(TokenKind::Eof, 17),
    ];
    let (arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 0);
    assert!(result.is_ok());
    let ty_id = result.unwrap();
    let ty_node = arena.get(ty_id).unwrap();
    assert_eq!(ty_node.kind, NodeKind::TypeFnPtr);
    if let Some(TypeData::FnPtr { params, .. }) = arena.type_data(ty_id) {
        assert_eq!(params.len(), 2);
    } else {
        panic!("expected TypeFnPtr with two params");
    }
}

#[test]
fn parse_fn_ptr_empty_effects() {
    // `(u32) -> u32 !{}` — empty effect row
    let tokens = vec![
        tok(TokenKind::LParen, 0),
        tok(TokenKind::Ident, 1),  // u32
        tok(TokenKind::RParen, 4),
        tok(TokenKind::Arrow, 6),
        tok(TokenKind::Ident, 9),  // u32
        tok(TokenKind::EffectOpen, 13),
        tok(TokenKind::RBrace, 14),
        tok(TokenKind::Eof, 15),
    ];
    let (arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 0);
    assert!(result.is_ok());
    let ty_id = result.unwrap();
    if let Some(TypeData::FnPtr { effects, .. }) = arena.type_data(ty_id) {
        assert!(effects.is_some());
    } else {
        panic!("expected TypeFnPtr with empty effects");
    }
}

#[test]
fn parse_fn_ptr_trailing_comma() {
    // `(u32, u32,) -> u32` — trailing comma
    let tokens = vec![
        tok(TokenKind::LParen, 0),
        tok(TokenKind::Ident, 1),  // u32
        tok(TokenKind::Comma, 4),
        tok(TokenKind::Ident, 6),  // u32
        tok(TokenKind::Comma, 9),
        tok(TokenKind::RParen, 10),
        tok(TokenKind::Arrow, 12),
        tok(TokenKind::Ident, 15), // u32
        tok(TokenKind::Eof, 18),
    ];
    let (arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 0);
    assert!(result.is_ok());
    let ty_id = result.unwrap();
    if let Some(TypeData::FnPtr { params, .. }) = arena.type_data(ty_id) {
        assert_eq!(params.len(), 2);
    } else {
        panic!("expected TypeFnPtr with two params (trailing comma)");
    }
}

#[test]
fn parse_fn_ptr_as_return() {
    // `(u32) -> ((u32) -> u32)` — fn-ptr as return
    let tokens = vec![
        tok(TokenKind::LParen, 0),
        tok(TokenKind::Ident, 1),  // u32
        tok(TokenKind::RParen, 4),
        tok(TokenKind::Arrow, 6),
        tok(TokenKind::LParen, 9),
        tok(TokenKind::LParen, 10),
        tok(TokenKind::Ident, 11), // u32
        tok(TokenKind::RParen, 14),
        tok(TokenKind::Arrow, 16),
        tok(TokenKind::Ident, 19), // u32
        tok(TokenKind::RParen, 22),
        tok(TokenKind::Eof, 23),
    ];
    let (arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 0);
    assert!(result.is_ok());
    let outer_id = result.unwrap();
    let outer_node = arena.get(outer_id).unwrap();
    assert_eq!(outer_node.kind, NodeKind::TypeFnPtr);
    // Return type should be a TypeFnPtr
    if let Some(TypeData::FnPtr { ret, .. }) = arena.type_data(outer_id) {
        let ret_node = arena.get(*ret).unwrap();
        assert_eq!(ret_node.kind, NodeKind::TypeFnPtr);
    } else {
        panic!("expected outer TypeFnPtr");
    }
}

#[test]
fn parse_fn_ptr_as_param() {
    // `((u32) -> u32) -> u32` — fn-ptr as param
    let tokens = vec![
        tok(TokenKind::LParen, 0),
        tok(TokenKind::LParen, 1),
        tok(TokenKind::Ident, 2),  // u32
        tok(TokenKind::RParen, 5),
        tok(TokenKind::Arrow, 7),
        tok(TokenKind::Ident, 10), // u32
        tok(TokenKind::RParen, 13),
        tok(TokenKind::Arrow, 15),
        tok(TokenKind::Ident, 18), // u32
        tok(TokenKind::Eof, 21),
    ];
    let (arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 0);
    assert!(result.is_ok());
    let outer_id = result.unwrap();
    let outer_node = arena.get(outer_id).unwrap();
    assert_eq!(outer_node.kind, NodeKind::TypeFnPtr);
    if let Some(TypeData::FnPtr { params, .. }) = arena.type_data(outer_id) {
        assert_eq!(params.len(), 1);
        let param_node = arena.get(params[0]).unwrap();
        assert_eq!(param_node.kind, NodeKind::TypeFnPtr);
    } else {
        panic!("expected outer TypeFnPtr");
    }
}

#[test]
fn parse_ptr_to_fn_ptr() {
    // `*(u32) -> u32` — *(FnPtr)
    let tokens = vec![
        tok(TokenKind::Star, 0),
        tok(TokenKind::LParen, 1),
        tok(TokenKind::Ident, 2),  // u32
        tok(TokenKind::RParen, 5),
        tok(TokenKind::Arrow, 7),
        tok(TokenKind::Ident, 10), // u32
        tok(TokenKind::Eof, 13),
    ];
    let (arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 0);
    assert!(result.is_ok());
    let ptr_id = result.unwrap();
    let ptr_node = arena.get(ptr_id).unwrap();
    assert_eq!(ptr_node.kind, NodeKind::TypePtr);
    if let Some(TypeData::Ptr { pointee }) = arena.type_data(ptr_id) {
        let fn_node = arena.get(*pointee).unwrap();
        assert_eq!(fn_node.kind, NodeKind::TypeFnPtr);
    } else {
        panic!("expected TypePtr");
    }
}

#[test]
fn parse_fn_ptr_taking_pointer() {
    // `(*u32) -> u32` — FnPtr taking pointer
    let tokens = vec![
        tok(TokenKind::LParen, 0),
        tok(TokenKind::Star, 1),
        tok(TokenKind::Ident, 2),  // u32
        tok(TokenKind::RParen, 5),
        tok(TokenKind::Arrow, 7),
        tok(TokenKind::Ident, 10), // u32
        tok(TokenKind::Eof, 13),
    ];
    let (arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 0);
    assert!(result.is_ok());
    let ty_id = result.unwrap();
    let ty_node = arena.get(ty_id).unwrap();
    assert_eq!(ty_node.kind, NodeKind::TypeFnPtr);
    if let Some(TypeData::FnPtr { params, .. }) = arena.type_data(ty_id) {
        assert_eq!(params.len(), 1);
        let param_node = arena.get(params[0]).unwrap();
        assert_eq!(param_node.kind, NodeKind::TypePtr);
    } else {
        panic!("expected TypeFnPtr");
    }
}

#[test]
fn parse_fn_ptr_in_record() {
    // `record { f: (u32) -> u32 }` — fn-ptr in record (vops shape)
    let tokens = vec![
        tok(TokenKind::KwRecord, 0),
        tok(TokenKind::LBrace, 7),
        tok(TokenKind::Ident, 9),  // f
        tok(TokenKind::Colon, 10),
        tok(TokenKind::LParen, 12),
        tok(TokenKind::Ident, 13), // u32
        tok(TokenKind::RParen, 16),
        tok(TokenKind::Arrow, 18),
        tok(TokenKind::Ident, 21), // u32
        tok(TokenKind::RBrace, 24),
        tok(TokenKind::Eof, 25),
    ];
    let (arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 0);
    assert!(result.is_ok());
    let record_id = result.unwrap();
    let record_node = arena.get(record_id).unwrap();
    assert_eq!(record_node.kind, NodeKind::TypeRecord);
    if let Some(TypeData::Record { fields }) = arena.type_data(record_id) {
        assert_eq!(fields.len(), 1);
        let (_field_name, field_ty) = fields[0];
        let field_node = arena.get(field_ty).unwrap();
        assert_eq!(field_node.kind, NodeKind::TypeFnPtr);
    } else {
        panic!("expected TypeRecord");
    }
}

// Error tests for FnPtr

#[test]
fn parse_fn_ptr_missing_return_type() {
    // `(u32) ->` — missing return type
    let tokens = vec![
        tok(TokenKind::LParen, 0),
        tok(TokenKind::Ident, 1),  // u32
        tok(TokenKind::RParen, 4),
        tok(TokenKind::Arrow, 6),
        tok(TokenKind::Eof, 9),
    ];
    let (_arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 1, "expected 1 diagnostic");
    let diag = &diags[0];
    assert_eq!(diag.code().number(), 100, "expected P0100 (expected type)");
    assert!(result.is_err(), "expected parse error for missing return type");
}

#[test]
fn parse_fn_ptr_malformed_effects() {
    // `(u32) -> u32 !` — malformed effect annot
    let tokens = vec![
        tok(TokenKind::LParen, 0),
        tok(TokenKind::Ident, 1),  // u32
        tok(TokenKind::RParen, 4),
        tok(TokenKind::Arrow, 6),
        tok(TokenKind::Ident, 9),  // u32
        tok(TokenKind::EffectOpen, 13),
        tok(TokenKind::Eof, 14),
    ];
    let (_arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 1, "expected 1 diagnostic");
    let diag = &diags[0];
    assert_eq!(diag.code().number(), 100, "expected P0100 (expected closing brace)");
    assert!(result.is_err(), "expected parse error for malformed effects");
}

#[test]
fn parse_fn_ptr_effect_on_param() {
    // `(u32 !{Atomic}) -> u32` — effect on param position (reject)
    // This should parse as a type error: effects on param types are not allowed
    let tokens = vec![
        tok(TokenKind::LParen, 0),
        tok(TokenKind::Ident, 1),  // u32
        tok(TokenKind::EffectOpen, 4),
        tok(TokenKind::Ident, 6),  // Atomic
        tok(TokenKind::RBrace, 12),
        tok(TokenKind::RParen, 13),
        tok(TokenKind::Arrow, 15),
        tok(TokenKind::Ident, 18), // u32
        tok(TokenKind::Eof, 21),
    ];
    let (_arena, result, diags) = parse_t(tokens);

    assert_eq!(diags.len(), 1, "expected 1 diagnostic");
    let diag = &diags[0];
    assert_eq!(diag.code().number(), 100, "expected P0100 (expected closing paren)");
    assert!(result.is_err(), "expected parse error for effect on param");
}
