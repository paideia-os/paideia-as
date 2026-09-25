//! R226.M10 — per-iteration progress emission fixture corpus.
//!
//! Six fixtures (`r226m10-prog-01` .. `r226m10-prog-06`) pinning the
//! [`crate::progress::ProgressSink`] contract wired through the
//! seminaïve fixpoint driver. Each panic message carries its
//! fingerprint so the R220.M10 `@fingerprint` correlator can attribute
//! a regression to a single fixture without re-parsing the test name.
//!
//! # What each fixture pins
//!
//! * 01 — Facts-only program (no rules) emits exactly one seed tick
//!   `(1, 0, |facts|)`. Documents the "one iteration added nothing
//!   beyond the EDB you already gave me" contract from the
//!   `progress` module doc.
//! * 02 — 4-hop transitive-closure chain emits ≥4 ticks (the fixpoint
//!   walks the chain one hop per iteration).
//! * 03 — Empty program (no rules, no facts) emits **0** ticks: our
//!   choice per the acceptance note. Rationale — a stratum with no
//!   prior-round tuples has no work to report on, so reporting a
//!   spurious `(1, 0, 0)` tick would mislead a REPL's "iterations run"
//!   counter. Matches the "either 0 or 1 is acceptable" spec.
//! * 04 — Two independently-built evaluators keep independent
//!   collecting sinks; a query on one leaves the other's tick log
//!   empty.
//! * 05 — Progress + fingerprint sinks both fire on one query
//!   (attach both, `run_query` once, expect exactly one fingerprint
//!   tag AND ≥1 progress tick).
//! * 06 — Tick iteration counts are strictly monotonic within a
//!   single-stratum query (`iter[k+1] > iter[k]`).

mod common;

use common::dlg_tokens;
use paideia_as_shell_datalog::{
    parse_block, parse_query, CollectingProgressSink, CollectingSink, Evaluator,
    FingerprintSink, ProgressSink,
};
use std::sync::Arc;

// --------------------------------------------------------------------
// Local newtype wrappers — the orphan rule forbids implementing the
// crate's own traits for `Arc<...>`, so per-test wrappers let the
// tests hand a `Box<dyn ProgressSink + Send + Sync>` (or the
// fingerprint variant) to the evaluator while still holding a live
// `Arc<...>` handle for post-run inspection. Mirrors the pattern in
// `query_fingerprint.rs`.
// --------------------------------------------------------------------

struct ArcProgress(Arc<CollectingProgressSink>);

impl ProgressSink for ArcProgress {
    fn tick(&self, iteration: usize, delta_tuples: usize, total_tuples: usize) {
        self.0.tick(iteration, delta_tuples, total_tuples);
    }
}

struct ArcFingerprint(Arc<CollectingSink>);

impl FingerprintSink for ArcFingerprint {
    fn emit(&self, tag: &str) {
        self.0.emit(tag);
    }
}

/// Build an evaluator whose progress ticks land in a fresh
/// [`CollectingProgressSink`]. Returns both so the test can drive the
/// evaluator and inspect the sink at the end.
fn ev_with_progress_sink() -> (Evaluator, Arc<CollectingProgressSink>) {
    let sink = Arc::new(CollectingProgressSink::new());
    let boxed: Box<dyn ProgressSink + Send + Sync> = Box::new(ArcProgress(Arc::clone(&sink)));
    let ev = Evaluator::new().with_progress_sink(boxed);
    (ev, sink)
}

// ====================================================================
// 01 — facts-only program emits exactly one seed tick (1, 0, |facts|)
// ====================================================================

#[test]
fn r226m10_prog_01_facts_only_emits_single_seed_tick() {
    let fp = "r226m10-prog-01";
    let tokens = dlg_tokens("p(a). p(b). p(c).");
    let program = parse_block(&tokens).unwrap_or_else(|e| panic!("{fp}: parse: {e:?}"));
    let query = parse_query(&dlg_tokens("p(?x)"))
        .unwrap_or_else(|e| panic!("{fp}: query parse: {e:?}"));
    let (ev, sink) = ev_with_progress_sink();

    let bindings = ev
        .run_query(&program, &query)
        .unwrap_or_else(|e| panic!("{fp}: run_query: {e:?}"));
    assert_eq!(bindings.len(), 3, "{fp}: expected 3 substitutions");

    let ticks = sink.ticks();
    assert_eq!(
        ticks.len(),
        1,
        "{fp}: expected exactly 1 seed tick, got {ticks:?}",
    );
    let (iter, delta, total) = ticks[0];
    assert_eq!(iter, 1, "{fp}: seed tick iteration must be 1, got {iter}");
    assert_eq!(delta, 0, "{fp}: seed tick delta must be 0, got {delta}");
    assert_eq!(total, 3, "{fp}: seed tick total must be |facts|=3, got {total}");
}

