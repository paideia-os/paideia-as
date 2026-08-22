//! NIST ACVP FIPS-204 (ML-DSA) known-answer tests, ML-DSA-65 parameter set.
//!
//! Retires the deterministic-rnd round-trip KAT that shipped with m7-001 and
//! satisfies issue #525 (Phase 3 m8) via the "vendor NIST JSON directly"
//! trigger from `tests/pq-corpus/ML_DSA_ACVP_STATUS.md`.
//!
//! Corpus provenance: `tests/pq-corpus/vectors/README.md` (pinned to
//! `usnistgov/ACVP-Server @ 65370b8` — the same commit RustCrypto's
//! `ml-dsa` crate tracks upstream, whose `Cargo.toml` `exclude` list
//! keeps them out of the published crate).
//!
//! Coverage (60 vectors total, ML-DSA-65 only):
//!  - keyGen: 25 vectors — seed → pk derivation matches NIST.
//!  - sigGen (deterministic): 10 vectors — (sk, msg, rnd=[0;32]) → sig
//!    matches NIST byte-for-byte.
//!  - sigGen (hedged):        10 vectors — (sk, msg, rnd) → sig matches
//!    NIST byte-for-byte.
//!  - sigVer: 15 vectors — verify outcome matches `testPassed` (3 valid,
//!    12 tampered/malformed with ACVP-supplied `reason`).
//!
//! Each of the four test functions loops over every vector in its file so a
//! single failure is easy to localise via the `tcId` in the assert message.

use paideia_pq_sign::mldsa::kat;
use paideia_pq_sign::mldsa::{PublicKey, Signature};
use paideia_pq_sign::{MlDsa65Marker, Signer};
use pq_corpus::acvp::{
    hex_to_array, hex_to_vec, load_keygen, load_siggen, load_sigver,
};

// ---------------------------------------------------------------------------
// keyGen: seed → pk (25 vectors)
// ---------------------------------------------------------------------------

