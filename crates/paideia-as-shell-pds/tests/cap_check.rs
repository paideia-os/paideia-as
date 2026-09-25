//! R227.M2 capability-declaration checker fixture corpus.
//!
//! 12 unit tests, tagged `r227m2-cap-NN`, exercising the subset
//! semantics of `paideia_as_shell_pds::check_subset` and
//! `CapabilitySet`; plus one end-to-end fixture that runs
//! `parse_header` followed by `PdsHeader::check_against` to confirm
//! the header/checker seam.

use paideia_as_shell_pds::{
    check_subset, parse_header, CapCheckError, CapabilitySet,
};

/// Convenience: build a `CapabilitySet` from string literals.
fn invoker(caps: &[&str]) -> CapabilitySet {
    CapabilitySet::from_iter(caps.iter().map(|s| (*s).to_owned()))
}

/// Convenience: build a declared-capabilities `Vec<String>` from
/// string literals.
fn declared(caps: &[&str]) -> Vec<String> {
    caps.iter().map(|s| (*s).to_owned()).collect()
}

// ────────────────────────────────────────────────────────────────
// r227m2-cap-01 .. r227m2-cap-07 — passing cases
// ────────────────────────────────────────────────────────────────

#[test]
fn r227m2_cap_01_empty_declared_nonempty_invoker_ok() {
    let inv = invoker(&["fs.read", "net.dial"]);
    let dec = declared(&[]);
    let result = check_subset(&dec, &inv);
    assert!(
        result.is_ok(),
        "r227m2-cap-01: empty declared always passes, got {result:?}"
    );
}

#[test]
fn r227m2_cap_02_empty_declared_empty_invoker_ok() {
    let inv = CapabilitySet::new();
    let dec = declared(&[]);
    assert!(inv.is_empty(), "r227m2-cap-02: sanity — invoker is empty");
    assert_eq!(inv.len(), 0, "r227m2-cap-02: sanity — len is zero");
    let result = check_subset(&dec, &inv);
    assert!(
        result.is_ok(),
        "r227m2-cap-02: nothing demanded ⇒ nothing missing, got {result:?}"
    );
}

#[test]
fn r227m2_cap_03_proper_subset_ok() {
    let inv = invoker(&["fs.read", "fs.write", "net.dial", "net.listen"]);
    let dec = declared(&["fs.read", "net.dial"]);
    let result = check_subset(&dec, &inv);
    assert!(
        result.is_ok(),
        "r227m2-cap-03: declared is a proper subset, got {result:?}"
    );
}

#[test]
fn r227m2_cap_04_exact_match_ok() {
    let inv = invoker(&["fs.read", "net.dial"]);
    let dec = declared(&["fs.read", "net.dial"]);
    let result = check_subset(&dec, &inv);
    assert!(
        result.is_ok(),
        "r227m2-cap-04: exact equality is trivially a subset, got {result:?}"
    );
}

#[test]
fn r227m2_cap_05_superset_invoker_extras_ok() {
    // Invoker has many rights the script does not declare — extras
    // on the invoker side never fail a subset check.
    let inv = invoker(&[
        "fs.read", "fs.write", "fs.stat", "net.dial", "net.listen",
        "proc.spawn", "sys.time",
    ]);
    let dec = declared(&["fs.read"]);
    let result = check_subset(&dec, &inv);
    assert!(
        result.is_ok(),
        "r227m2-cap-05: unused invoker rights don't matter, got {result:?}"
    );
}

#[test]
fn r227m2_cap_06_single_cap_match_ok() {
    let inv = invoker(&["fs.read.home"]);
    let dec = declared(&["fs.read.home"]);
    let result = check_subset(&dec, &inv);
    assert!(
        result.is_ok(),
        "r227m2-cap-06: single-cap match passes, got {result:?}"
    );
}

#[test]
fn r227m2_cap_07_multi_cap_all_present_ok() {
    let inv = invoker(&[
        "fs.read.home",
        "fs.write.home/backup",
        "net.dial",
        "sys.time",
    ]);
    let dec = declared(&[
        "fs.read.home",
        "fs.write.home/backup",
        "net.dial",
        "sys.time",
    ]);
    let result = check_subset(&dec, &inv);
    assert!(
        result.is_ok(),
        "r227m2-cap-07: all four declared are present, got {result:?}"
    );
}

// ────────────────────────────────────────────────────────────────
// r227m2-cap-08 .. r227m2-cap-12 — failure cases
// ────────────────────────────────────────────────────────────────

