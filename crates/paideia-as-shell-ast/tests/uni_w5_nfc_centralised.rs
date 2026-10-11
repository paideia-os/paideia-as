//! UNI-W5 wire-up fixture (paideia-as#1599) for the shell-ast NFC
//! compose path.
//!
//! `NfcMap::build` now delegates the non-ASCII NFC compose to
//! `paideia_as_unicode::nfc_normalize`. This test guards the
//! delegation: any future divergence (an impl swap on either side,
//! or an accidental re-introduction of a direct
//! `UnicodeNormalization::nfc()` call) would surface as a diff
//! between the two paths.

use paideia_as_shell_ast::nfc_map::NfcMap;

/// uni-w5-ast-01 — decomposed `é` composes to the same bytes
/// through `NfcMap::build` and through `paideia_as_unicode::nfc_normalize`.
///
/// Input: `cafe\u{0301}` (lowercase `e` + COMBINING ACUTE ACCENT, i.e.
/// the decomposed form of `café`). The NFC form is `caf\u{00E9}`.
/// `NfcMap::build` returns the composed string as its first tuple
/// element; the substrate's `nfc_normalize` must return the same
/// bytes. Pre-UNI-W5 both paths called `UnicodeNormalization::nfc`
/// directly and happened to agree; the delegation makes the agreement
/// mechanical.
#[test]
fn uni_w5_ast_01_build_delegates_to_substrate_nfc() {
    let decomposed = "cafe\u{0301}";
    let expected = paideia_as_unicode::nfc_normalize(decomposed);
    let (composed, _map) = NfcMap::build(decomposed);
    assert_eq!(
        composed, expected,
        "NfcMap::build's non-ASCII path must delegate to paideia_as_unicode::nfc_normalize"
    );
    // Independent spot check: composed == `caf\u{00E9}` byte-for-byte.
    assert_eq!(composed, "caf\u{00E9}", "canonical NFC form of 'café'");
}
