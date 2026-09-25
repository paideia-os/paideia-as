//! R226.M11 — per-query fingerprint emission.
//! R226.M10 — per-iteration progress emission (see the second half of
//! this module).
//!
//! Every [`crate::eval::Evaluator`] query path that reaches a completed
//! result (successful `run_query`, `run_query_via_magic_sets`,
//! `run_stratified`, `run_aggregate_query`, and their `_with_session`
//! variants) emits one fingerprint of the shape
//! `dlg.<hex-query-id>.<result-count>` into a [`FingerprintSink`] the
//! caller injected at construction time. Failure paths (bound-term
//! rejection, unstratifiable negation, aggregation errors) emit
//! nothing — a fingerprint is proof that a query *completed*, not
//! merely that it started.
//!
//! The R226.M10 progress channel is a **separate** stream on the same
//! [`crate::eval::Evaluator`]: while a fingerprint is one tag per
//! *completed* query, a progress tick is one event per *seminaïve
//! fixpoint iteration* — a stream a REPL can render as a "3 iterations,
//! Δ=42 tuples, total=1_284" progress line without waiting for the
//! query to finish. The two sinks are wired independently
//! ([`crate::eval::Evaluator::with_fingerprint_sink`] and
//! [`crate::eval::Evaluator::with_progress_sink`]) so a caller can
//! attach one without the other. Their defaults are both no-ops so
//! neither M10 nor M11 changes observable behaviour when a caller does
//! not opt in.
//!
//! # Why a trait and not a concrete channel
//!
//! The Datalog evaluator is one of several sub-engines that will
//! eventually feed the R220.M10 `@fingerprint` correlator (the parser,
//! the type-checker, and the pipeline runner already do). The
//! correlator's ingestion side is not the concern of this crate; the
//! obligation here is to *emit* — the concrete sink at the far end
//! ships tags to whatever channel the outer shell has wired up
//! (stderr, a ring buffer, a socket, a `Vec<String>` for a debugger's
//! attention).
//!
//! # Default is silent
//!
//! [`NullSink`] is the default so pre-M11 call sites see no observable
//! behaviour change — a bare `Evaluator::new()` still runs a query
//! without side effects. A caller who wants the emissions must
//! explicitly wrap a sink via
//! [`crate::eval::Evaluator::with_fingerprint_sink`].
//!
//! # Format
//!
//! `format!("dlg.{:016x}.{}", id.0, result_count)`:
//!
//! * `dlg.` prefix — namespace so the correlator can route the tag to
//!   the Datalog engine without a lookup table.
//! * `{:016x}` — 16-digit zero-padded lowercase hex of the query id.
//!   Fixed width so a `grep` over correlator logs matches every id
//!   with one regex.
//! * `{}` — decimal result count. `Vec<Binding>::len()` for
//!   `run_query`/`run_query_via_magic_sets`/`run_query_with_session`;
//!   `HashMap` group-count for the aggregate paths;
//!   [`crate::eval::Database::total_tuple_count`] for `run_stratified`
//!   (which materialises a DB rather than a substitution set — the
//!   fingerprint still needs a single scalar summary).

use std::sync::Mutex;

/// Ingest point for a completed query's fingerprint.
///
/// Implementations must be `Send + Sync` because
/// [`crate::eval::Evaluator`] stores the sink behind a
/// `Box<dyn FingerprintSink + Send + Sync>` — a shell embeds one
/// evaluator behind an `Arc` and shares it across worker threads,
/// so the sink itself has to be thread-safe.
///
/// Emission is unconditional from the evaluator's perspective: the
/// evaluator does not distinguish between "the sink is a no-op" and
/// "the sink is a real channel" — that decision belongs to the sink.
pub trait FingerprintSink {
    /// Consume one fingerprint tag. Must not panic — the evaluator
    /// calls this on a hot success path and cannot afford to unwind
    /// mid-completion.
    fn emit(&self, tag: &str);
}

/// Discarding sink — the default. Every call is a no-op.
///
/// Used when the caller does not want to observe fingerprints; keeps
/// the M11 emission code path uniform (no `Option<...>` guard in the
/// evaluator) at the cost of one virtual dispatch per completed query,
/// which is a rounding error against a fixpoint iteration.
#[derive(Clone, Copy, Debug, Default)]
pub struct NullSink;

impl FingerprintSink for NullSink {
    fn emit(&self, _tag: &str) {
        // Deliberate no-op.
    }
}

/// Test-only sink that accumulates every emitted tag under a
/// [`Mutex`]. Kept in the crate proper (not behind `#[cfg(test)]`) so
/// downstream integration tests — which live in `tests/` and cannot
/// see `#[cfg(test)]`-gated items — can use it.
#[derive(Debug, Default)]
pub struct CollectingSink {
    collected: Mutex<Vec<String>>,
}

