//! paideia-as#1373 (v0.28-M1-004, Wave 0 Batch 2): `@packed_struct`.
//!
//! Snapshot + diagnostic coverage for the struct-level `@packed_struct`
//! parser primitive:
//! - `Ok` cases: bare `@packed_struct` and `@packed_struct(align=<n>)`
//!   record a `StructAttr::Packed { align }` entry on the arena's
//!   `struct_attr` side-table, keyed by the resulting struct-decl's
//!   NodeId.
//! - Composition with b2-05: `@packed_struct` on a struct whose fields
//!   carry `@endian(be)` records BOTH side-tables in the same parse,
//!   independently.
//! - `P0292`: malformed `@packed_struct(...)` argument list (wrong
//!   attribute name, missing `=`, missing `)`).
//! - `P0293`: `align` value zero or not a power of two.
//! - `P0295`: `@packed_struct` attached to a non-`struct` decl (`enum`,
//!   `let`, …).
//!
//! Follows the parse-and-check shape used by `endian_attr_snapshots.rs`
//! and its siblings — one end-to-end pass through lexer + parser so the
//! snapshot pins observable behaviour rather than an internal parser
//! fragment.

use paideia_as_ast::{AstArena, Endianness, FieldAttr, ItemData, NodeId, NodeKind, StructAttr};
use paideia_as_diagnostics::{DiagnosticSink, VecSink};
use paideia_as_lexer::{Lexer, SourceText, TokenKind};
use paideia_as_parser::{Parser, parse_packed_struct};
use std::path::PathBuf;

/// End-to-end parse that invokes `parse_packed_struct` at the current
/// token position. Returns the arena, the parse result, and every
/// diagnostic emitted along the way. The top-level `parse_source_file`
/// entry does not yet dispatch on `@packed_struct` (that wiring is a
/// separate primitive), so the test drives the entry point directly.
fn parse_packed_from(
    source: &str,
) -> (
    AstArena,
    Result<NodeId, paideia_as_parser::ParseError>,
    Vec<paideia_as_diagnostics::Diagnostic>,
) {
    let mut source_map = paideia_as_diagnostics::SourceMap::new();
    let file = source_map.add_file(PathBuf::from("test.pdx"), source.to_string());
    let source_text = SourceText::from_bytes(file, source.as_bytes()).expect("valid utf-8");
    let mut arena = AstArena::new();
    let mut sink = VecSink::new();
    let mut lex = Lexer::new(file, &source_text);
    let mut collector = VecSink::new();
    let tokens = lex.collect_tokens(&mut collector);
    for d in collector.into_diagnostics() {
        let _ = sink.emit(d);
    }
    let result = {
        let mut p = Parser::new(&tokens, source_text.content(), file, &mut arena, &mut sink);
        parse_packed_struct(&mut p)
    };
    (arena, result, sink.into_diagnostics())
}

/// Assert that at least one diagnostic with `P<number>` is present.
fn assert_has_p_code(diags: &[paideia_as_diagnostics::Diagnostic], number: u16) {
    let matches: Vec<_> = diags
        .iter()
        .filter(|d| d.code().category().letter() == 'P' && d.code().number() == number)
        .collect();
    assert!(
        !matches.is_empty(),
        "expected P{:04}, got: {:?}",
        number,
        diags
            .iter()
            .map(|d| format!("{}{:04}", d.code().category().letter(), d.code().number()))
            .collect::<Vec<_>>()
    );
}

/// Assert that no diagnostic with `P<number>` is present.
fn assert_no_p_code(diags: &[paideia_as_diagnostics::Diagnostic], number: u16) {
    assert!(
        !diags
            .iter()
            .any(|d| d.code().category().letter() == 'P' && d.code().number() == number),
        "unexpected P{:04} in diagnostics: {:?}",
        number,
        diags
            .iter()
            .map(|d| format!("{}{:04}", d.code().category().letter(), d.code().number()))
            .collect::<Vec<_>>()
    );
}

/// Locate the first `Struct` item in the arena and return its
/// field-name NodeIds in source order.
fn field_ids_of_first_struct(arena: &AstArena) -> Vec<NodeId> {
    for raw in 1u32..u32::MAX {
        let id = match NodeId::new(raw) {
            Some(id) => id,
            None => break,
        };
        let node = match arena.get(id) {
            Some(n) => n,
            None => break,
        };
        if node.kind == NodeKind::Struct {
            if let Some(ItemData::Struct { fields, .. }) = arena.item_data(id) {
                return fields.iter().map(|(name, _)| *name).collect();
            }
        }
    }
    Vec::new()
}

// ---------------------------------------------------------------------
// Ok cases — bare and with-align
// ---------------------------------------------------------------------

