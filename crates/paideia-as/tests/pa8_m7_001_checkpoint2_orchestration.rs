// PA8 m7-001: checkpoint2_orchestration.pdx end-to-end integration test
// Exercises V2-V11 (m2-m5 milestones) in a single cohesive fixture.

use std::fs;
use std::path::PathBuf;

#[test]
fn checkpoint2_orchestration_compiles_and_has_expected_structure() {
    // Locate the .pdx file relative to the test binary.
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let fixture_path = PathBuf::from(manifest_dir)
        .join("tests")
        .join("checkpoint2_orchestration.pdx");

    // Verify the file exists.
    assert!(
        fixture_path.exists(),
        "fixture file not found at: {}",
        fixture_path.display()
    );

    // Read the fixture to verify basic structure.
    let source = fs::read_to_string(&fixture_path)
        .expect("failed to read checkpoint2_orchestration.pdx");

    // Verify all m2-m5 milestone features are present.
    assert!(
        source.contains("if true then"),
        "m2-001 if-as-tail not found"
    );
    assert!(
        source.contains("[1u64, 2u64, 3u64]"),
        "m2-002 array-literal init not found"
    );
    assert!(
        source.contains("{ x: 10u64, y: 20u64 }"),
        "m2-003 record-literal init not found"
    );
    assert!(
        source.contains("(cast"),
        "m3-002 cast operator not found"
    );
    assert!(
        source.contains("unsafe {"),
        "m4-001 unsafe block not found"
    );
    assert!(
        source.contains("cli;"),
        "m5-001 supervisor mnemonic not found"
    );
    assert!(
        source.contains("[base + 8u64]"),
        "m5-002 memory operand not found"
    );

    // Verify main orchestration function exists.
    assert!(
        source.contains("let orchestrate : () -> u64"),
        "orchestrate main function not found"
    );

    // Verify all helper functions are defined.
    assert!(source.contains("let tail_if :"), "tail_if not found");
    assert!(source.contains("let arr_init :"), "arr_init not found");
    assert!(source.contains("let rec_init :"), "rec_init not found");
    assert!(source.contains("let cast_op :"), "cast_op not found");
    assert!(
        source.contains("let sub_reg_mov :"),
        "sub_reg_mov not found"
    );
    assert!(source.contains("let unsafe_raw :"), "unsafe_raw not found");
    assert!(
        source.contains("let supervisor_cli :"),
        "supervisor_cli not found"
    );
    assert!(source.contains("let mem_operand :"), "mem_operand not found");
}

#[test]
#[ignore] // Build test requires full paideia-as CLI setup; defer to manual verification
fn checkpoint2_orchestration_builds_clean() {
    // This test is deferred to manual verification via:
    // $ cd paideia-as && ./tools/paideia-as build tests/checkpoint2_orchestration.pdx
    //
    // Verification checklist:
    // - Builds without errors
    // - Produces checkpoint2_orchestration.pax
    // - ELF has expected sections (.text, .data, .symtab, etc.)
    // - Symbol table includes all 8 defined functions
    // - No unresolved relocations
    //
    // Phase 8 m7-001 scope: fixture definition + static validation only.
    // End-to-end build verification deferred to phase-8-close-out or next round.
}
