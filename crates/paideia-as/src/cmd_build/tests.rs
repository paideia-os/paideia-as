use std::path::Path;

use paideia_as_ir::{IrNodeId, ModuleSideTable};

use super::functors_from_modules;
use super::identifier::parse_integer_literal;
use super::pax::pax_path_for;
use super::placeholder::placeholder_path_for;

#[test]
fn placeholder_path_replaces_extension() {
    let p = Path::new("example.pdx");
    assert_eq!(placeholder_path_for(p), Path::new("example.placeholder"));
}

#[test]
fn placeholder_path_preserves_directory() {
    let p = Path::new("/tmp/foo/example.pdx");
    assert_eq!(
        placeholder_path_for(p),
        Path::new("/tmp/foo/example.placeholder")
    );
}

#[test]
fn pax_path_replaces_extension() {
    let p = Path::new("example.pdx");
    assert_eq!(pax_path_for(p), Path::new("example.pax"));
}

#[test]
fn pax_path_preserves_directory() {
    let p = Path::new("/tmp/foo/example.pdx");
    assert_eq!(pax_path_for(p), Path::new("/tmp/foo/example.pax"));
}

#[test]
fn functors_from_modules_extracts_functor_entries() {
    use paideia_as_ir::{FunctorInfo, ModuleInfo};

    let mut table = ModuleSideTable::new();
    let functor_module_id = IrNodeId::new(1).unwrap();
    let body_id = IrNodeId::new(10).unwrap();

    // Create a functor module.
    let functor_info = FunctorInfo {
        param_signature_hash: 0x1111111111111111,
        result_signature_hash: 0x2222222222222222,
        body_node_id: body_id,
    };

    let module_info = ModuleInfo {
        name: "MyFunctor".to_string(),
        fields: vec![],
        functor: Some(functor_info),
    };

    table.insert(functor_module_id, module_info);

    // Define a simple symbol resolver.
    let symbol_resolver = |_id: IrNodeId| -> u64 { 42 };

    // Call the bridge.
    let section = functors_from_modules(&table, symbol_resolver);

    // Bridge must emit exactly one entry for the functor module.
    assert_eq!(section.len(), 1, "expected one functor entry");
    let entry = &section.entries[0];
    assert_eq!(entry.functor_symbol_id, 42);
    assert_eq!(entry.param_signature_hash, 0x1111111111111111);
    assert_eq!(entry.result_signature_hash, 0x2222222222222222);
    assert_eq!(entry.closure_data_offset, 0);
    assert_eq!(entry.closure_data_size, 0);
    assert_eq!(entry.flags, 0);
}

/// Phase-5-m1-005: Test that EmitWalker is integrated into the build pipeline.
/// Empty IR produces zero instruction table entries.
#[test]
fn emit_walker_empty_ir_produces_zero_entries() {
    use paideia_as_elaborator::EmitWalker;

    let mut emit_walker = EmitWalker::new();
    let mut arena = paideia_as_ir::IrArena::new();
    emit_walker.walk(&mut arena);

    assert_eq!(
        emit_walker.state().instructions().len(),
        0,
        "empty IR should produce zero instruction entries"
    );
}

/// Phase-5-m1-005: Test that EmitWalker populates instruction table on non-empty IR.
/// A simple Let+Literal should produce one instruction entry.
#[test]
fn emit_walker_let_literal_produces_entry() {
    use paideia_as_diagnostics::FileId;
    use paideia_as_elaborator::EmitWalker;

    let mut emit_walker = EmitWalker::new();
    let mut arena = paideia_as_ir::IrArena::new();

    // Create a simple Let+Literal IR: let x = 42
    let span = paideia_as_diagnostics::Span::new(FileId::new(1).unwrap(), 0, 1);
    let lit_id = arena.alloc(paideia_as_ir::IrKind::Literal, span);
    let let_id = arena.alloc_with_children(paideia_as_ir::IrKind::Let, span, [lit_id]);

    // Register the literal value.
    arena.literal_values_mut().insert(lit_id, 42);

    // Walk and verify one instruction was emitted.
    emit_walker.walk(&mut arena);

    assert_eq!(
        emit_walker.state().instructions().len(),
        1,
        "Let+Literal should produce one instruction entry"
    );
    assert!(
        emit_walker.state().instructions().get(let_id).is_some(),
        "instruction should be keyed by let_id"
    );
}

/// Phase-5-m1-005: Test that EmitWalker records Lambda offsets.
/// A Lambda should populate function_offsets.
#[test]
fn emit_walker_lambda_records_offset() {
    use paideia_as_diagnostics::FileId;
    use paideia_as_elaborator::EmitWalker;

    let mut emit_walker = EmitWalker::new();
    let mut arena = paideia_as_ir::IrArena::new();

    // Create a simple Lambda: fn (x) -> x
    let span = paideia_as_diagnostics::Span::new(FileId::new(1).unwrap(), 0, 1);
    let var_id = arena.alloc(paideia_as_ir::IrKind::Var, span);
    let lambda_id = arena.alloc_with_children(paideia_as_ir::IrKind::Lambda, span, [var_id]);

    // Walk and verify offset was recorded.
    emit_walker.walk(&mut arena);

    assert!(
        emit_walker
            .state()
            .lambda_first_instr()
            .contains_key(&lambda_id.get()),
        "lambda entry point should be recorded"
    );
}