// ====================================================================
// 02 — 4-hop transitive-closure chain emits ≥4 ticks
// ====================================================================

#[test]
fn r226m10_prog_02_four_hop_chain_emits_at_least_four_ticks() {
    let fp = "r226m10-prog-02";
    // Chain: 1 → 2 → 3 → 4 → 5. Four edges, four hops. Transitive
    // closure fires once per hop, so the fixpoint takes ≥4 productive
    // rounds plus one terminating round.
    let src = "\
        edge(a, b). edge(b, c). edge(c, d). edge(d, e). \
        path(?X, ?Y) => edge(?X, ?Y). \
        path(?X, ?Z) => edge(?X, ?Y), path(?Y, ?Z).\
    ";
    let tokens = dlg_tokens(src);
    let program = parse_block(&tokens).unwrap_or_else(|e| panic!("{fp}: parse: {e:?}"));
    let query = parse_query(&dlg_tokens("path(?x, ?y)"))
        .unwrap_or_else(|e| panic!("{fp}: query parse: {e:?}"));
    let (ev, sink) = ev_with_progress_sink();

    let _bindings = ev
        .run_query(&program, &query)
        .unwrap_or_else(|e| panic!("{fp}: run_query: {e:?}"));

    let ticks = sink.ticks();
    assert!(
        ticks.len() >= 4,
        "{fp}: expected at least 4 progress ticks for a 4-hop chain, got {}: {ticks:?}",
        ticks.len(),
    );
}

// ====================================================================
// 03 — empty program (no rules, no facts) emits 0 ticks
// ====================================================================

#[test]
fn r226m10_prog_03_empty_program_emits_zero_ticks() {
    let fp = "r226m10-prog-03";
    // Genuinely empty program — parse an empty token stream. The
    // parser accepts an empty program (0 facts + 0 rules).
    let tokens = dlg_tokens("");
    let program = parse_block(&tokens).unwrap_or_else(|e| panic!("{fp}: parse: {e:?}"));
    // A query over a predicate that does not exist — safe on an empty
    // DB; returns 0 bindings.
    let query = parse_query(&dlg_tokens("p(?x)"))
        .unwrap_or_else(|e| panic!("{fp}: query parse: {e:?}"));
    let (ev, sink) = ev_with_progress_sink();

    let bindings = ev
        .run_query(&program, &query)
        .unwrap_or_else(|e| panic!("{fp}: run_query: {e:?}"));
    assert_eq!(bindings.len(), 0, "{fp}: expected 0 substitutions on empty DB");

    let ticks = sink.ticks();
    // Design choice (documented in module doc): a stratum with no
    // prior-round tuples has no work to report on, so the empty-empty
    // case emits nothing rather than a spurious (1, 0, 0).
    assert_eq!(
        ticks.len(),
        0,
        "{fp}: expected 0 ticks for a genuinely empty program, got {ticks:?}",
    );
}

// ====================================================================
// 04 — two evaluators keep independent collecting sinks
// ====================================================================

#[test]
fn r226m10_prog_04_two_evaluators_have_independent_sinks() {
    let fp = "r226m10-prog-04";
    let tokens = dlg_tokens("p(a). p(b).");
    let program = parse_block(&tokens).unwrap_or_else(|e| panic!("{fp}: parse: {e:?}"));
    let query = parse_query(&dlg_tokens("p(?x)"))
        .unwrap_or_else(|e| panic!("{fp}: query parse: {e:?}"));

    let (ev_a, sink_a) = ev_with_progress_sink();
    let (_ev_b, sink_b) = ev_with_progress_sink();

    // Drive only evaluator A. B must remain untouched.
    ev_a.run_query(&program, &query)
        .unwrap_or_else(|e| panic!("{fp}: A run_query: {e:?}"));

    assert!(
        !sink_a.is_empty(),
        "{fp}: A's sink should have recorded at least one tick",
    );
    assert!(
        sink_b.is_empty(),
        "{fp}: B's sink must be empty (no cross-talk), got {:?}",
        sink_b.ticks(),
    );
}