#[test]
fn acvp_ml_dsa_65_keygen() {
    let corpus = load_keygen();
    assert_eq!(corpus.algorithm, "ML-DSA");
    assert_eq!(corpus.mode, "keyGen");

    let mut checked = 0usize;
    for group in &corpus.test_groups {
        assert_eq!(
            group.parameter_set, "ML-DSA-65",
            "vendored corpus should carry ML-DSA-65 only"
        );
        for t in &group.tests {
            let seed = hex_to_array::<32>(&t.seed, "seed", t.tc_id);
            let expected_pk = hex_to_vec(&t.pk, "pk", t.tc_id);

            let actual_pk = kat::keygen_pk_from_seed(&seed);

            assert_eq!(
                actual_pk.len(),
                paideia_pq_sign::MLDSA65_PK_LEN,
                "tcId={}: pk length",
                t.tc_id
            );
            assert_eq!(
                actual_pk, expected_pk,
                "tcId={}: derived pk does not match NIST",
                t.tc_id
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 25, "expected 25 keyGen vectors after vendoring");
}

// ---------------------------------------------------------------------------
// sigGen deterministic + hedged: (sk_expanded, msg, rnd) → sig  (20 vectors)
// ---------------------------------------------------------------------------

#[test]
fn acvp_ml_dsa_65_siggen() {
    let corpus = load_siggen();
    assert_eq!(corpus.algorithm, "ML-DSA");
    assert_eq!(corpus.mode, "sigGen");

    let mut deterministic_checked = 0usize;
    let mut hedged_checked = 0usize;

    for group in &corpus.test_groups {
        assert_eq!(group.parameter_set, "ML-DSA-65");
        for t in &group.tests {
            let sk = hex_to_array::<4032>(&t.sk, "sk", t.tc_id);
            let msg = hex_to_vec(&t.message, "message", t.tc_id);
            let expected_sig = hex_to_vec(&t.signature, "signature", t.tc_id);

            let rnd = if group.deterministic {
                assert!(
                    t.rnd.is_none(),
                    "tcId={}: deterministic group should not carry rnd",
                    t.tc_id
                );
                [0u8; 32]
            } else {
                let rnd_hex = t.rnd.as_deref().unwrap_or_else(|| {
                    panic!("tcId={}: hedged group missing rnd", t.tc_id)
                });
                hex_to_array::<32>(rnd_hex, "rnd", t.tc_id)
            };

            let actual_sig = kat::sign_expanded_with_rnd(&sk, &msg, &rnd);

            assert_eq!(
                actual_sig.len(),
                paideia_pq_sign::MLDSA65_SIG_LEN,
                "tcId={}: signature length",
                t.tc_id
            );
            assert_eq!(
                actual_sig, expected_sig,
                "tcId={} (deterministic={}): signature does not match NIST",
                t.tc_id, group.deterministic
            );

            if group.deterministic {
                deterministic_checked += 1;
            } else {
                hedged_checked += 1;
            }
        }
    }

    assert_eq!(deterministic_checked, 10, "expected 10 deterministic sigGen vectors");
    assert_eq!(hedged_checked, 10, "expected 10 hedged sigGen vectors");
}

// ---------------------------------------------------------------------------
// sigVer: (pk, msg, sig) → bool matches testPassed  (15 vectors)
// ---------------------------------------------------------------------------

#[test]
fn acvp_ml_dsa_65_sigver() {
    let corpus = load_sigver();
    assert_eq!(corpus.algorithm, "ML-DSA");
    assert_eq!(corpus.mode, "sigVer");

    let mut valid_seen = 0usize;
    let mut invalid_seen = 0usize;

    for group in &corpus.test_groups {
        assert_eq!(group.parameter_set, "ML-DSA-65");
        // Group-level pk is shared across every vector in this group.
        let pk_bytes = hex_to_vec(&group.pk, "group.pk", group.tg_id);
        assert_eq!(pk_bytes.len(), paideia_pq_sign::MLDSA65_PK_LEN);
        let pk = PublicKey(pk_bytes);

        for t in &group.tests {
            let msg = hex_to_vec(&t.message, "message", t.tc_id);
            let sig = Signature(hex_to_vec(&t.signature, "signature", t.tc_id));

            let actual = MlDsa65Marker::verify(&pk, &msg, &sig);
            assert_eq!(
                actual,
                t.test_passed,
                "tcId={} reason={:?}: verify outcome {} disagrees with NIST expected {}",
                t.tc_id, t.reason, actual, t.test_passed,
            );

            if t.test_passed {
                valid_seen += 1;
            } else {
                invalid_seen += 1;
            }
        }
    }

    // 2026-08-22 corpus census (documented for regression-visibility).
    assert_eq!(valid_seen, 3, "expected 3 valid sigVer vectors");
    assert_eq!(invalid_seen, 12, "expected 12 invalid sigVer vectors");
}

// ---------------------------------------------------------------------------
// Cross-check: every NIST sigGen signature must verify under the paideia
// wrapper's own `verify` path when we feed the pk recovered from the
// vector's expanded sk. Catches drifts that only surface at the boundary
// between wrapper types (Signature, PublicKey) and the underlying ml-dsa
// codec — the sigVer test above uses NIST-supplied pk, this one uses a
// pk we synthesize from the sigGen sk.
// ---------------------------------------------------------------------------

#[test]
fn acvp_ml_dsa_65_siggen_signatures_verify_under_wrapper() {
    let siggen = load_siggen();
    let mut cross_checked = 0usize;

    for group in &siggen.test_groups {
        for t in &group.tests {
            let sk = hex_to_array::<4032>(&t.sk, "sk", t.tc_id);
            let msg = hex_to_vec(&t.message, "message", t.tc_id);
            let sig_bytes = hex_to_vec(&t.signature, "signature", t.tc_id);

            let pk_bytes = kat::verifying_key_from_expanded_sk(&sk);
            assert_eq!(pk_bytes.len(), paideia_pq_sign::MLDSA65_PK_LEN);

            let pk = PublicKey(pk_bytes);
            let sig = Signature(sig_bytes);

            assert!(
                MlDsa65Marker::verify(&pk, &msg, &sig),
                "tcId={} (deterministic={}): wrapper verify must accept NIST signature",
                t.tc_id,
                group.deterministic
            );
            cross_checked += 1;
        }
    }
    assert_eq!(cross_checked, 20, "expected 20 sigGen cross-verify vectors");
}