#[test]
fn snapshot_bare_packed_struct_records_align_none() {
    // `@packed_struct struct S { x: u32, y: u16 }` records
    // `StructAttr::Packed { align: None }` (i.e. align = 1) on the
    // resulting struct node, and leaves the two fields intact.
    let source = "@packed_struct struct S { x: u32, y: u16 }";
    let (arena, result, diags) = parse_packed_from(source);
    let struct_id = result.expect("parse succeeded");
    // None of the P0292 / P0293 / P0295 codes may fire on a clean parse.
    for n in [292u16, 293, 295] {
        assert_no_p_code(&diags, n);
    }

    let attrs = arena
        .struct_attr()
        .get(struct_id)
        .expect("packed struct entry recorded");
    assert_eq!(attrs, &[StructAttr::Packed { align: None }][..]);

    // Struct body itself parses two fields.
    let fields = field_ids_of_first_struct(&arena);
    assert_eq!(fields.len(), 2);

    let snapshot = format!("struct.attrs = {:?}\nfields.len = {}", attrs, fields.len());
    insta::assert_snapshot!("bare_packed_struct_records_align_none", snapshot);
}

#[test]
fn snapshot_packed_struct_with_align_records_value() {
    // `@packed_struct(align=4) struct S { x: u32 }` records
    // `StructAttr::Packed { align: Some(4) }`.
    let source = "@packed_struct(align=4) struct S { x: u32 }";
    let (arena, result, diags) = parse_packed_from(source);
    let struct_id = result.expect("parse succeeded");
    for n in [292u16, 293, 295] {
        assert_no_p_code(&diags, n);
    }

    let attrs = arena
        .struct_attr()
        .get(struct_id)
        .expect("packed struct entry recorded");
    assert_eq!(attrs, &[StructAttr::Packed { align: Some(4) }][..]);

    let snapshot = format!("struct.attrs = {:?}", attrs);
    insta::assert_snapshot!("packed_struct_with_align_records_value", snapshot);
}

#[test]
fn snapshot_packed_struct_align_16_records_value() {
    // A larger power-of-two boundary — 16 — is accepted.
    let source = "@packed_struct(align=16) struct Cacheline { a: u64, b: u64 }";
    let (arena, result, _diags) = parse_packed_from(source);
    let struct_id = result.expect("parse succeeded");
    let attrs = arena.struct_attr().get(struct_id).expect("entry present");
    assert_eq!(attrs, &[StructAttr::Packed { align: Some(16) }][..]);
    let snapshot = format!("struct.attrs = {:?}", attrs);
    insta::assert_snapshot!("packed_struct_align_16_records_value", snapshot);
}

// ---------------------------------------------------------------------
// Composition with b2-05 (`@endian`)
// ---------------------------------------------------------------------

#[test]
fn snapshot_packed_struct_composes_with_endian_fields() {
    // Coordination with b2-05: `@packed_struct` at struct scope must
    // compose with `@endian(be)` on individual fields — packing fixes
    // offsets, endian fixes byte order. Both side-tables must land
    // populated after the same parse.
    let source = "\
@packed_struct(align=1) struct Header {
    @endian(be) magic: u32,
    length: u16,
}
";
    let (arena, result, diags) = parse_packed_from(source);
    let struct_id = result.expect("parse succeeded");
    for n in [292u16, 293, 295, 301, 302, 303, 304] {
        assert_no_p_code(&diags, n);
    }

    // Struct-level side-table.
    let struct_attrs = arena.struct_attr().get(struct_id).expect("entry present");
    assert_eq!(struct_attrs, &[StructAttr::Packed { align: Some(1) }][..]);

    // Field-level side-table.
    let fields = field_ids_of_first_struct(&arena);
    assert_eq!(fields.len(), 2);
    let magic_attrs = arena
        .struct_field_attrs()
        .get(fields[0])
        .expect("magic should carry @endian(be)");
    assert_eq!(magic_attrs, &[FieldAttr::Endian(Endianness::Be)][..]);
    // The unannotated `length` field must NOT show up in the side-table.
    assert!(arena.struct_field_attrs().get(fields[1]).is_none());

    let snapshot = format!(
        "struct.attrs = {:?}\nfield0.attrs = {:?}\nfield1.attrs = {:?}",
        struct_attrs,
        magic_attrs,
        arena.struct_field_attrs().get(fields[1])
    );
    insta::assert_snapshot!("packed_struct_composes_with_endian_fields", snapshot);
}

// ---------------------------------------------------------------------
// Diagnostic paths — malformed align argument
// ---------------------------------------------------------------------

#[test]
fn packed_struct_wrong_arg_key_emits_p0292() {
    // Argument key must be exactly `align`, not e.g. `size`.
    let source = "@packed_struct(size=4) struct S { x: u32 }";
    let (_arena, result, diags) = parse_packed_from(source);
    assert!(result.is_err(), "malformed key should fail parse");
    assert_has_p_code(&diags, 292);
}

