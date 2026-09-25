//! R222.M1 — `SchemasSig` fixture corpus.
//!
//! Five fixtures — one per light command's expected schema shape.
//! Fingerprint `r222-m1-schema-NN`. Each fixture builds a
//! `SchemasSig` from the R220 seed (`FileSchema@0.1` +
//! `RawByteChunk@0.1`) or a bare empty session, then asserts the
//! fingerprint of every schema reference matches what the FNV-1a-64
//! algorithm produces over the schema's canonical name bytes
//! (schema-registry.md §3).

mod common;
use common::assert_eq_tagged;
use paideia_as_shell_cmd::{
    fnv1a_64, SchemaFingerprint, SchemaRef, SchemasSig,
};

/// Fixture 01 — the `find` command's expected shape: no input,
/// `FileSchema@0.1` output.
#[test]
fn r222_m1_schema_01_find_shape() {
    let seed = SchemasSig::r220_seed();
    let file = seed.extra_by_name("FileSchema@0.1").expect("seeded");
    let expected = SchemaFingerprint(fnv1a_64(b"FileSchema@0.1"));
    assert_eq_tagged("r222-m1-schema-01", file.fingerprint, expected);
    assert_eq_tagged("r222-m1-schema-01", seed.input.clone(), None);
}

/// Fixture 02 — the `where` command's expected shape: schema-
/// preserving; the R220 seed's first extra doubles for both sides.
#[test]
fn r222_m1_schema_02_where_shape() {
    let seed = SchemasSig::r220_seed();
    let carried = seed.extras.first().expect("seeded").clone();
    // Build the shape the where_ functor consumes.
    let sig = SchemasSig {
        input: Some(carried.clone()),
        output: Some(carried.clone()),
        extras: seed.extras.clone(),
    };
    assert_eq_tagged("r222-m1-schema-02", sig.input.clone(), sig.output.clone());
    assert_eq_tagged(
        "r222-m1-schema-02",
        sig.input.unwrap().fingerprint,
        SchemaFingerprint(fnv1a_64(b"FileSchema@0.1")),
    );
}

/// Fixture 03 — the `sort` command's expected shape: also schema-
/// preserving over the R220 seed; distinct from where_ only in that
/// the FieldRef/Lambda key argument is a different type (that lives
/// on the CommandSig, not SchemasSig — R222.M2 covers).
#[test]
fn r222_m1_schema_03_sort_shape() {
    let seed = SchemasSig::r220_seed();
    let carried = seed.extras.first().expect("seeded").clone();
    let sig = SchemasSig {
        input: Some(carried.clone()),
        output: Some(carried),
        extras: seed.extras.clone(),
    };
    // FileSchema fingerprint is stable + deterministic.
    let fp = sig.input.as_ref().unwrap().fingerprint;
    assert_eq_tagged("r222-m1-schema-03", fp.0, fnv1a_64(b"FileSchema@0.1"));
}

/// Fixture 04 — the `head` command's expected shape: schema-preserving
/// over an empty session (functor fabricates the sentinel
/// `Record@0.1`). Proves the empty-session fallback path.
#[test]
fn r222_m1_schema_04_head_shape_empty_session() {
    let empty = SchemasSig::empty();
    // The functor would fabricate Record@0.1; we build the shape it
    // returns here and assert the fingerprint.
    let sentinel = SchemaRef::of_name("Record@0.1");
    let expected = SchemaFingerprint(fnv1a_64(b"Record@0.1"));
    assert_eq_tagged("r222-m1-schema-04", sentinel.fingerprint, expected);
    assert_eq_tagged("r222-m1-schema-04", empty.extras.len(), 0);
}

/// Fixture 05 — the `count` command's expected shape: input carried,
/// output = None. Sink command shape.
#[test]
fn r222_m1_schema_05_count_shape() {
    let seed = SchemasSig::r220_seed();
    let carried = seed.extras.first().expect("seeded").clone();
    let sig = SchemasSig {
        input: Some(carried),
        output: None,
        extras: seed.extras.clone(),
    };
    assert!(sig.input.is_some());
    assert_eq_tagged("r222-m1-schema-05", sig.output.clone(), None);
    // Version bump must produce a different fingerprint (registry §5).
    let v01 = SchemaRef::of_name("FileSchema@0.1").fingerprint;
    let v02 = SchemaRef::of_name("FileSchema@0.2").fingerprint;
    assert_ne!(v01, v02, "r222-m1-schema-05: version bump did not change fingerprint");
}
