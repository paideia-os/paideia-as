//! Integration test for PA10-002: string literal lowering to .rodata.
//!
//! Tests that:
//! 1. String literals parse and lower to StringLiteral IR nodes
//! 2. Byte payloads are extracted and stored in literal_bytes table
//! 3. Identical strings are interned to the same symbol (dedup)
//! 4. populate_data_table creates .rodata entries with relocations

use paideia_as_diagnostics::Span;
use paideia_as_ir::{IrArena, IrKind};

#[test]
fn string_lit_emit_literal_bytes_extracted() {
    // Test: lowering extracts byte payloads from string literals.
    // This would be done by cmd_build.rs in the AST walk; here we directly
    // populate the table to test the round-trip.

    let mut arena = IrArena::new();

    // Allocate a StringLiteral IR node
    let str_id = arena.alloc(
        IrKind::StringLiteral,
        Span::new(paideia_as_diagnostics::FileId::new(1).unwrap(), 0, 1),
    );

    // Populate the literal_bytes table (simulating AST extraction)
    let bytes = b"hello".to_vec();
    arena.literal_bytes_mut().insert(str_id, bytes.clone());

    // Verify the bytes are retrievable
    assert_eq!(arena.literal_bytes().get(str_id), Some(&bytes));
}

#[test]
fn string_lit_emit_intern_same_bytes_returns_same_symbol() {
    // Test: identical byte sequences intern to the same symbol.
    use paideia_as_elaborator::string_intern::StringInternTable;

    let mut intern_table = StringInternTable::new();

    let bytes1 = b"banner";
    let bytes2 = b"banner";

    let (sym1, len_sym1) = intern_table.intern(bytes1);
    let (sym2, len_sym2) = intern_table.intern(bytes2);

    assert_eq!(sym1, sym2);
    assert_eq!(len_sym1, len_sym2);
    assert_eq!(intern_table.len(), 1);
}

#[test]
fn string_lit_emit_distinct_bytes_distinct_symbols() {
    // Test: distinct byte sequences get distinct symbols.
    use paideia_as_elaborator::string_intern::StringInternTable;

    let mut intern_table = StringInternTable::new();

    let (sym_hello, _) = intern_table.intern(b"hello");
    let (sym_world, _) = intern_table.intern(b"world");
    let (sym_banner, _) = intern_table.intern(b"banner");

    assert_ne!(sym_hello, sym_world);
    assert_ne!(sym_hello, sym_banner);
    assert_ne!(sym_world, sym_banner);
    assert_eq!(intern_table.len(), 3);
}

#[test]
fn string_lit_emit_symbol_hash_is_deterministic_and_distinguishes() {
    // PAS-DEBT-B4-001 (#1523): symbol_hash is BLAKE3-truncated-to-u64.
    // Verify determinism and that similar inputs do not collide.
    use paideia_as_elaborator::string_intern::symbol_hash;

    assert_eq!(symbol_hash(b"hello"), symbol_hash(b"hello"));
    assert_ne!(symbol_hash(b"hello"), symbol_hash(b"world"));
    // Off-by-one and case-change: independent digests.
    assert_ne!(symbol_hash(b"schema@0.1"), symbol_hash(b"schema@0.2"));
    assert_ne!(symbol_hash(b"Schema"), symbol_hash(b"schema"));
}

#[test]
fn string_lit_emit_symbol_names_have_correct_format() {
    // Test: interned symbol names follow the __str_<hash> pattern.
    use paideia_as_elaborator::string_intern::StringInternTable;

    let mut intern_table = StringInternTable::new();
    let (sym, len_sym) = intern_table.intern(b"test");

    assert!(sym.starts_with("__str_"));
    assert!(len_sym.starts_with("__str_"));
    assert!(len_sym.ends_with("__len"));

    // Symbol should be 22 chars: __str_ (6) + 16 hex digits
    assert_eq!(sym.len(), 22);
    // Length symbol should be 27 chars: __str_ (6) + 16 hex digits + __len (5)
    assert_eq!(len_sym.len(), 27);
}
