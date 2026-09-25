//! R226.M11 — per-query fingerprint emission.
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
}