impl CollectingSink {
    /// Construct an empty sink.
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot every emitted tag in insertion order. Clones the
    /// underlying buffer — cheap for test corpora (≤ tens of tags per
    /// fixture).
    ///
    /// Recovers from a poisoned mutex by draining the poisoned guard;
    /// the sink is single-purpose and a poisoning implies a panicking
    /// concurrent emitter, which is the fault the test is trying to
    /// diagnose — surfacing the tags collected before the panic is
    /// more useful than re-panicking here.
    pub fn tags(&self) -> Vec<String> {
        match self.collected.lock() {
            Ok(guard) => guard.clone(),
            Err(poison) => poison.into_inner().clone(),
        }
    }

    /// Number of tags collected so far. Convenience over `tags().len()`
    /// that avoids the clone.
    pub fn len(&self) -> usize {
        match self.collected.lock() {
            Ok(guard) => guard.len(),
            Err(poison) => poison.into_inner().len(),
        }
    }

    /// True iff no tag has been emitted.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl FingerprintSink for CollectingSink {
    fn emit(&self, tag: &str) {
        // Mirror `tags()`'s poisoning recovery — a poisoned guard
        // means a concurrent panic, but this thread can still record
        // its tag without contributing a second panic.
        match self.collected.lock() {
            Ok(mut guard) => guard.push(tag.to_owned()),
            Err(poison) => poison.into_inner().push(tag.to_owned()),
        }
    }
}

/// A per-evaluator monotonic query id. Newtype so a caller cannot mix
/// it with an unrelated `u64` counter — the atomic under the hood
/// belongs to the [`crate::eval::Evaluator`] that minted this id, and
/// the type prevents an unrelated `u64` (a tuple count, an iteration
/// index) from being formatted into a fingerprint by accident.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QueryId(
    /// Raw monotonic counter value. Public so a caller correlating
    /// fingerprints with out-of-band trace records can read the id
    /// without an accessor; construct via
    /// [`crate::eval::Evaluator::next_query_id`], not by hand.
    pub u64,
);

impl QueryId {
    /// Format this id into the canonical fingerprint tag body — the
    /// `dlg.<hex>.<count>` shape both success paths use.
    ///
    /// Kept in one place so a future format change (say, a wider hex
    /// field or a different prefix) touches a single site rather than
    /// six emission call sites in `eval.rs`.
    pub(crate) fn format_tag(self, result_count: usize) -> String {
        format!("dlg.{:016x}.{}", self.0, result_count)
    }
}

// ====================================================================
// R226.M10 — per-iteration progress emission
// ====================================================================
//
// A separate sink from `FingerprintSink` on purpose: progress ticks fire
// once per fixpoint iteration (a bulk-hot stream a REPL renders as a
// live counter), while fingerprints fire once per completed query (a
// low-rate summary the correlator attributes results against). Two
// consumers, two rates, two lifecycles — sharing one trait would force
// every progress consumer to filter out the completion tag and vice
// versa, and any future extension (per-tick timestamps, backpressure,
// cancel tokens) belongs on one channel without disturbing the other.
//
// The evaluator emits ticks from the seminaïve fixpoint loop *after
// each iteration merges its delta*, so `total_tuples` is the DB size the
// caller would observe if the loop stopped right there. The iteration
// counter is 1-based and monotonic within one fixpoint invocation; a
// multi-stratum program restarts the count per stratum (a stratum's
// counter reflects the work that stratum did, which is the scalar a
// caller diagnosing a slow stratum wants). A tick is fired even for the
// terminating iteration where `delta_tuples == 0`, so the caller always
// sees at least one tick whenever a stratum has any prior-round tuples
// to pivot on — including the facts-only case where there are no rules
// (the seed tick then reads `(1, 0, |facts|)`, i.e. "one iteration
// added nothing beyond the EDB you already gave me").

/// Ingest point for one seminaïve-fixpoint-iteration progress tick.
///
/// Implementations must be `Send + Sync` for the same reason
/// [`FingerprintSink`] is — the [`crate::eval::Evaluator`] stores the
/// sink behind a `Box<dyn ProgressSink + Send + Sync>` and is itself
/// shared across worker threads via an `Arc` in the wider shell.
///
/// # Arguments
///
/// * `iteration` — 1-based iteration index within the current fixpoint
///   invocation. Multi-stratum programs restart the count per stratum;
///   within one stratum the values are strictly monotonic (`1, 2, 3, …`).
/// * `delta_tuples` — number of *new* tuples the iteration produced
///   after filtering against the existing DB. Zero on the terminating
///   iteration (the tick that told the fixpoint to stop).
/// * `total_tuples` — [`crate::eval::Database::total_tuple_count`] taken
///   *after* the iteration's delta was merged. The value the caller
///   would observe if the fixpoint halted at this tick.
pub trait ProgressSink {
    /// Consume one progress tick. Must not panic — the evaluator calls
    /// this on the seminaïve inner loop and cannot afford to unwind
    /// mid-fixpoint.
    fn tick(&self, iteration: usize, delta_tuples: usize, total_tuples: usize);
}