#[test]
fn packed_struct_missing_equals_emits_p0292() {
    // `align 4` — no `=`.
    let source = "@packed_struct(align 4) struct S { x: u32 }";
    let (_arena, result, diags) = parse_packed_from(source);
    assert!(result.is_err());
    assert_has_p_code(&diags, 292);
}

#[test]
fn packed_struct_missing_close_paren_emits_p0292() {
    // Missing `)` — the parser is expected to see `struct` where it
    // wants `)` and fail there.
    let source = "@packed_struct(align=4 struct S { x: u32 }";
    let (_arena, result, diags) = parse_packed_from(source);
    assert!(result.is_err());
    // Depending on where the parser bails, it may emit either P0292
    // (missing `)`) or a downstream code. We assert on the P0292
    // path — the argument-list diagnostic — since that is the one the
    // primitive owns.
    assert_has_p_code(&diags, 292);
}

// ---------------------------------------------------------------------
// Diagnostic paths — align value validation
// ---------------------------------------------------------------------

#[test]
fn packed_struct_align_zero_emits_p0293() {
    let source = "@packed_struct(align=0) struct S { x: u32 }";
    let (_arena, result, diags) = parse_packed_from(source);
    assert!(result.is_err(), "align=0 should fail parse");
    assert_has_p_code(&diags, 293);
}

#[test]
fn packed_struct_align_non_power_of_two_emits_p0293() {
    let source = "@packed_struct(align=3) struct S { x: u32 }";
    let (_arena, result, diags) = parse_packed_from(source);
    assert!(result.is_err(), "align=3 should fail parse");
    assert_has_p_code(&diags, 293);
}

#[test]
fn packed_struct_align_six_emits_p0293() {
    // 6 is even but not a power of two.
    let source = "@packed_struct(align=6) struct S { x: u32 }";
    let (_arena, result, diags) = parse_packed_from(source);
    assert!(result.is_err());
    assert_has_p_code(&diags, 293);
}

#[test]
fn packed_struct_align_128_ok() {
    // 128 = 2^7, still a valid power of two — regression check for the
    // upper end of the byte-boundary range that OS runtimes commonly use.
    let source = "@packed_struct(align=128) struct C { a: u64, b: u64 }";
    let (arena, result, diags) = parse_packed_from(source);
    let struct_id = result.expect("align=128 should parse cleanly");
    assert_no_p_code(&diags, 293);
    let attrs = arena.struct_attr().get(struct_id).expect("entry present");
    assert_eq!(attrs, &[StructAttr::Packed { align: Some(128) }][..]);
}

// ---------------------------------------------------------------------
// Diagnostic paths — non-struct decls
// ---------------------------------------------------------------------

#[test]
fn packed_struct_on_enum_emits_p0295() {
    // Reject on non-struct: `enum` after `@packed_struct` triggers P0295.
    let source = "@packed_struct enum Color { Red, Green }";
    let (_arena, result, diags) = parse_packed_from(source);
    assert!(result.is_err(), "packed on enum should fail parse");
    assert_has_p_code(&diags, 295);
}

#[test]
fn packed_struct_on_let_emits_p0295() {
    let source = "@packed_struct let x = 1";
    let (_arena, result, diags) = parse_packed_from(source);
    assert!(result.is_err());
    assert_has_p_code(&diags, 295);
}

#[test]
fn packed_struct_on_trait_emits_p0295() {
    let source = "@packed_struct trait Foo { }";
    let (_arena, result, diags) = parse_packed_from(source);
    assert!(result.is_err());
    assert_has_p_code(&diags, 295);
}

// ---------------------------------------------------------------------
// Diagnostic paths — wrong attribute name at direct entry
// ---------------------------------------------------------------------

#[test]
fn packed_struct_wrong_attr_name_emits_p0292() {
    // A caller who dispatches `parse_packed_struct` on the wrong `@name`
    // gets P0292 rather than a silent misparse.
    let source = "@packed struct S { x: u32 }";
    let (_arena, result, diags) = parse_packed_from(source);
    assert!(result.is_err(), "wrong attr name should fail parse");
    assert_has_p_code(&diags, 292);
}

// ---------------------------------------------------------------------
// Sanity: KwStruct still tokenises the same way (guards the P0295 arm)
// ---------------------------------------------------------------------

#[test]
fn kw_struct_still_lexes_as_kwstruct() {
    // Cheap sanity check that the lexer still treats `struct` as
    // KwStruct — the P0295 arm's "found …" clause depends on this.
    let source = "struct";
    let mut source_map = paideia_as_diagnostics::SourceMap::new();
    let file = source_map.add_file(PathBuf::from("t.pdx"), source.to_string());
    let source_text = SourceText::from_bytes(file, source.as_bytes()).expect("utf-8");
    let mut sink = VecSink::new();
    let mut lex = Lexer::new(file, &source_text);
    let tokens = lex.collect_tokens(&mut sink);
    assert_eq!(tokens[0].kind, TokenKind::KwStruct);
}
