//! paideia-as#1372 (v0.28-M1-003): `@endian(be|le)` field attribute.
//!
//! Snapshot + diagnostic coverage for the per-field `@endian(...)` parser:
//! - `Ok` cases: `@endian(be)` / `@endian(le)` on integral scalar fields
//!   record on `AstArena::struct_field_attrs` under the field-name id.
//! - `P0301`: the attribute rejects non-integral field types (record,
//!   array, tuple, pointer, `bool`, `f64`).
//! - `P0302`: unknown attribute names in field-prefix position.
//! - `P0303` / `P0304`: malformed `@endian(...)` syntax.
//!
//! Follows the parse-and-check test shape used by
//! `align_attr_errors.rs` and its siblings — a single end-to-end pass
//! through lexer + parser so the snapshot pins the observable behaviour
//! rather than an internal parser fragment.

use paideia_as_ast::{AstArena, FieldAttr, Endianness, ItemData, NodeId};
use paideia_as_diagnostics::{DiagnosticSink, VecSink};
use paideia_as_lexer::{Lexer, SourceText};
use paideia_as_parser::Parser;
use std::path::PathBuf;

/// End-to-end parse: lex `source`, run the parser, hand back the arena
/// (so tests can inspect the struct-field-attribute side-table), the
/// parse result, and every diagnostic emitted along the way.
fn parse_and_check(
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
    // Forward lexer diagnostics into the main sink so the caller sees
    // one merged stream.
    for d in collector.into_diagnostics() {
        let _ = sink.emit(d);
    }
    let result = {
        let mut p = Parser::new(&tokens, source_text.content(), file, &mut arena, &mut sink);
        p.parse_source_file()
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

/// Locate the first `Struct` item in the arena and return its field-name
/// NodeIds in source order.
fn field_ids_of_first_struct(arena: &AstArena) -> Vec<NodeId> {
    // Node ids are minted contiguously from 1; scan the arena for the
    // first Struct item.
    for raw in 1u32..u32::MAX {
        let id = match NodeId::new(raw) {
            Some(id) => id,
            None => break,
        };
        let node = match arena.get(id) {
            Some(n) => n,
            None => break,
        };
        if node.kind == paideia_as_ast::NodeKind::Struct {
            if let Some(ItemData::Struct { fields, .. }) = arena.item_data(id) {
                return fields.iter().map(|(name, _)| *name).collect();
            }
        }
    }
    Vec::new()
}

#[test]
fn snapshot_endian_be_on_u32_field_records_attr() {
    let source = "\
struct Header {
    @endian(be) magic: u32,
    length: u16,
}
";
    let (arena, result, diags) = parse_and_check(source);
    assert!(result.is_ok(), "parse should succeed; diags: {:?}", diags);
    // No P0301-range diagnostics on a well-formed u32 field.
    for n in 301u16..=304 {
        assert_no_p_code(&diags, n);
    }

    let fields = field_ids_of_first_struct(&arena);
    assert_eq!(fields.len(), 2, "expected two fields, got {}", fields.len());

    // Field 0 (`magic`) has @endian(be); field 1 (`length`) has nothing.
    let magic_attrs = arena
        .struct_field_attrs()
        .get(fields[0])
        .expect("magic should have an attribute");
    let length_attrs = arena.struct_field_attrs().get(fields[1]);

    let snapshot = format!(
        "field0.attrs = {:?}\nfield1.attrs = {:?}",
        magic_attrs, length_attrs
    );
    insta::assert_snapshot!("endian_be_on_u32_field_records_attr", snapshot);
}

#[test]
fn snapshot_endian_le_on_i64_field_records_attr() {
    let source = "\
struct Wire {
    @endian(le) checksum: i64,
}
";
    let (arena, result, diags) = parse_and_check(source);
    assert!(result.is_ok(), "parse should succeed; diags: {:?}", diags);

    let fields = field_ids_of_first_struct(&arena);
    assert_eq!(fields.len(), 1);
    let checksum_attrs = arena.struct_field_attrs().get(fields[0]).expect("le attr");
    assert_eq!(checksum_attrs, &[FieldAttr::Endian(Endianness::Le)][..]);

    let snapshot = format!("field0.attrs = {:?}", checksum_attrs);
    insta::assert_snapshot!("endian_le_on_i64_field_records_attr", snapshot);
}

#[test]
fn endian_on_array_field_emits_p0301() {
    // Arrays are not integral scalars.
    let source = "\
struct Bad {
    @endian(be) buf: [u8; 4],
}
";
    let (arena, _result, diags) = parse_and_check(source);
    assert_has_p_code(&diags, 301);
    // Rejection MUST NOT record the attribute on the side-table — the
    // elaborator would then see a byte-swap request on a non-scalar type.
    let fields = field_ids_of_first_struct(&arena);
    assert!(!fields.is_empty(), "struct still parses; field list present");
    assert!(
        arena.struct_field_attrs().get(fields[0]).is_none(),
        "rejected @endian must not land on the side-table"
    );
}

#[test]
fn endian_on_bool_field_emits_p0301() {
    let source = "\
struct Bad {
    @endian(be) flag: bool,
}
";
    let (_arena, _result, diags) = parse_and_check(source);
    assert_has_p_code(&diags, 301);
}

#[test]
fn endian_on_f64_field_emits_p0301() {
    // Floating-point is a scalar but not integral — reject.
    let source = "\
struct Bad {
    @endian(be) value: f64,
}
";
    let (_arena, _result, diags) = parse_and_check(source);
    assert_has_p_code(&diags, 301);
}

#[test]
fn unknown_field_attr_name_emits_p0302() {
    // `@foo(be)` is not a recognised per-field attribute.
    let source = "\
struct Bad {
    @foo(be) magic: u32,
}
";
    let (_arena, _result, diags) = parse_and_check(source);
    assert_has_p_code(&diags, 302);
}

#[test]
fn malformed_endian_missing_paren_emits_p0303() {
    // `@endian be` — no `(`.
    let source = "\
struct Bad {
    @endian be magic: u32,
}
";
    let (_arena, _result, diags) = parse_and_check(source);
    assert_has_p_code(&diags, 303);
}

#[test]
fn endian_unknown_byte_order_emits_p0304() {
    // `@endian(mid)` — not `be` or `le`.
    let source = "\
struct Bad {
    @endian(mid) magic: u32,
}
";
    let (_arena, _result, diags) = parse_and_check(source);
    assert_has_p_code(&diags, 304);
}

#[test]
fn snapshot_multiple_endian_fields_all_record() {
    // Both fields carry attributes; verify both survive to the side-table
    // and the unannotated middle field is absent.
    let source = "\
struct Frame {
    @endian(be) magic: u32,
    version: u16,
    @endian(le) payload_len: u64,
}
";
    let (arena, result, diags) = parse_and_check(source);
    assert!(result.is_ok(), "parse should succeed; diags: {:?}", diags);

    let fields = field_ids_of_first_struct(&arena);
    assert_eq!(fields.len(), 3);
    let a0 = arena.struct_field_attrs().get(fields[0]).map(<[FieldAttr]>::to_vec);
    let a1 = arena.struct_field_attrs().get(fields[1]).map(<[FieldAttr]>::to_vec);
    let a2 = arena.struct_field_attrs().get(fields[2]).map(<[FieldAttr]>::to_vec);
    let snapshot = format!(
        "field0.attrs = {:?}\nfield1.attrs = {:?}\nfield2.attrs = {:?}",
        a0, a1, a2
    );
    insta::assert_snapshot!("multiple_endian_fields_all_record", snapshot);
}
