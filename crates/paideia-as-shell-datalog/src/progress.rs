//! R226.M10 — per-iteration progress emission for the seminaïve
//! fixpoint driver.
//!
//! Companion channel to [`crate::fingerprint`]: while a fingerprint
//! fires **once per completed query** as a summary tag the R220.M10
//! `@fingerprint` correlator attributes results against, a progress
//! tick fires **once per seminaïve fixpoint iteration** — a bulk-hot
//! stream a REPL renders as a live "3 iterations, Δ=42 tuples,
//! total=1_284" counter without waiting for the query to finish.
//!
//! # Why a separate module (and a separate trait) from `fingerprint`
//!
//! Two consumers, two emission rates, two lifecycles:
//!
//! * A fingerprint is one tag per completed query — low volume, arrives
//!   with a completion count in it.
//! * A progress tick is one event per fixpoint iteration — high volume,
//!   fires *during* evaluation before any completion count exists.
//!
//! Sharing one trait would force every progress consumer to filter out
//! the completion tag and vice versa, and any future channel-specific
//! extension (per-tick timestamps, cooperative-cancel tokens,
//! backpressure) belongs on one channel without disturbing the other.
//! The two sinks are wired independently on the same
//! [`crate::eval::Evaluator`] via
//! [`crate::eval::Evaluator::with_fingerprint_sink`] and
//! [`crate::eval::Evaluator::with_progress_sink`] so a caller can
//! attach one without the other.
//!
//! # Default is silent
//!
//! [`NullProgressSink`] is the default so pre-M10 call sites see no
//! observable behaviour change — a bare `Evaluator::new()` still runs
//! a query without side effects. A caller who wants the emissions must
//! explicitly wrap a sink via
//! [`crate::eval::Evaluator::with_progress_sink`]. Keeping the sink
//! virtual-dispatched at all times means the fixpoint loop needs no
//! `Option<...>` guard — one indirect call per iteration is a rounding
//! error against the tuple-derivation work an iteration performs.
//!
//! # Semantics of one tick
//!
//! The evaluator emits ticks from the seminaïve fixpoint loop *after
//! each iteration merges its delta into the database*, so
//! `total_tuples` is the DB size the caller would observe if the loop
//! stopped right there. The iteration counter is 1-based and monotonic
//! within one fixpoint invocation; a multi-stratum program restarts
//! the count per stratum (a stratum's counter reflects the work that
//! stratum did, which is the scalar a caller diagnosing a slow stratum
//! wants). A tick is fired even for the terminating iteration where
//! `delta_tuples == 0`, so the caller always sees at least one tick
//! whenever a stratum has any prior-round tuples to pivot on —
//! including the facts-only case where there are no rules (the seed
//! tick then reads `(1, 0, |facts|)`, i.e. "one iteration added
//! nothing beyond the EDB you already gave me"). A stratum that runs
//! against an empty DB (empty program: no facts, no rules) emits
//! nothing — there is no work to report on.

use std::sync::Mutex;

/// Ingest point for one seminaïve-fixpoint-iteration progress tick.
///
/// Implementations must be `Send + Sync` because
/// [`crate::eval::Evaluator`] stores the sink behind a
/// `Box<dyn ProgressSink + Send + Sync>` — a shell embeds one
/// evaluator behind an `Arc` and shares it across worker threads,
/// so the sink itself has to be thread-safe.
///
/// # Arguments
///
/// * `iteration` — 1-based iteration index within the current fixpoint
///   invocation. Multi-stratum programs restart the count per stratum;
///   within one stratum the values are strictly monotonic (`1, 2, 3, …`).
/// * `delta_tuples` — number of *new* tuples the iteration produced
///   after filtering against the existing DB. Zero on the terminating
///   iteration (the tick that told the fixpoint to stop).
/// * `total_tuples` — [`crate::eval::Database::total_tuple_count`]
///   taken *after* the iteration's delta was merged. The value the
///   caller would observe if the fixpoint halted at this tick.
pub trait ProgressSink {
    /// Consume one progress tick. Must not panic — the evaluator calls
    /// this on the seminaïve inner loop and cannot afford to unwind
    /// mid-fixpoint.
    fn tick(&self, iteration: usize, delta_tuples: usize, total_tuples: usize);
}

/// Discarding progress sink — the default. Every tick is a no-op.
///
/// Used when the caller does not want to observe progress; keeps the
/// M10 emission code path uniform (no `Option<...>` guard in the
/// fixpoint loop) at the cost of one virtual dispatch per iteration.
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
    /// underlying buffer — cheap for test corpora (tens of ticks per
    /// fixture at most).
    ///
    /// Recovers from a poisoned mutex by draining the poisoned guard;
    /// a poisoning implies a panicking concurrent emitter, and
    /// surfacing the ticks collected before the panic is more useful
    /// than re-panicking here.
    pub fn ticks(&self) -> Vec<(usize, usize, usize)> {
        match self.collected.lock() {
            Ok(guard) => guard.clone(),
            Err(poison) => poison.into_inner().clone(),
        }
    }

    /// Number of ticks collected so far. Convenience over
    /// `ticks().len()` that avoids the clone.
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
            Err(poison) => poison
                .into_inner()
                .push((iteration, delta_tuples, total_tuples)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(sink.ticks(), vec![(1, 3, 3), (2, 5, 8), (3, 0, 8)]);
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
