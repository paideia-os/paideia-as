//! R227.M8 `.pds` script-load fingerprint fixture corpus.
//!
//! 5 tests, tagged `r227m8-load-NN`, exercising the sink surface and
//! the determinism / isolation guarantees of
//! `paideia_as_shell_pds::emit_load`:
//!
//!   01 — `NullLoadSink` is safe to hand `emit_load` (no crash).
//!   02 — `CollectingLoadSink` captures exactly one tag per emit call.
//!   03 — Same bytes twice emit the same tag twice (deterministic).
//!   04 — Different bytes emit different tags (dispersion).
//!   05 — Two `CollectingLoadSink` instances do not share state.
//!
//! The fixtures deliberately do not hard-code the FNV-1a-64 output of
//! any given script — locking a hash value into the corpus would
//! either duplicate the algorithm from the crate under test (a
//! self-referential assertion) or freeze the interim FNV choice into
//! the observable API (the load-fingerprint module is expected to
//! move to BLAKE3 in the same release as the schema-registry
//! fingerprint). Instead the tests assert *shape* — a well-formed
//! `pds.load.<16 hex>` string — plus the invariants
//! (determinism, dispersion, per-sink isolation) that any hash-based
//! implementation of this contract must uphold.

use paideia_as_shell_pds::{emit_load, CollectingLoadSink, LoadSink, NullLoadSink};

/// Assert that `tag` matches the emitted shape `pds.load.` followed by
/// exactly 16 lower-case hexadecimal digits.
///
/// Factored out so every fixture that inspects a tag agrees on the
/// same shape check — a future hash swap that keeps the tag shape
/// (e.g. BLAKE3-64 rendered as 16 hex digits) requires no fixture
/// edits; one that changes the shape trips every check at once.
fn assert_well_formed_tag(tag: &str, ctx: &str) {
    let prefix = "pds.load.";
    assert!(
        tag.starts_with(prefix),
        "{ctx}: expected tag to start with {prefix:?}, got {tag:?}"
    );
    let hex = &tag[prefix.len()..];
    assert_eq!(
        hex.len(),
        16,
        "{ctx}: expected 16 hex digits after prefix, got {} in {tag:?}",
        hex.len()
    );
    assert!(
        hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')),
        "{ctx}: expected lower-case hex digits, got {tag:?}"
    );
}

// ────────────────────────────────────────────────────────────────
// r227m8-load-01 — NullLoadSink is a legal target for emit_load
// ────────────────────────────────────────────────────────────────

#[test]
fn r227m8_load_01_null_sink_accepts_emit() {
    // The load path is expected to be reachable in release binaries
    // that keep no observation. `NullLoadSink` is the target there;
    // the only invariant we can check locally is that handing it to
    // `emit_load` neither panics nor requires any interior state.
    let sink = NullLoadSink;
    emit_load(&sink, b"#!/usr/bin/env pds\n");
    emit_load(&sink, b""); // empty scripts must also be tolerated
    emit_load(&sink, &[0u8; 4096]); // non-textual byte content is fine
    // No assertion beyond "did not panic" — the sink is deliberately
    // opaque.  The `let _: &dyn LoadSink = &sink;` below just pins the
    // trait wiring at compile time in case a future refactor drops the
    // impl by accident.
    let _sink_as_trait: &dyn LoadSink = &sink;
}

// ────────────────────────────────────────────────────────────────
// r227m8-load-02 — CollectingLoadSink captures one tag per emit
// ────────────────────────────────────────────────────────────────

#[test]
fn r227m8_load_02_collecting_sink_captures_one_tag() {
    let sink = CollectingLoadSink::new();
    assert!(
        sink.snapshot().is_empty(),
        "r227m8-load-02: sanity — fresh sink is empty"
    );

    emit_load(&sink, b"#capability \"fs.read\"\n\nls\n");

    let tags = sink.snapshot();
    assert_eq!(
        tags.len(),
        1,
        "r227m8-load-02: one emit_load call must produce one tag, got {tags:?}"
    );
    assert_well_formed_tag(&tags[0], "r227m8-load-02");
}

// ────────────────────────────────────────────────────────────────
// r227m8-load-03 — determinism: same bytes → same tag
// ────────────────────────────────────────────────────────────────

#[test]
fn r227m8_load_03_same_bytes_emit_same_tag() {
    // Two emits of byte-identical script sources must produce two
    // byte-identical tags. This is the load-fingerprint contract's
    // headline guarantee: downstream correlators may hash the tag
    // itself and expect the two observations to fold together.
    let script: &[u8] = b"#requires-paideia >= 0.36.22\n\necho hello\n";
    let sink = CollectingLoadSink::new();
    emit_load(&sink, script);
    emit_load(&sink, script);

    let tags = sink.snapshot();
    assert_eq!(
        tags.len(),
        2,
        "r227m8-load-03: expected two tags, got {tags:?}"
    );
    assert_eq!(
        tags[0], tags[1],
        "r227m8-load-03: identical bytes must fingerprint identically"
    );
    assert_well_formed_tag(&tags[0], "r227m8-load-03");
}

// ────────────────────────────────────────────────────────────────
// r227m8-load-04 — dispersion: different bytes → different tags
// ────────────────────────────────────────────────────────────────

#[test]
fn r227m8_load_04_different_bytes_emit_different_tags() {
    // Two scripts that differ by a single byte must emit different
    // tags. FNV-1a-64 is not cryptographic, but its dispersion on
    // small edits is well-established (and the whole point of using a
    // hash rather than a byte-length or line-count for the
    // fingerprint). If a future algorithm swap loses this property
    // for these two inputs, we want to know at test time.
    let a: &[u8] = b"echo alpha\n";
    let b: &[u8] = b"echo beta\n";
    let sink = CollectingLoadSink::new();
    emit_load(&sink, a);
    emit_load(&sink, b);

    let tags = sink.snapshot();
    assert_eq!(
        tags.len(),
        2,
        "r227m8-load-04: expected two tags, got {tags:?}"
    );
    assert_ne!(
        tags[0], tags[1],
        "r227m8-load-04: different bytes must fingerprint differently"
    );
    assert_well_formed_tag(&tags[0], "r227m8-load-04 (a)");
    assert_well_formed_tag(&tags[1], "r227m8-load-04 (b)");
}

// ────────────────────────────────────────────────────────────────
// r227m8-load-05 — two CollectingLoadSink instances are isolated
// ────────────────────────────────────────────────────────────────

#[test]
fn r227m8_load_05_sinks_are_isolated() {
    // Emitting into one sink must not touch another. The sink's
    // internal state is documented as private-per-instance; this
    // fixture pins that contract so a future refactor to a shared
    // static buffer would trip it immediately.
    let left = CollectingLoadSink::new();
    let right = CollectingLoadSink::new();

    emit_load(&left, b"only-left\n");

    assert_eq!(
        left.snapshot().len(),
        1,
        "r227m8-load-05: left sink must have captured its own emit"
    );
    assert!(
        right.snapshot().is_empty(),
        "r227m8-load-05: right sink must remain empty — got {:?}",
        right.snapshot()
    );

    // Emit into the other side and confirm the two remain separate.
    emit_load(&right, b"only-right\n");
    let left_tags = left.snapshot();
    let right_tags = right.snapshot();
    assert_eq!(left_tags.len(), 1, "r227m8-load-05: left still has one tag");
    assert_eq!(
        right_tags.len(),
        1,
        "r227m8-load-05: right now has its own one tag"
    );
    assert_ne!(
        left_tags[0], right_tags[0],
        "r227m8-load-05: the two scripts differ, so their tags must differ too"
    );
}
