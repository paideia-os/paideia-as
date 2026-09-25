//! R229.M9 — bounded turn-history ring buffer.
//!
//! # Role
//!
//! A REPL that lasts more than a few seconds needs an ordered record of
//! what has been evaluated: for a replay harness (deterministic
//! re-execution of a recorded transcript), for a diagnostics layer that
//! wants to correlate an error at turn N with the source that produced
//! it, and — R228.M4's next step — for a completer that ranks
//! suggestions by recency. `TurnHistory` is the single owning
//! container: a `VecDeque<ReplTurn>` with a fixed maximum length, into
//! which every completed [`crate::ReplTurn`] is pushed by
//! [`crate::eval_turn`].
//!
//! # Why a ring buffer (not `Vec`)
//!
//! A pure `Vec` grows without bound. A REPL session that spans hours of
//! ad-hoc data-exploration turns can easily push tens of thousands of
//! entries — the same session's per-turn source may be a few KB of
//! datalog block, and the retained `ReplTurn` also holds every stage's
//! rendered output. Left unbounded, the driver's resident set balloons
//! and the "look back N turns" completers we're building over this
//! substrate scan work that no user cares about. A `VecDeque` with a
//! capacity cap gives us amortised `O(1)` push and `O(1)` eviction of
//! the oldest entry (`pop_front`) while preserving the "most recent
//! at the back" invariant that both the replay harness and the M4
//! recency completer read against.
//!
//! # Insertion order and indexing
//!
//! `record` always pushes to the back, then trims the front until
//! `len() <= capacity()`. `get(i)` returns entry `i` counting from the
//! oldest still in the buffer (index 0), and `latest()` returns entry
//! `len() - 1` — i.e. the same entry as `get(len()-1)`. Callers that
//! record a fingerprint (`repl.turn.NNNN`) at capture time and want to
//! find the entry later should treat the index as a *position within
//! the buffer's live window*, not as the pre-eviction turn counter —
//! the turn counter is monotone in [`crate::ReplState`], but a bounded
//! history necessarily loses the lowest indices as it fills.
//!
//! # Capacity contract
//!
//! `capacity` is fixed at construction and never grows. `TurnHistory::new(0)`
//! is legal — it evicts on every record and never retains any entry.
//! The [`Default`] impl chooses `1000`, matching the R229 milestone doc's
//! "an interactive session's working set" heuristic; a driver that
//! wants a bigger or smaller window constructs a `TurnHistory::new(N)`
//! directly and writes it into `state.history` before the first turn.

use std::collections::VecDeque;

use crate::ReplTurn;

/// Bounded FIFO ring buffer of [`ReplTurn`] records.
///
/// See the module doc for the ring-buffer rationale, the insertion-order
/// / indexing contract, and the capacity-fixed-at-construction rule.
#[derive(Clone, Debug)]
pub struct TurnHistory {
    /// The retained entries, oldest at the front, most recent at the
    /// back. `VecDeque` (over `Vec`) gives `O(1)` `pop_front` on
    /// eviction; the "oldest at index 0" convention lets `get(i)` map
    /// cleanly onto the deque's own random-access `get`.
    entries: VecDeque<ReplTurn>,
    /// Fixed maximum retained length. `record` evicts from the front
    /// until `entries.len() <= capacity`.
    capacity: usize,
}

impl TurnHistory {
    /// Fresh buffer with the given maximum retained length.
    ///
    /// A capacity of `0` is legal: every `record` call evicts the entry
    /// it just pushed, so `len()` stays at `0` forever. The
    /// [`VecDeque::with_capacity`] backing storage is a *pre-allocation*
    /// hint — `record` may still grow it up to `capacity` entries
    /// before eviction stabilises the length.
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// Push `turn` onto the back of the buffer, then evict from the
    /// front until `entries.len() <= capacity`.
    ///
    /// A `while` loop (not a single `if`) is used because a capacity
    /// smaller than 1 leaves the buffer over budget after the initial
    /// push; the loop terminates in a single iteration for any non-zero
    /// capacity but stays correct for the capacity-0 edge case.
    pub fn record(&mut self, turn: ReplTurn) {
        self.entries.push_back(turn);
        while self.entries.len() > self.capacity {
            self.entries.pop_front();
        }
    }

    /// Entry at position `index` within the buffer's live window
    /// (0 = oldest still retained). Returns `None` if `index >= len()`.
    pub fn get(&self, index: usize) -> Option<&ReplTurn> {
        self.entries.get(index)
    }

    /// Most recent entry (back of the deque), or `None` if the buffer
    /// is empty. Equivalent to `self.get(self.len() - 1)` when non-empty.
    pub fn latest(&self) -> Option<&ReplTurn> {
        self.entries.back()
    }

    /// Number of retained entries. Always `<= capacity()`.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` iff no turns have been recorded (or all have been
    /// evicted, which for a non-zero-capacity buffer requires the
    /// initial state).
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The fixed maximum retained length.
    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

impl Default for TurnHistory {
    /// Default capacity of `1000` — the R229.M9 milestone doc's
    /// "interactive session working set" heuristic. A driver that
    /// wants a different window replaces `state.history` with a
    /// `TurnHistory::new(N)` before the first turn.
    fn default() -> Self {
        Self::new(1000)
    }
}
