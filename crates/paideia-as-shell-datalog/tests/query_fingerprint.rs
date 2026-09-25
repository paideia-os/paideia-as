//! R226.M11 — per-query fingerprint emission fixture corpus.
//!
//! Eight fixtures (`r226m11-fp-01` .. `r226m11-fp-08`) covering every
//! `Evaluator` query entry point plus the counter's monotonicity and
//! per-evaluator isolation properties. Each panic message carries its
//! fingerprint so the R220.M10 `@fingerprint` correlator can attribute
//! a regression to a single fixture without re-parsing the test name.
//!
//! # What each fixture pins
//!
//! * 01 — `run_query`: three-fact program emits exactly one tag whose
//!   shape is `dlg.<16-hex>.<n>`. First id is `0` (zero-padded), so
//!   the tag is `dlg.0000000000000000.3`.
//! * 02 — `run_query_via_magic_sets`: one tag of the same shape (the
//!   magic-set path and the naïve path both mint an id and emit).
//! * 03 — `run_stratified`: one tag; `run_stratified` materialises a
//!   database, so the tag's tail is the total-tuple-count of the DB
//!   rather than a substitution count — the fixture checks shape, not
//!   the exact scalar, so a future rewrite that changes what counts
//!   as "one tuple" does not break this contract.
//! * 04 — `run_aggregate_query`: a `count group by ?y` query with
//!   two groups → the tag's tail is `.2`. Aggregate paths report
//!   group cardinality, not substitution count.
//! * 05 — `run_query_with_session`: overlay path emits its own
//!   fingerprint.
//! * 06 — Two sequential queries on the same evaluator → ids `0`
//!   and `1` (monotonic, dense).
//! * 07 — Two evaluators built independently → both counters start
//!   at `0`; the state is per-evaluator.
//! * 08 — Error path (unstratified program → `run_stratified` errors
//!   → NO fingerprint emitted). Proves the emission is gated on
//!   successful completion.

mod common;

use common::dlg_tokens;
use paideia_as_shell_datalog::{
    parse_aggregate_query, parse_block, parse_query, AggregateQuery, CollectingSink, Evaluator,
    Program, Query, SessionEdb, Value,
};

// --------------------------------------------------------------------
// Shared helpers — small, local, and self-contained. This corpus has
// no need to reach into `tests/common/mod.rs` beyond the `dlg_tokens`
// tokeniser.
// --------------------------------------------------------------------

fn build_program(fp: &str, src: &str) -> Program {
    let tokens = dlg_tokens(src);
    parse_block(&tokens).unwrap_or_else(|e| panic!("{fp}: parse failed: {e:?}"))
}

fn build_query(fp: &str, src: &str) -> Query {
    let tokens = dlg_tokens(src);
    parse_query(&tokens).unwrap_or_else(|e| panic!("{fp}: query parse failed: {e:?}"))
}

fn build_agg_query(fp: &str, src: &str) -> AggregateQuery {
    let tokens = dlg_tokens(src);
    parse_aggregate_query(&tokens)
        .unwrap_or_else(|e| panic!("{fp}: aggregate query parse failed: {e:?}"))
}

fn ident(s: &str) -> Value {
    Value::Ident(s.to_owned())
}

// --------------------------------------------------------------------
// Test-only newtype wrapper: `FingerprintSink` for
// `Arc<CollectingSink>` is orphan-rule-forbidden (both the trait and
// `CollectingSink` live in this crate but `Arc` does not), so a local
// newtype is what lets a test hand a `Box<dyn FingerprintSink + Send
// + Sync>` to the evaluator while still holding a live `Arc<...>`
// handle for post-run inspection.
// --------------------------------------------------------------------

struct ArcSink(std::sync::Arc<CollectingSink>);

impl paideia_as_shell_datalog::FingerprintSink for ArcSink {
    fn emit(&self, tag: &str) {
        self.0.emit(tag)
    }
}

/// Build an evaluator whose fingerprints land in a fresh
/// [`CollectingSink`]. Returns both so the test can drive the
/// evaluator and inspect the sink at the end without threading the
/// sink through a shared pointer.
fn ev_with_arc_sink() -> (Evaluator, std::sync::Arc<CollectingSink>) {
    let sink = std::sync::Arc::new(CollectingSink::new());
    let boxed: Box<dyn paideia_as_shell_datalog::FingerprintSink + Send + Sync> =
        Box::new(ArcSink(std::sync::Arc::clone(&sink)));
    let ev = Evaluator::new().with_fingerprint_sink(boxed);
    (ev, sink)
}

