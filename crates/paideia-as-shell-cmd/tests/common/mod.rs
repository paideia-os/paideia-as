//! Shared helpers for the R222.M1..M3 test corpora.
//!
//! Every fixture below stamps a fingerprint (`r222-m1-schema-NN`,
//! `r222-m2-cmd-NN`, `r222-m3-cmd-NN`) so failures point at the exact
//! canary that broke. Matches the R220.M10 fingerprint discipline and
//! the R221.M4 lexer-fixture convention.

#![allow(dead_code)] // helpers used by only a subset of files

/// Assert the two byte-strings are equal; on mismatch print both under
/// the fingerprint tag for a debuggable failure.
pub fn assert_eq_tagged<T: PartialEq + std::fmt::Debug>(
    fingerprint: &str,
    got: T,
    expected: T,
) {
    if got != expected {
        panic!(
            "{fingerprint}: mismatch\n  expected: {expected:?}\n       got: {got:?}"
        );
    }
    assert!(!fingerprint.is_empty());
}
