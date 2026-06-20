//! Regression: confirm all O1500-O1512 diagnostic codes are registered.
//!
//! Per Phase 2 m9-011: the optimization-pass catalog reserves O1500-O1599.
//! m9-002..009 populated O1500 + O1503..O1512; O1501 / O1502 are reserved
//! for future per-rewrite peephole diagnostics. Removing any of these
//! breaks the documented contract — this test trips on the removal.

use std::path::PathBuf;

fn catalog_path() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("catalog.toml");
    p
}

#[test]
fn opt_codes_o1500_through_o1512_are_registered() {
    let content = std::fs::read_to_string(catalog_path()).expect("catalog readable");
    for n in 1500u16..=1512 {
        let marker = format!("[diagnostic.O{n}]");
        assert!(content.contains(&marker), "missing O-code: {marker}");
    }
}

#[test]
fn at_least_ten_opt_codes_registered() {
    let content = std::fs::read_to_string(catalog_path()).expect("catalog readable");
    let count = (1500u16..=1599)
        .filter(|n| content.contains(&format!("[diagnostic.O{n}]")))
        .count();
    assert!(count >= 10, "expected at least 10 O codes, got {count}");
}