/// Assert `tag` matches the canonical shape `dlg.<16 lowercase hex>.<decimal>`.
/// Any deviation panics with `fp` in the message.
fn assert_tag_shape(fp: &str, tag: &str) {
    let (prefix, rest) = tag.split_once('.').unwrap_or_else(|| {
        panic!("{fp}: tag missing first '.' separator: {tag:?}")
    });
    assert_eq!(prefix, "dlg", "{fp}: tag prefix must be 'dlg', got {prefix:?}");
    let (hex, tail) = rest.split_once('.').unwrap_or_else(|| {
        panic!("{fp}: tag missing second '.' separator: {tag:?}")
    });
    assert_eq!(hex.len(), 16, "{fp}: hex id must be 16 chars, got {hex:?}");
    assert!(
        hex.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
        "{fp}: hex id must be lowercase ASCII hex, got {hex:?}",
    );
    assert!(
        tail.chars().all(|c| c.is_ascii_digit()),
        "{fp}: tail must be ASCII decimal, got {tail:?}",
    );
}

// ====================================================================
// 01 — run_query on a 3-fact program emits exactly `dlg.0.3`
// ====================================================================

#[test]
fn r226m11_fp_01_run_query_emits_one_tag_with_correct_count() {
    let fp = "r226m11-fp-01";
    let program = build_program(fp, "p(a). p(b). p(c).");
    let query = build_query(fp, "p(?x)");
    let (ev, sink) = ev_with_arc_sink();

    let bindings = ev
        .run_query(&program, &query)
        .unwrap_or_else(|e| panic!("{fp}: run_query failed: {e:?}"));
    assert_eq!(bindings.len(), 3, "{fp}: expected 3 substitutions");

    let tags = sink.tags();
    assert_eq!(
        tags.len(),
        1,
        "{fp}: expected exactly one fingerprint, got {tags:?}"
    );
    // First query on a fresh evaluator: id = 0, count = 3.
    assert_eq!(
        tags[0], "dlg.0000000000000000.3",
        "{fp}: fingerprint tag mismatch",
    );
    assert_tag_shape(fp, &tags[0]);
}

// ====================================================================
// 02 — run_query_via_magic_sets emits one tag of the canonical shape
// ====================================================================

#[test]
fn r226m11_fp_02_magic_sets_emits_one_tag_with_correct_shape() {
    let fp = "r226m11-fp-02";
    // Trivial one-rule program the magic-set rewriter handles without
    // fuss; the exact result count is not what this fixture pins —
    // shape is.
    let program = build_program(
        fp,
        "edge(a, b). edge(b, c). edge(c, d).\n\
         reach(?x, ?y) => edge(?x, ?y).\n\
         reach(?x, ?z) => edge(?x, ?y), reach(?y, ?z).",
    );
    let query = build_query(fp, "reach(a, ?z)");
    let (ev, sink) = ev_with_arc_sink();

    let bindings = ev
        .run_query_via_magic_sets(&program, &query)
        .unwrap_or_else(|e| panic!("{fp}: run_query_via_magic_sets failed: {e:?}"));

    let tags = sink.tags();
    assert_eq!(
        tags.len(),
        1,
        "{fp}: expected exactly one fingerprint, got {tags:?}"
    );
    assert_tag_shape(fp, &tags[0]);
    // First query on a fresh evaluator: id must be 0 (zero-padded).
    assert!(
        tags[0].starts_with("dlg.0000000000000000."),
        "{fp}: first fingerprint must carry id=0, got {tag:?}",
        tag = tags[0]
    );
    // The tail must equal the returned binding count — the magic-set
    // path is answer-preserving, so its fingerprint agrees with the
    // substitution count.
    let want_tail = format!(".{}", bindings.len());
    assert!(
        tags[0].ends_with(&want_tail),
        "{fp}: tag {t:?} should end with {want_tail:?}",
        t = tags[0]
    );
}

