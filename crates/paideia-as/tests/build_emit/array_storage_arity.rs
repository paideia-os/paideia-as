//! Byte-exact storage-allocation guards for array-typed module bindings.
//!
//! Issue #1308 — `[v; N]` repeat literals were expanded to a *single* element
//! while the type system still reported `[T; N]`. `runqueue : [u64; 2]` and
//! `_loader_seed_empty_sidecar : [u64; 2]` linked at 8 bytes in the shipping
//! paideia-os kernel image, so any write to index 1 landed in the next symbol.
//!
//! Issue #1309 — the module-level array-literal data pass emitted one packed
//! element per *encodable* child and never reconciled that against the declared
//! arity, so a short initialiser list (paideia-os `_frame_meta : [u64; 1024]`
//! written with 992 elements) or a non-constant element silently produced an
//! undersized symbol.
//!
//! These tests assert on the linked symbol size *and* the emitted bytes, so a
//! regression cannot hide behind a correct-looking size with wrong contents.

use object::{Object, ObjectSection, ObjectSymbol};

use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

/// Return the exact bytes a symbol occupies, resolved through the section the
/// symbol actually lives in (`.data` / `.rodata` / a `@link_section` override).
///
/// Unlike `common::elf::symbol_bytes` this is not `.text`-only, which is what
/// data-storage assertions need.
fn symbol_storage(elf_bytes: &[u8], symbol: &str) -> (u64, Vec<u8>) {
    let file = object::File::parse(elf_bytes).expect("object should parse the ELF");

    let sym = file
        .symbols()
        .find(|s| s.name().unwrap_or("") == symbol)
        .unwrap_or_else(|| panic!("symbol `{symbol}` not found in ELF"));

    let section_index = match sym.section() {
        object::SymbolSection::Section(idx) => idx,
        other => panic!("symbol `{symbol}` is not in a regular section: {other:?}"),
    };
    let section = file
        .section_by_index(section_index)
        .expect("symbol's section index resolves");
    let section_data = section.data().unwrap_or(b"");

    let start = sym.address() as usize;
    let size = sym.size() as usize;
    assert!(
        start + size <= section_data.len(),
        "symbol `{symbol}` at {start}+{size} overruns section `{}` of {} bytes",
        section.name().unwrap_or("?"),
        section_data.len()
    );

    (sym.size(), section_data[start..start + size].to_vec())
}

#[test]
fn repeat_literal_u64_two_allocates_sixteen_zero_bytes() {
    // `pub let mut repeat_two : [u64; 2] = [0; 2]`
    let out = run_build(build_emit("pa1308_repeat_u64.pdx"));
    out.assert_ok();

    let (size, bytes) = symbol_storage(&out.artifact_bytes(), "repeat_two");

    assert_eq!(size, 16, "[u64; 2] = [0; 2] must link at 16 bytes, not 8");
    assert_eq!(bytes, vec![0u8; 16], "both qwords must be zero");
}

#[test]
fn repeat_literal_u64_512_is_not_small_n_special_cased() {
    // `pub let mut repeat_many : [u64; 512] = [0; 512]`
    let out = run_build(build_emit("pa1308_repeat_u64_big.pdx"));
    out.assert_ok();

    let (size, bytes) = symbol_storage(&out.artifact_bytes(), "repeat_many");

    assert_eq!(
        size, 4096,
        "[u64; 512] = [0; 512] must link at 512 * 8 bytes"
    );
    assert_eq!(bytes, vec![0u8; 4096], "all 512 qwords must be zero");
}

#[test]
fn repeat_literal_honours_value_and_element_width() {
    // `pub let mut repeat_seven : [u32; 4] = [7; 4]` — proves the repeated
    // value is replicated (not zero-filled) and that per-element width is
    // taken from the declared type rather than assumed to be 8.
    let out = run_build(build_emit("pa1308_repeat_u32_nonzero.pdx"));
    out.assert_ok();

    let (size, bytes) = symbol_storage(&out.artifact_bytes(), "repeat_seven");

    assert_eq!(size, 16, "[u32; 4] must link at 4 * 4 bytes");
    let expected: Vec<u8> = (0..4).flat_map(|_| 7u32.to_le_bytes()).collect();
    assert_eq!(
        bytes, expected,
        "every element must hold the literal 7 as u32 LE"
    );
}

#[test]
fn non_constant_repeat_count_is_rejected_with_p0211() {
    // A repeat count that is not a resolvable literal must be a hard error.
    // Silently collapsing to one element is the behaviour #1308 removed.
    let out = run_build(build_emit("pa1308_repeat_nonconst_count.pdx"));
    out.assert_diag("P0211");
}

#[test]
fn short_array_initialiser_is_rejected_with_t0576() {
    // `[u64; 4] = [1, 2, 3]` used to link at 24 bytes with no diagnostic.
    let out = run_build(build_emit("pa1309_array_arity_short.pdx"));
    out.assert_diag("T0576");
}

#[test]
fn unencodable_array_element_is_rejected_with_t0576() {
    // `[u64; 4] = [1, 2, k, 4]` where `k` is a binding, not a constant: the
    // packing loop used to skip `k` and emit a 24-byte symbol.
    let out = run_build(build_emit("pa1309_array_arity_unencodable.pdx"));
    out.assert_diag("T0576");
}

#[test]
fn array_with_no_encodable_elements_is_out_of_t0576_scope() {
    // Issue #1310 scope boundary. When NOTHING encodes, the ArrayLit data
    // branch never claimed the array — it emits no data entry and therefore no
    // symbol, which is a *link*-time failure for any user, not a silent short
    // read. paideia-os `_klog_files : [u64; 205] = [(rip + name_file_0), ...]`
    // is the live instance. T0576 is deliberately scoped to partial emission
    // and must not fire here; widening it would report an arity mismatch for
    // an unimplemented element shape.
    //
    // This test pins the boundary in both directions: the build must succeed,
    // and the symbol must be absent rather than present-and-undersized.
    let out = run_build(build_emit("pa1310_array_all_unencodable.pdx"));
    out.assert_ok();
    assert!(
        !out.stderr_contains("T0576"),
        "T0576 must not fire when no element encodes:\n{}",
        out.stderr
    );

    let bytes = out.artifact_bytes();
    let file = object::File::parse(&*bytes).expect("object should parse the ELF");
    assert!(
        !file
            .symbols()
            .any(|s| s.name().unwrap_or("") == "all_unencodable"),
        "the array emits no data entry, so no symbol should exist (see #1310)"
    );
}
