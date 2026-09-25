//! R228.M3 — completion ranking primitive.
//!
//! [`score_match`] is a pure function returning `Some(i32)` when `prefix`
//! matches `candidate` under one of four tiers, or `None` when there is
//! no match at all. Higher scores rank first; the [`crate::complete`]
//! caller uses `(score desc, text asc)` as the total order.
//!
//! # Score tiers
//!
//! From strongest to weakest match strength (with shorter candidates
//! ranked higher within each tier so the popup surfaces the tightest
//! completion first):
//!
//! | Tier                          | Score formula                                    |
//! |-------------------------------|--------------------------------------------------|
//! | Empty prefix (match-all)      | `1000 - candidate.len()`                         |
//! | Exact case-sensitive prefix   | `1000 - candidate.len()`                         |
//! | Case-insensitive prefix       | `500  - candidate.len()`                         |
//! | Subsequence (in order, gaps)  | `100 - gap_count - candidate.len()`              |
//!
//! Empty-prefix and exact-prefix share the top tier because an empty
//! prefix (typed TAB at command position) is semantically the same as
//! "prefix matches trivially" — the popup should show every command
//! ordered by length only.
//!
//! # Subsequence semantics
//!
//! A subsequence match walks `candidate` greedily left-to-right,
//! consuming one prefix character per hit; the match succeeds iff every
//! prefix character is consumed. `gap_count` is the number of candidate
//! characters that fall **between** the first and last matched
//! positions but were not themselves consumed:
//!
//! ```text
//! score_match("gt", "growth") -> first g@0, last t@4
//!                                span = last - first + 1 = 5
//!                                gap_count = span - prefix.len()
//!                                          = 5   - 2           = 3
//!                                score = 100 - 3 - 6           = 91
//! ```
//!
//! Greedy left-to-right (rather than the globally-minimal-gap match)
//! is deliberate: it is O(|candidate|) with no backtracking, matches
//! the reader's mental model ("first hit wins"), and cannot flip which
//! candidates match vs. don't — only the exact `gap_count` differs
//! from the minimal-gap variant, and both agree when the prefix
//! characters are distinct (the M3 corpus). A more sophisticated
//! variant (Sublime-style skip penalties, camelCase word-boundary
//! bonuses) is R228.M4+ territory.
//!
//! # Character-vs-byte accounting
//!
//! All positions and lengths are **character** counts (`chars().count()`,
//! `.enumerate()` on `chars()`), not byte offsets. This matters for
//! non-ASCII candidates: `"é"` (single composed code point) has
//! `char_count == 1` but `byte_len == 2`, so a byte-based length would
//! double-penalize accented commands. The M1 layer NFC-normalizes the
//! prefix before this module ever sees it, so composed vs. decomposed
//! spellings agree by the time score_match runs.

/// Score tier bases -- named so the caller (and the CHANGELOG grep) can
/// reason about tier boundaries without decoding magic numbers.
const EXACT_PREFIX_BASE: i32 = 1000;
const CASE_INSENSITIVE_BASE: i32 = 500;
const SUBSEQUENCE_BASE: i32 = 100;

/// Rank one `(prefix, candidate)` pair.
///
/// Returns `Some(score)` when the pair matches under one of the four
/// tiers documented at the module level, or `None` when no tier fires.
/// See the module docs for the score formula per tier.
pub fn score_match(prefix: &str, candidate: &str) -> Option<i32> {
    let cand_char_len = candidate.chars().count() as i32;

    // Tier 1a -- empty prefix matches everything trivially. Return
    // before the exact-prefix branch so an empty prefix does not go
    // through an unnecessary `starts_with("")` (which is always true
    // but reads less clearly than an explicit branch).
    if prefix.is_empty() {
        return Some(EXACT_PREFIX_BASE - cand_char_len);
    }

    // Tier 1b -- exact ASCII case-sensitive prefix.
    if candidate.starts_with(prefix) {
        return Some(EXACT_PREFIX_BASE - cand_char_len);
    }

    // Tier 2 -- case-insensitive prefix. The `to_lowercase()` call
    // allocates; that's fine here because the caller runs one
    // score_match per catalogue entry per keystroke (bounded, small).
    // A future M4 pass may cache lowercased candidates in the engine
    // if profiling shows this in a hot path.
    let lowered_prefix = prefix.to_lowercase();
    let lowered_candidate = candidate.to_lowercase();
    if lowered_candidate.starts_with(&lowered_prefix) {
        return Some(CASE_INSENSITIVE_BASE - cand_char_len);
    }

    // Tier 3 -- subsequence. Walk candidate greedily, consuming one
    // prefix character per hit. Match succeeds iff every prefix
    // character was consumed.
    let mut prefix_iter = prefix.chars();
    let mut wanted = prefix_iter.next();
    let mut first_match: Option<usize> = None;
    let mut last_match: usize = 0;
    for (pos, ch) in candidate.chars().enumerate() {
        if let Some(want) = wanted {
            if ch == want {
                if first_match.is_none() {
                    first_match = Some(pos);
                }
                last_match = pos;
                wanted = prefix_iter.next();
            }
        }
    }
    if wanted.is_some() {
        // Prefix has characters we never consumed -> no match.
        return None;
    }
    // Prefix is empty here only when it was empty on entry, which we
    // already short-circuited; so `first_match` is always `Some` at
    // this point. Fall back safely for future refactors.
    let first = first_match.unwrap_or(0);
    let prefix_char_len = prefix.chars().count();
    // Span between first and last matched positions, inclusive.
    // For a 1-char prefix, first == last, span == 1, gap == 0.
    let span = last_match - first + 1;
    let gap_count = span.saturating_sub(prefix_char_len) as i32;
    Some(SUBSEQUENCE_BASE - gap_count - cand_char_len)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_prefix_matches_any_with_length_only_score() {
        assert_eq!(score_match("", "anything"), Some(1000 - 8));
        assert_eq!(score_match("", ""), Some(1000));
        assert_eq!(score_match("", "x"), Some(1000 - 1));
    }

    #[test]
    fn exact_prefix_beats_case_insensitive() {
        // "ls" starts_with "l" -> Tier 1b (1000 - 2 = 998).
        assert_eq!(score_match("l", "ls"), Some(998));
        // "ls" case-insensitive starts_with "L" -> Tier 2 (500 - 2 = 498).
        assert_eq!(score_match("L", "ls"), Some(498));
    }

    #[test]
    fn subsequence_score_reflects_gap_and_length() {
        // "gt" in "git": g@0, t@2 -> span 3, gap 1, score 100 - 1 - 3 = 96.
        assert_eq!(score_match("gt", "git"), Some(96));
        // "gt" in "growth": g@0, t@4 -> span 5, gap 3, score 100 - 3 - 6 = 91.
        assert_eq!(score_match("gt", "growth"), Some(91));
    }

    #[test]
    fn no_match_returns_none() {
        assert_eq!(score_match("xyz", "abc"), None);
        assert_eq!(score_match("abc", "ab"), None); // ran out of candidate.
    }
}
