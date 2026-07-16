//! Issue #1217: @jump_table with holes creates rodata relocations to match_default_<N> labels.
//! Without the fix, these labels are not exported as ELF symbols, causing LD to fail
//! with "undefined reference to match_default_<N>".
//!
//! Tests that when a @jump_table has a density_ok=true configuration but non-consecutive
//! coverage (e.g., values 0,1,2,4 with slot 3 as a hole), the emitted ELF object's
//! symbol table includes the match_default_<N> label as a defined symbol.

use object::{Object, ObjectSymbol};

use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

#[test]
fn semi_sparse_jump_table_compiles_and_exports_default_label() {
    let input = build_emit("pa_r15_009_jump_table_semi_sparse.pdx");
    let out = run_build(input);

    // Assertion 1: build_emit(...) succeeds with no error diagnostics.
    out.assert_ok();

    // Parse the emitted ELF object.
    let bytes = out.artifact_bytes();
    let obj = object::File::parse(&*bytes).expect("parse ELF object");

    // Assertion 2: The emitted ELF's symbol table contains a match_default_<N> entry that is DEFINED.
    // An undefined symbol (address = 0, not a local/global function) indicates the linker will fail
    // with "undefined reference to match_default_<N>" when linking this object file.
    let mut found_defined_default_symbol = false;
    let mut all_symbols = Vec::new();
    let mut default_symbols = Vec::new();

    for symbol in obj.symbols() {
        if let Ok(name) = symbol.name() {
            all_symbols.push(name.to_string());
            if name.starts_with("match_default_") {
                default_symbols.push((name.to_string(), symbol.address()));
                // A defined symbol will have a non-zero address or be marked as a function
                // (with non-zero value). An undefined symbol has address 0.
                if symbol.address() != 0 || symbol.is_definition() {
                    found_defined_default_symbol = true;
                }
            }
        }
    }

    assert!(
        found_defined_default_symbol,
        "Expected a DEFINED match_default_<N> symbol in ELF symbol table for semi-sparse jump table. \
         Default symbols found: {:?}. Without a defined symbol, relocations to the default label fail during linking. \
         All symbols: {:?}",
        default_symbols,
        all_symbols
    );
}
