//! Tests for @jump_table with wildcard (`_`) default arms.
//!
//! Issue #1055: The parser emits `NodeKind::PatIdent` for `_` patterns instead
//! of `NodeKind::PatWildcard`. This test verifies that `is_wildcard_pattern`
//! correctly identifies both real wildcards and PatIdent patterns with text "_",
//! so that @jump_table matches with `_` defaults pass validation and don't
//! trigger T0550 (non-integer pattern) errors.

use paideia_as_ast::AstArena;
use paideia_as_diagnostics::{SourceMap, VecSink};
use paideia_as_elaborator::lower_ast_to_ir;
use paideia_as_lexer::{Lexer, SourceText};
use paideia_as_parser::Parser;
use std::fs;
use std::path::{Path, PathBuf};

/// Compute the repo root from CARGO_MANIFEST_DIR.
fn get_repo_root() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let mut path = PathBuf::from(manifest_dir);
    // Go up: elaborator/.. (crates), crates/.. (repo root)
    path.pop();
    path.pop();
    path
}

/// Helper to load and parse a .pdx file into an AST.
fn load_and_parse_fixture(fixture_path: &Path) -> (SourceMap, AstArena) {
    let bytes = fs::read(fixture_path).expect(&format!("Failed to read fixture: {}", fixture_path.display()));

    let mut source_map = SourceMap::new();
    let content_string = String::from_utf8_lossy(&bytes).into_owned();
    let file_id = source_map.add_file(fixture_path.to_path_buf(), content_string);

    let source = SourceText::from_bytes(file_id, &bytes).expect("Failed to create SourceText");

    // Lex.
    let mut lex_sink = VecSink::new();
    let mut lexer = Lexer::new(file_id, &source);
    let tokens = lexer.collect_tokens(&mut lex_sink);
    // Note: we ignore lex_sink errors for this test; we focus on lowering/jump_table validation.

    // Parse.
    let mut arena = AstArena::new();
    {
        let mut parser_sink = VecSink::new();
        let mut p = Parser::new(
            &tokens,
            source.content(),
            file_id,
            &mut arena,
            &mut parser_sink,
        );
        let _ = p.parse_source_file();
        // Note: we ignore parser_sink errors for this test.
    }

    (source_map, arena)
}

/// Test the dense fixture: should pass density check and have no T0550 errors.
#[test]
fn jump_table_dense_fixture_wildcard_ok() {
    let repo_root = get_repo_root();
    let fixture_path = repo_root.join("tests/end-to-end/codes/pa_r15_009_jump_table_dense.pdx");

    let (source_map, ast) = load_and_parse_fixture(&fixture_path);

    // Lower to IR and collect diagnostics.
    let mut sink = VecSink::new();
    let result = lower_ast_to_ir(&ast, &source_map, &mut sink);

    // Check that no T0550 errors were emitted.
    let diagnostics = sink.into_diagnostics();
    let t0550_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code().number() == 550) // T0550
        .collect();

    assert!(
        t0550_errors.is_empty(),
        "Dense fixture should not emit T0550 errors, but got: {:?}",
        t0550_errors
            .iter()
            .map(|d| d.message())
            .collect::<Vec<_>>()
    );

    // Verify the IR was produced (not empty).
    assert!(
        result.ir.len() > 0,
        "Dense fixture should produce IR nodes"
    );

    // Locate the first Match node in the IR.
    let mut match_node_id = None;
    for i in 1..=result.ir.len() {
        let ir_id = paideia_as_ir::IrNodeId::new(i as u32).expect("valid id");
        if let Some(node) = result.ir.get(ir_id) {
            if node.kind == paideia_as_ir::IrKind::Match {
                match_node_id = Some(ir_id);
                break;
            }
        }
    }

    let match_id = match_node_id.expect("Dense fixture should have at least one Match IR node");

    // Assert MatchDispatchMeta fields.
    let meta_table = result.ir.match_dispatch_meta();
    let meta = meta_table
        .get(match_id)
        .expect("Match node should have MatchDispatchMeta");

    assert_eq!(
        meta.jump_table, true,
        "Dense fixture should have jump_table = true"
    );
    assert_eq!(
        meta.density_ok, true,
        "Dense fixture should pass density check (density_ok = true). \
         Actual: covered_arms={}, range={}, density formula: {} * 2 >= {}",
        meta.covered_arms, meta.range, meta.covered_arms, meta.range
    );
    assert!(
        meta.covered_arms >= 3,
        "Dense fixture should cover at least 3 integer arms, got {}",
        meta.covered_arms
    );
    assert!(
        meta.range > 0,
        "Dense fixture should have positive range, got {}",
        meta.range
    );
}