// ====================================================================
// 05 — progress + fingerprint sinks both fire on one query
// ====================================================================

#[test]
fn r226m10_prog_05_progress_and_fingerprint_both_fire() {
    let fp = "r226m10-prog-05";
    let tokens = dlg_tokens("p(a). p(b). p(c).");
    let program = parse_block(&tokens).unwrap_or_else(|e| panic!("{fp}: parse: {e:?}"));
    let query = parse_query(&dlg_tokens("p(?x)"))
        .unwrap_or_else(|e| panic!("{fp}: query parse: {e:?}"));

    let progress = Arc::new(CollectingProgressSink::new());
    let fingerprint = Arc::new(CollectingSink::new());
    let ev = Evaluator::new()
        .with_progress_sink(Box::new(ArcProgress(Arc::clone(&progress))))
        .with_fingerprint_sink(Box::new(ArcFingerprint(Arc::clone(&fingerprint))));

    let bindings = ev
        .run_query(&program, &query)
        .unwrap_or_else(|e| panic!("{fp}: run_query: {e:?}"));
    assert_eq!(bindings.len(), 3, "{fp}: expected 3 substitutions");

    // Fingerprint side: exactly one tag on this single-query path.
    let tags = fingerprint.tags();
    assert_eq!(
        tags.len(),
        1,
        "{fp}: expected exactly 1 fingerprint tag, got {tags:?}",
    );

    // Progress side: at least one tick (a facts-only program yields
    // the seed tick).
    let ticks = progress.ticks();
    assert!(
        !ticks.is_empty(),
        "{fp}: expected at least 1 progress tick, got none",
    );
}

// ====================================================================
// 06 — tick iteration counts are strictly monotonic (single stratum)
// ====================================================================

#[test]
fn r226m10_prog_06_iteration_counts_monotonic_within_stratum() {
    let fp = "r226m10-prog-06";
    // Single-stratum program (positive-only Datalog, so all rules
    // collapse to stratum 0). A longer chain than fixture 02 so we
    // get multiple ticks to compare against each other.
    let src = "\
        edge(a, b). edge(b, c). edge(c, d). edge(d, e). edge(e, f). edge(f, g). \
        path(?X, ?Y) => edge(?X, ?Y). \
        path(?X, ?Z) => edge(?X, ?Y), path(?Y, ?Z).\
    ";
    let tokens = dlg_tokens(src);
    let program = parse_block(&tokens).unwrap_or_else(|e| panic!("{fp}: parse: {e:?}"));
    let query = parse_query(&dlg_tokens("path(?x, ?y)"))
        .unwrap_or_else(|e| panic!("{fp}: query parse: {e:?}"));
    let (ev, sink) = ev_with_progress_sink();

    ev.run_query(&program, &query)
        .unwrap_or_else(|e| panic!("{fp}: run_query: {e:?}"));

    let ticks = sink.ticks();
    assert!(
        ticks.len() >= 2,
        "{fp}: need at least 2 ticks to verify monotonicity, got {}",
        ticks.len(),
    );
    // Every subsequent tick's iteration index must strictly exceed
    // its predecessor's. This is a per-stratum invariant; the fixture
    // deliberately uses a single-stratum program so the assertion
    // holds across every consecutive pair.
    for pair in ticks.windows(2) {
        let (prev_iter, _, _) = pair[0];
        let (next_iter, _, _) = pair[1];
        assert!(
            next_iter > prev_iter,
            "{fp}: expected iteration counts strictly monotonic, saw {prev_iter} then {next_iter} in {ticks:?}",
        );
    }
    // The first iteration is 1-based.
    assert_eq!(
        ticks[0].0, 1,
        "{fp}: first tick's iteration must be 1, got {}",
        ticks[0].0,
    );
}