// ====================================================================
// 03 — run_stratified emits one tag (shape check; scalar is total
//      tuple count of the materialised DB, deliberately not pinned)
// ====================================================================

#[test]
fn r226m11_fp_03_run_stratified_emits_one_tag() {
    let fp = "r226m11-fp-03";
    // Simple positive-only program the stratifier collapses to a
    // single stratum. `run_stratified` still runs the full pipeline
    // and must fingerprint on completion.
    let program = build_program(
        fp,
        "p(1). p(2). q(?x) => p(?x).",
    );
    let (ev, sink) = ev_with_arc_sink();

    let db = ev
        .run_stratified(&program)
        .unwrap_or_else(|e| panic!("{fp}: run_stratified failed: {e:?}"));

    let tags = sink.tags();
    assert_eq!(
        tags.len(),
        1,
        "{fp}: expected exactly one fingerprint, got {tags:?}"
    );
    assert_tag_shape(fp, &tags[0]);
    // The scalar tail is the total tuple count — 2 p/1 facts + 2
    // derived q/1 tuples = 4. Pinning it here documents the
    // convention; a future change to the scalar summary would flag
    // this fixture as intentional attention rather than a silent shift.
    let want = format!("dlg.0000000000000000.{}", db.total_tuple_count());
    assert_eq!(tags[0], want, "{fp}: fingerprint tag mismatch");
}

// ====================================================================
// 04 — run_aggregate_query on `count group by ?y` with 2 groups →
//      the fingerprint tail is `.2` (aggregate paths report group
//      cardinality, not substitution count)
// ====================================================================

#[test]
fn r226m11_fp_04_aggregate_group_count_in_tail() {
    let fp = "r226m11-fp-04";
    // Two distinct groups: g1 has 2 members, g2 has 1. Count is per-
    // group, and the returned map has one entry per group → tail = 2.
    let program = build_program(
        fp,
        "obs(g1, a). obs(g1, b). obs(g2, c).",
    );
    let query = build_agg_query(fp, "count(?x) group by ?g where obs(?g, ?x)");
    let (ev, sink) = ev_with_arc_sink();

    let groups = ev
        .run_aggregate_query(&program, &query)
        .unwrap_or_else(|e| panic!("{fp}: run_aggregate_query failed: {e:?}"));
    assert_eq!(groups.len(), 2, "{fp}: expected 2 groups");

    let tags = sink.tags();
    assert_eq!(
        tags.len(),
        1,
        "{fp}: expected exactly one fingerprint, got {tags:?}"
    );
    assert_tag_shape(fp, &tags[0]);
    assert_eq!(
        tags[0], "dlg.0000000000000000.2",
        "{fp}: aggregate tag must carry group count in the tail",
    );
}

// ====================================================================
// 05 — run_query_with_session emits one fingerprint
// ====================================================================

#[test]
fn r226m11_fp_05_session_query_emits_one_tag() {
    let fp = "r226m11-fp-05";
    // Empty program so every returned binding traces back to the
    // session overlay — pins the overlay path on its own.
    let program = Program::empty();
    let query = build_query(fp, "color(?c)");
    let mut session = SessionEdb::new();
    session.assert("color", vec![ident("red")]);
    session.assert("color", vec![ident("blue")]);

    let (ev, sink) = ev_with_arc_sink();
    let bindings = ev
        .run_query_with_session(&program, &query, &session)
        .unwrap_or_else(|e| panic!("{fp}: run_query_with_session failed: {e:?}"));
    assert_eq!(bindings.len(), 2, "{fp}: expected 2 substitutions");

    let tags = sink.tags();
    assert_eq!(
        tags.len(),
        1,
        "{fp}: expected exactly one fingerprint, got {tags:?}"
    );
    assert_tag_shape(fp, &tags[0]);
    assert_eq!(
        tags[0], "dlg.0000000000000000.2",
        "{fp}: session-query tag must reflect substitution count",
    );
}

// ====================================================================
// 06 — Two sequential queries on the same evaluator → ids 0 and 1
// ====================================================================