/// Test the sparse fixture: should fail density check but not emit T0550.
///
/// The sparse fixture has 3 integer patterns out of a range of 17 values (3/17 ≈ 17%),
/// which is well below the 50% threshold. However, it should still be valid for
/// parsing and lowering, and the `_` default arm should be recognized as a wildcard.
#[test]
fn jump_table_sparse_fixture_wildcard_ok() {
    let repo_root = get_repo_root();
    let fixture_path = repo_root.join("tests/end-to-end/codes/pa_r15_009_jump_table_sparse.pdx");

    let (source_map, ast) = load_and_parse_fixture(&fixture_path);

    // Lower to IR and collect diagnostics.
    let mut sink = VecSink::new();
    let result = lower_ast_to_ir(&ast, &source_map, &mut sink);

    // Check that no T0550 errors were emitted (even though density is low).
    // T0550 should only be emitted for non-integer, non-wildcard patterns.
    // The `_` wildcard should pass the check.
    let diagnostics = sink.into_diagnostics();
    let t0550_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code().number() == 550) // T0550
        .collect();

    assert!(
        t0550_errors.is_empty(),
        "Sparse fixture should not emit T0550 for wildcard, but got: {:?}",
        t0550_errors
            .iter()
            .map(|d| d.message())
            .collect::<Vec<_>>()
    );

    // Verify the IR was produced.
    assert!(
        result.ir.len() > 0,
        "Sparse fixture should produce IR nodes"
    );

    // Locate the first Match node in the IR.
    let mut match_node_id = None;
    for i in 1..=result.ir.len() {
        let ir_id = paideia_as_ir::IrNodeId::new(i as u32).expect("valid id");
        if let Some(node) = result.ir.get(ir_id) {
            if node.kind == paideia_as_ir::IrKind::Match {
                match_node_id = Some(ir_id);
                break;
            }
        }
    }

    let match_id = match_node_id.expect("Sparse fixture should have at least one Match IR node");

    // Assert MatchDispatchMeta fields.
    let meta_table = result.ir.match_dispatch_meta();
    let meta = meta_table
        .get(match_id)
        .expect("Match node should have MatchDispatchMeta");

    assert_eq!(
        meta.jump_table, true,
        "Sparse fixture should have jump_table = true"
    );
    assert_eq!(
        meta.density_ok, false,
        "Sparse fixture should fail density check (density_ok = false). \
         Actual: covered_arms={}, range={}, density formula: {} * 2 >= {} is {}",
        meta.covered_arms, meta.range, meta.covered_arms, meta.range,
        meta.covered_arms * 2 >= meta.range as u32 * 2
    );
    assert!(
        meta.covered_arms > 0,
        "Sparse fixture should have at least one covered arm, got {}",
        meta.covered_arms
    );
}

/// Test a non-integer pattern fixture to verify T0550 is still emitted for real errors.
///
/// This fixture (if it exists) should have a pattern like `y` (variable) which is not
/// an integer and not a wildcard, so it should trigger T0550.
#[test]
fn jump_table_non_int_fixture_triggers_t0550() {
    let repo_root = get_repo_root();
    let fixture_path = repo_root.join("tests/end-to-end/codes/pa_r15_009_jump_table_non_int.pdx");

    if !fixture_path.exists() {
        // If the fixture doesn't exist, skip this test.
        return;
    }

    let (source_map, ast) = load_and_parse_fixture(&fixture_path);

    // Lower to IR and collect diagnostics.
    let mut sink = VecSink::new();
    let result = lower_ast_to_ir(&ast, &source_map, &mut sink);

    // Check that T0550 errors WERE emitted (for the non-integer pattern).
    let diagnostics = sink.into_diagnostics();
    let t0550_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code().number() == 550) // T0550
        .collect();

    assert!(
        !t0550_errors.is_empty(),
        "Non-integer fixture should emit T0550 errors"
    );

    // Locate the first Match node in the IR.
    let mut match_node_id = None;
    for i in 1..=result.ir.len() {
        let ir_id = paideia_as_ir::IrNodeId::new(i as u32).expect("valid id");
        if let Some(node) = result.ir.get(ir_id) {
            if node.kind == paideia_as_ir::IrKind::Match {
                match_node_id = Some(ir_id);
                break;
            }
        }
    }

    let match_id = match_node_id.expect("Non-int fixture should have at least one Match IR node");

    // Assert MatchDispatchMeta fields.
    let meta_table = result.ir.match_dispatch_meta();
    let meta = meta_table
        .get(match_id)
        .expect("Match node should have MatchDispatchMeta");

    assert_eq!(
        meta.jump_table, true,
        "Non-int fixture should have jump_table = true (attribute is present)"
    );
    assert_eq!(
        meta.density_ok, false,
        "Non-int fixture should have density_ok = false (non-integer patterns force fallback)"
    );
}

