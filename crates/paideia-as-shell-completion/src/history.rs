//! R228.M4 — recency-aware usage history buffer.
//!
//! [`UsageHistory`] is a bounded MRU (most-recently-used) log of
//! completion selections. The ranker consults it via
//! [`UsageHistory::recency_boost`] to lift previously-selected candidates
//! above equal-scored but unseen alternatives, so a user who habitually
//! reaches for `ls` sees it float to the top of any prefix `l` match
//! without a re-teaching phase.
//!
//! # Structure
//!
//! Entries are stored in a `VecDeque<String>` with position 0 = most
//! recent. [`UsageHistory::record`] enforces MRU semantics: an existing
//! duplicate is removed before the fresh entry is pushed to the front,
//! so `record("a"); record("b"); record("a")` yields `[a, b]`, not
//! `[a, b, a]`. Once `entries.len() == capacity`, the oldest tail entry
//! is dropped on the next `record` call.
//!
//! # Boost formula
//!
//! `recency_boost(text)` returns:
//!
//! * `2000 - position * 100` when `text` is found at `position`
//!   (position 0 = most-recent).
//! * `0` when `text` is not in the buffer.
//!
//! Position 0 gets a boost of 2000, which is enough to outrank a Tier 1a
//! candidate (max base score `1000`); a position-19 entry still contributes
//! `100`. Positions beyond 20 contribute non-positive boosts, at which
//! point the entry has "aged out" of ranker relevance — a deliberate
//! taper, not a defect. Clamping to 0 would erase the tie-break signal
//! among old entries; the raw formula preserves it, and the sorter still
//! orders monotonically.
//!
//! Kept as `i32` (matching [`crate::Candidate::score`]) so callers can
//! add the boost to the base score without a cast.
//!
//! # Capacity
//!
//! The default capacity of 64 is a lever, not a limit: R229 wire-up may
//! choose a different cap via [`crate::CompletionEngine::with_history_capacity`].
//! 64 is enough to cover a typical REPL session's working-set of
//! commands, records and identifiers without the buffer becoming a
//! liability under memory pressure (each entry is a short `String`).

use std::collections::VecDeque;

/// The MRU buffer of previously-selected completion texts.
///
/// Owned as a field of [`crate::CompletionEngine`]; see the module docs
/// for the recency-boost formula.
#[derive(Clone, Debug)]
pub struct UsageHistory {
    /// The buffer itself. Front = most-recent; back = oldest. Kept as
    /// `VecDeque` (not `Vec`) so both push_front and pop_back are O(1).
    entries: VecDeque<String>,
    /// The maximum entry count. `record` trims from the back once the
    /// buffer reaches this size. Zero-capacity is a legal but degenerate
    /// configuration: every `record` immediately drops the just-pushed
    /// entry, so the buffer stays empty and every `recency_boost` call
    /// returns 0.
    capacity: usize,
}

impl UsageHistory {
    /// Construct an empty buffer with the given capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// Record a selection.
    ///
    /// If `text` is already in the buffer, its prior position is
    /// removed before the fresh copy is pushed to the front — the
    /// buffer never carries duplicates. Once the buffer reaches
    /// capacity, the oldest entry is evicted from the back.
    pub fn record(&mut self, text: impl Into<String>) {
        let text = text.into();
        // Remove any prior occurrence so re-selection promotes rather
        // than duplicates. Manual linear scan is fine at capacity 64;
        // profiling can revisit if this ever becomes hot.
        if let Some(pos) = self.entries.iter().position(|e| e == &text) {
            self.entries.remove(pos);
        }
        self.entries.push_front(text);
        // Trim from the tail. Using `while` (not `if`) so a shrink-in-
        // place after a `with_history_capacity` swap eventually
        // converges even if the previous cap held more entries.
        while self.entries.len() > self.capacity {
            self.entries.pop_back();
        }
    }

    /// Boost score for `text`.
    ///
    /// Returns `2000 - position * 100` when `text` is at `position`
    /// (position 0 = most-recent), or `0` when it is absent. See the
    /// module docs for the taper rationale.
    pub fn recency_boost(&self, text: &str) -> i32 {
        match self.entries.iter().position(|e| e == text) {
            Some(pos) => 2000 - (pos as i32) * 100,
            None => 0,
        }
    }

    /// Current entry count.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Whether `text` is currently in the buffer.
    pub fn contains(&self, text: &str) -> bool {
        self.entries.iter().any(|e| e == text)
    }
}

impl Default for UsageHistory {
    /// Default capacity of 64 — see the module docs for the rationale.
    fn default() -> Self {
        Self::new(64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_promotes_existing_entry_to_front() {
        let mut h = UsageHistory::new(8);
        h.record("a");
        h.record("b");
        h.record("a");
        assert_eq!(h.len(), 2, "duplicate should be de-duped, not appended");
        assert_eq!(h.recency_boost("a"), 2000, "a is now most recent");
        assert_eq!(h.recency_boost("b"), 1900, "b slid to position 1");
    }

    #[test]
    fn record_trims_to_capacity() {
        let mut h = UsageHistory::new(3);
        h.record("a");
        h.record("b");
        h.record("c");
        h.record("d");
        assert_eq!(h.len(), 3, "oldest should be evicted");
        assert!(!h.contains("a"), "eldest entry `a` must be gone");
        assert!(h.contains("d"));
        assert!(h.contains("c"));
        assert!(h.contains("b"));
    }

    #[test]
    fn recency_boost_missing_is_zero() {
        let h = UsageHistory::default();
        assert_eq!(h.recency_boost("nope"), 0);
    }
}