/// Discarding progress sink — the default. Every tick is a no-op.
///
/// Kept as the default so pre-M10 call sites see no observable
/// behaviour change; the M10 emission code path stays uniform (no
/// `Option<...>` guard in the evaluator's fixpoint loop) at the cost of
/// one virtual dispatch per iteration, which is a rounding error
/// against the tuple-derivation work an iteration performs.
#[derive(Clone, Copy, Debug, Default)]
pub struct NullProgressSink;

impl ProgressSink for NullProgressSink {
    fn tick(&self, _iteration: usize, _delta_tuples: usize, _total_tuples: usize) {
        // Deliberate no-op.
    }
}

/// Test-only progress sink that accumulates every tick as an
/// `(iteration, delta_tuples, total_tuples)` triple under a [`Mutex`].
/// Kept in the crate proper (not behind `#[cfg(test)]`) so downstream
/// integration tests — which live in `tests/` and cannot see
/// `#[cfg(test)]`-gated items — can use it.
#[derive(Debug, Default)]
pub struct CollectingProgressSink {
    collected: Mutex<Vec<(usize, usize, usize)>>,
}

impl CollectingProgressSink {
    /// Construct an empty sink.
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot every emitted tick in insertion order. Clones the
    /// underlying buffer — cheap for test corpora.
    ///
    /// Recovers from a poisoned mutex by draining the poisoned guard,
    /// matching the pattern of [`CollectingSink::tags`]: a poisoning
    /// implies a panicking concurrent emitter, and surfacing the ticks
    /// collected before the panic is more useful than re-panicking here.
    pub fn ticks(&self) -> Vec<(usize, usize, usize)> {
        match self.collected.lock() {
            Ok(guard) => guard.clone(),
            Err(poison) => poison.into_inner().clone(),
        }
    }

    /// Number of ticks collected so far. Convenience over `ticks().len()`
    /// that avoids the clone.
    pub fn len(&self) -> usize {
        match self.collected.lock() {
            Ok(guard) => guard.len(),
            Err(poison) => poison.into_inner().len(),
        }
    }

    /// True iff no tick has been emitted.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl ProgressSink for CollectingProgressSink {
    fn tick(&self, iteration: usize, delta_tuples: usize, total_tuples: usize) {
        // Mirror `ticks()`'s poisoning recovery — a poisoned guard
        // means a concurrent panic, but this thread can still record
        // its tick without contributing a second panic.
        match self.collected.lock() {
            Ok(mut guard) => guard.push((iteration, delta_tuples, total_tuples)),
            Err(poison) => {
                poison
                    .into_inner()
                    .push((iteration, delta_tuples, total_tuples))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_sink_swallows_emission() {
        let sink = NullSink;
        sink.emit("dlg.0000000000000000.0");
        // No accessor exists — the sink is stateless by contract; the
        // absence of a panic is the entire assertion.
    }

    #[test]
    fn collecting_sink_records_in_order() {
        let sink = CollectingSink::new();
        sink.emit("dlg.0000000000000000.3");
        sink.emit("dlg.0000000000000001.5");
        assert_eq!(
            sink.tags(),
            vec![
                "dlg.0000000000000000.3".to_owned(),
                "dlg.0000000000000001.5".to_owned(),
            ]
        );
        assert_eq!(sink.len(), 2);
        assert!(!sink.is_empty());
    }

    #[test]
    fn collecting_sink_starts_empty() {
        let sink = CollectingSink::new();
        assert!(sink.is_empty());
        assert_eq!(sink.len(), 0);
        assert!(sink.tags().is_empty());
    }

    #[test]
    fn query_id_format_tag_zero_padded_16_hex() {
        let tag = QueryId(0).format_tag(3);
        assert_eq!(tag, "dlg.0000000000000000.3");
        let tag = QueryId(0xff).format_tag(42);
        assert_eq!(tag, "dlg.00000000000000ff.42");
        let tag = QueryId(u64::MAX).format_tag(0);
        assert_eq!(tag, "dlg.ffffffffffffffff.0");
    }

    // -- R226.M10 -----------------------------------------------------

    #[test]
    fn null_progress_sink_swallows_tick() {
        let sink = NullProgressSink;
        sink.tick(1, 42, 100);
        // Stateless by contract; the absence of a panic is the entire
        // assertion.
    }

    #[test]
    fn collecting_progress_sink_records_in_order() {
        let sink = CollectingProgressSink::new();
        sink.tick(1, 3, 3);
        sink.tick(2, 5, 8);
        sink.tick(3, 0, 8);
        assert_eq!(
            sink.ticks(),
            vec![(1, 3, 3), (2, 5, 8), (3, 0, 8)],
        );
        assert_eq!(sink.len(), 3);
        assert!(!sink.is_empty());
    }

    #[test]
    fn collecting_progress_sink_starts_empty() {
        let sink = CollectingProgressSink::new();
        assert!(sink.is_empty());
        assert_eq!(sink.len(), 0);
        assert!(sink.ticks().is_empty());
    }
}