/// Test the full parse → lower → populate flow for the dense fixture.
///
/// This test exercises the shipped code path (populate_jump_tables_from_mutable_arena)
/// used in cmd_build.rs. It verifies:
/// 1. Parse and lower produce an IR with MatchDispatchMeta
/// 2. populate_jump_tables_from_mutable_arena correctly creates rodata entries
/// 3. The entries appear in arena.data() with correct symbol names and relocations
#[test]
fn jump_table_dense_populate_creates_rodata_entries() {
    use paideia_as_elaborator::data_encoder;
    use paideia_as_ir::{IrKind, SectionKind};

    let repo_root = get_repo_root();
    let fixture_path = repo_root.join("tests/end-to-end/codes/pa_r15_009_jump_table_dense.pdx");

    let (source_map, ast) = load_and_parse_fixture(&fixture_path);

    // Lower to IR and collect diagnostics.
    let mut sink = VecSink::new();
    let mut result = lower_ast_to_ir(&ast, &source_map, &mut sink);

    // Locate the first Match node in the IR before population.
    let mut match_node_id = None;
    for i in 1..=result.ir.len() {
        let ir_id = paideia_as_ir::IrNodeId::new(i as u32).expect("valid id");
        if let Some(node) = result.ir.get(ir_id) {
            if node.kind == IrKind::Match {
                match_node_id = Some(ir_id);
                break;
            }
        }
    }

    let match_id = match_node_id.expect("Dense fixture should have at least one Match IR node");

    // Verify MatchDispatchMeta exists and is valid.
    // Copy the metadata before calling the mutable function to avoid borrow checker issues.
    let meta = {
        let m = result.ir
            .match_dispatch_meta()
            .get(match_id)
            .expect("Match node should have MatchDispatchMeta");
        *m  // Copy the metadata
    };

    assert_eq!(meta.jump_table, true, "Dense fixture should have jump_table = true");
    assert_eq!(meta.density_ok, true, "Dense fixture should pass density check");

    // Call populate_jump_tables_from_mutable_arena (the shipped variant).
    data_encoder::populate_jump_tables_from_mutable_arena(&mut result.ir);

    // Verify the entry was created in arena.data().
    let entry = result.ir.data()
        .get(match_id)
        .expect("Jump table entry should exist in arena.data() after populate");

    // Verify section kind, symbol name, and alignment.
    assert_eq!(entry.section, SectionKind::Rodata, "Jump table should be rodata");
    assert_eq!(entry.symbol_name, format!("_jt_{}", match_id.get()), "Symbol name should be _jt_<id>");
    assert_eq!(entry.align, 8, "Alignment should be 8");

    // Verify bytes and relocations.
    let expected_bytes_len = (meta.range as usize) * 8;
    assert_eq!(entry.bytes.len(), expected_bytes_len,
        "Bytes length should match range: expected {}, got {}", expected_bytes_len, entry.bytes.len());

    let expected_reloc_count = meta.range as usize;
    assert_eq!(entry.relocations.len(), expected_reloc_count,
        "Should have {} relocations (one per slot in range), got {}",
        expected_reloc_count, entry.relocations.len());

    // Verify all relocations have valid offsets and symbol names.
    for (i, reloc) in entry.relocations.iter().enumerate() {
        let expected_offset = (i * 8) as u64;
        assert_eq!(reloc.offset, expected_offset,
            "Relocation {} should have offset {}, got {}", i, expected_offset, reloc.offset);

        // Symbol should reference either an arm label or the default label.
        assert!(
            reloc.symbol.contains("match_arm") || reloc.symbol.contains("match_default"),
            "Relocation {} should reference arm or default label, got: {}",
            i, reloc.symbol
        );
    }
}