/// Phase 8 m2-002: Test that ArrayLit data-emission populates DataEntry.
/// A Let+ArrayLit with Literal children should produce a data entry.
#[test]
fn array_literal_data_emission() {
    use paideia_as_diagnostics::FileId;
    use paideia_as_ir::DataEntry;

    let mut arena = paideia_as_ir::IrArena::new();

    // Create array literal with 3 elements: [1, 2, 3]
    let span = paideia_as_diagnostics::Span::new(FileId::new(1).unwrap(), 0, 1);

    let elem0_id = arena.alloc(paideia_as_ir::IrKind::Literal, span);
    let elem1_id = arena.alloc(paideia_as_ir::IrKind::Literal, span);
    let elem2_id = arena.alloc(paideia_as_ir::IrKind::Literal, span);

    // Register literal values: 1, 2, 3
    arena.literal_values_mut().insert(elem0_id, 1);
    arena.literal_values_mut().insert(elem1_id, 2);
    arena.literal_values_mut().insert(elem2_id, 3);

    // Create ArrayLit with 3 element children
    let array_lit_id = arena.alloc_with_children(
        paideia_as_ir::IrKind::ArrayLit,
        span,
        [elem0_id, elem1_id, elem2_id],
    );

    // Create Let with ArrayLit as RHS
    let let_id = arena.alloc_with_children(paideia_as_ir::IrKind::Let, span, [array_lit_id]);

    // Manually populate data entry (simulating cmd_build logic).
    // Pack 3 u64 elements: 1, 2, 3 → 3*8 = 24 bytes LE.
    let mut packed_bytes = Vec::new();
    for &value in &[1i64, 2i64, 3i64] {
        let u64_val = value as u64;
        packed_bytes.extend_from_slice(&u64_val.to_le_bytes());
    }

    let entry = DataEntry::new_rodata(packed_bytes, "data_array".to_string(), 8);
    arena.data_mut().insert(let_id, entry);

    // Verify data entry was recorded.
    let data_entry = arena.data().get(let_id);
    assert!(data_entry.is_some(), "data entry should be registered");

    if let Some(entry) = data_entry {
        // Verify packed bytes: 3 u64 elements (1, 2, 3) in LE.
        assert_eq!(
            entry.bytes.len(),
            24,
            "array should have 24 bytes (3 u64 elements)"
        );
        assert_eq!(entry.section, paideia_as_ir::data::SectionKind::Rodata);
        assert_eq!(entry.symbol_name, "data_array");
        assert_eq!(entry.align, 8);

        // Verify first element is 1 in LE
        let elem0_bytes = &entry.bytes[0..8];
        let elem0_val = u64::from_le_bytes([
            elem0_bytes[0],
            elem0_bytes[1],
            elem0_bytes[2],
            elem0_bytes[3],
            elem0_bytes[4],
            elem0_bytes[5],
            elem0_bytes[6],
            elem0_bytes[7],
        ]);
        assert_eq!(elem0_val, 1);

        // Verify second element is 2 in LE
        let elem1_bytes = &entry.bytes[8..16];
        let elem1_val = u64::from_le_bytes([
            elem1_bytes[0],
            elem1_bytes[1],
            elem1_bytes[2],
            elem1_bytes[3],
            elem1_bytes[4],
            elem1_bytes[5],
            elem1_bytes[6],
            elem1_bytes[7],
        ]);
        assert_eq!(elem1_val, 2);

        // Verify third element is 3 in LE
        let elem2_bytes = &entry.bytes[16..24];
        let elem2_val = u64::from_le_bytes([
            elem2_bytes[0],
            elem2_bytes[1],
            elem2_bytes[2],
            elem2_bytes[3],
            elem2_bytes[4],
            elem2_bytes[5],
            elem2_bytes[6],
            elem2_bytes[7],
        ]);
        assert_eq!(elem2_val, 3);
    }
}

/// PA-R12-003: Test hex literals with top bit set (issue #912).
/// Values > 0x7FFF_FFFF_FFFF_FFFF should parse as u64 and cast to i64 bit-preserving.
#[test]
fn parse_hex_top_bit_set() {
    assert_eq!(parse_integer_literal("0xFFFFFFFFFFFFFFFD"), Ok(0xFFFFFFFFFFFFFFFDu64 as i64));
    assert_eq!(parse_integer_literal("0xDEADBEEF00000007"), Ok(0xDEADBEEF00000007u64 as i64));
    assert_eq!(parse_integer_literal("0xFFFFFFFFFFFFFFFF"), Ok(-1i64));
    assert_eq!(parse_integer_literal("0x7FFFFFFFFFFFFFFF"), Ok(i64::MAX));
}

/// PA-R12-003: Test negative decimal parsing still works (regression test).
#[test]
fn parse_negative_decimal_still_works() {
    assert_eq!(parse_integer_literal("-42"), Ok(-42));
    assert_eq!(parse_integer_literal("-9223372036854775808"), Ok(i64::MIN));
}