#[test]
fn r227m2_cap_08_single_missing_err() {
    let inv = invoker(&["fs.read"]);
    let dec = declared(&["fs.read", "net.dial"]);
    let err = check_subset(&dec, &inv).expect_err("r227m2-cap-08: must fail");
    match err {
        CapCheckError::MissingCapabilities { missing } => {
            assert_eq!(
                missing,
                vec!["net.dial".to_owned()],
                "r227m2-cap-08: exactly one missing"
            );
        }
    }
}

#[test]
fn r227m2_cap_09_all_missing_err() {
    let inv = invoker(&["proc.spawn"]);
    let dec = declared(&["fs.read", "net.dial", "sys.time"]);
    let err = check_subset(&dec, &inv).expect_err("r227m2-cap-09: must fail");
    match err {
        CapCheckError::MissingCapabilities { missing } => {
            assert_eq!(
                missing,
                vec![
                    "fs.read".to_owned(),
                    "net.dial".to_owned(),
                    "sys.time".to_owned(),
                ],
                "r227m2-cap-09: all three missing, in declaration order"
            );
        }
    }
}

#[test]
fn r227m2_cap_10_partial_missing_err() {
    let inv = invoker(&["fs.read", "sys.time"]);
    let dec = declared(&["fs.read", "net.dial", "sys.time", "proc.spawn"]);
    let err = check_subset(&dec, &inv).expect_err("r227m2-cap-10: must fail");
    match err {
        CapCheckError::MissingCapabilities { missing } => {
            assert_eq!(
                missing,
                vec!["net.dial".to_owned(), "proc.spawn".to_owned()],
                "r227m2-cap-10: only the unheld two, in declaration order"
            );
        }
    }
}

#[test]
fn r227m2_cap_11_empty_invoker_with_declared_err() {
    let inv = CapabilitySet::new();
    let dec = declared(&["fs.read"]);
    let err = check_subset(&dec, &inv).expect_err("r227m2-cap-11: must fail");
    match err {
        CapCheckError::MissingCapabilities { missing } => {
            assert_eq!(
                missing,
                vec!["fs.read".to_owned()],
                "r227m2-cap-11: empty invoker misses everything declared"
            );
        }
    }
}

#[test]
fn r227m2_cap_12_case_sensitive_err() {
    // Invoker holds the uppercase spelling; script declares the
    // lowercase spelling. The M2 checker treats these as distinct
    // (no canonicalisation) — the arbiter, not this crate, decides
    // when two spellings should map to one right.
    let inv = invoker(&["FS.READ"]);
    let dec = declared(&["fs.read"]);
    let err = check_subset(&dec, &inv).expect_err("r227m2-cap-12: must fail");
    match err {
        CapCheckError::MissingCapabilities { missing } => {
            assert_eq!(
                missing,
                vec!["fs.read".to_owned()],
                "r227m2-cap-12: case-exact match is required"
            );
        }
    }
    // Cross-check the symmetric direction, to prove the invoker's
    // uppercase entry did not silently satisfy the lowercase demand.
    let inv2 = invoker(&["fs.read"]);
    assert!(
        !inv2.contains("FS.READ"),
        "r227m2-cap-12: CapabilitySet::contains is also byte-exact"
    );
    assert!(
        inv2.contains("fs.read"),
        "r227m2-cap-12: byte-equal lookup finds the stored name"
    );
}

// ────────────────────────────────────────────────────────────────
// Integration — parse_header → PdsHeader::check_against(&invoker)
// ────────────────────────────────────────────────────────────────

#[test]
fn r227m2_cap_integration_parse_then_check() {
    // A realistic `.pds` header: three declared capabilities, one
    // of which the invoker will lack. The end-to-end flow must
    // reach `MissingCapabilities` naming only the unheld cap.
    let src = "#capability \"fs.read.home\"\n\
               #capability \"net.dial\"\n\
               #capability \"proc.spawn\"\n\n\
               body\n";
    let header = parse_header(src).expect("r227m2-cap-integration: parse must succeed");
    assert_eq!(
        header.capabilities.len(),
        3,
        "r227m2-cap-integration: three caps captured from header"
    );

    // Case A: invoker holds all three → check_against passes.
    let full = invoker(&["fs.read.home", "net.dial", "proc.spawn", "sys.time"]);
    header
        .check_against(&full)
        .expect("r227m2-cap-integration: full invoker must pass");

    // Case B: invoker missing `proc.spawn` → check_against fails
    // with exactly that name in `missing`, preserving declaration
    // order (not hash order).
    let partial = invoker(&["fs.read.home", "net.dial"]);
    let err = header
        .check_against(&partial)
        .expect_err("r227m2-cap-integration: partial invoker must fail");
    match err {
        CapCheckError::MissingCapabilities { missing } => {
            assert_eq!(
                missing,
                vec!["proc.spawn".to_owned()],
                "r227m2-cap-integration: only the missing cap is reported"
            );
        }
    }
}