#[test]
fn r226m11_fp_06_sequential_ids_are_monotone() {
    let fp = "r226m11-fp-06";
    let program = build_program(fp, "p(a). p(b).");
    let query = build_query(fp, "p(?x)");
    let (ev, sink) = ev_with_arc_sink();

    let _ = ev
        .run_query(&program, &query)
        .unwrap_or_else(|e| panic!("{fp}: first query failed: {e:?}"));
    let _ = ev
        .run_query(&program, &query)
        .unwrap_or_else(|e| panic!("{fp}: second query failed: {e:?}"));

    let tags = sink.tags();
    assert_eq!(
        tags.len(),
        2,
        "{fp}: expected two fingerprints, got {tags:?}"
    );
    assert_eq!(
        tags[0], "dlg.0000000000000000.2",
        "{fp}: first tag must carry id=0",
    );
    assert_eq!(
        tags[1], "dlg.0000000000000001.2",
        "{fp}: second tag must carry id=1 (monotone by one)",
    );
}

// ====================================================================
// 07 — Two independent evaluators → both counters start at 0
// ====================================================================

#[test]
fn r226m11_fp_07_independent_evaluators_have_independent_counters() {
    let fp = "r226m11-fp-07";
    let program = build_program(fp, "p(a).");
    let query = build_query(fp, "p(?x)");

    let (ev_a, sink_a) = ev_with_arc_sink();
    let (ev_b, sink_b) = ev_with_arc_sink();

    // Drive one query per evaluator — the counters must be
    // independent, so both must emit id=0.
    let _ = ev_a
        .run_query(&program, &query)
        .unwrap_or_else(|e| panic!("{fp}: ev_a query failed: {e:?}"));
    let _ = ev_b
        .run_query(&program, &query)
        .unwrap_or_else(|e| panic!("{fp}: ev_b query failed: {e:?}"));

    let tags_a = sink_a.tags();
    let tags_b = sink_b.tags();
    assert_eq!(tags_a.len(), 1, "{fp}: ev_a should have one tag");
    assert_eq!(tags_b.len(), 1, "{fp}: ev_b should have one tag");
    assert_eq!(
        tags_a[0], "dlg.0000000000000000.1",
        "{fp}: ev_a's first id must be 0",
    );
    assert_eq!(
        tags_b[0], "dlg.0000000000000000.1",
        "{fp}: ev_b's first id must also be 0 — counters are per-evaluator",
    );

    // For belt-and-braces: `next_query_id` on a fresh third evaluator
    // must yield 0 as its first value, then 1.
    let ev_c = Evaluator::new();
    assert_eq!(
        ev_c.next_query_id(),
        paideia_as_shell_datalog::QueryId(0),
        "{fp}: fresh evaluator must issue QueryId(0) first",
    );
    assert_eq!(
        ev_c.next_query_id(),
        paideia_as_shell_datalog::QueryId(1),
        "{fp}: second call must yield QueryId(1) (monotone by one)",
    );
}

// ====================================================================
// 08 — Error path emits NO fingerprint
// ====================================================================

#[test]
fn r226m11_fp_08_error_path_emits_no_fingerprint() {
    let fp = "r226m11-fp-08";
    // Negation through recursion → the stratifier rejects the program
    // before any fixpoint iteration; `run_stratified` must therefore
    // return an error AND emit no fingerprint (fingerprints are
    // completion tokens, not attempt tokens).
    let program = build_program(
        fp,
        "p(?x) => q(?x), not r(?x).\n\
         r(?x) => p(?x).\n\
         q(a). q(b).",
    );
    let (ev, sink) = ev_with_arc_sink();

    let err = ev
        .run_stratified(&program)
        .expect_err(&format!("{fp}: unstratified program must be rejected"));
    // Sanity: we hit the negation-through-recursion path (rather than
    // some parser mishap that also happens to error).
    match err {
        paideia_as_shell_datalog::EvalError::UnstratifiedNegation { .. } => {}
        other => panic!("{fp}: expected UnstratifiedNegation, got {other:?}"),
    }

    // The core assertion: the sink saw nothing.
    let tags = sink.tags();
    assert!(
        tags.is_empty(),
        "{fp}: error path must not emit any fingerprint, got {tags:?}",
    );
    // And the counter did not tick — the id-after-success discipline
    // is what makes the correlator's "N successful queries" tally
    // match the "N fingerprints" it observes.
    let next = ev.next_query_id();
    assert_eq!(
        next,
        paideia_as_shell_datalog::QueryId(0),
        "{fp}: counter must not have advanced past an error",
    );
}
