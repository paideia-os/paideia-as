//! R228.M4 recency-aware ranking + usage-history buffer -- fixture
//! corpus.
//!
//! Each test is fingerprinted `r228m4-cmp-NN` so a git-log grep against
//! the CHANGELOG-r228m4 fragment lands on the exact fixture that
//! motivated a change. The 8-fixture plan is in paideia-as issue #1474.
//!
//! Recency boost formula (see
//! [`paideia_as_shell_completion::history::UsageHistory::recency_boost`]):
//!
//! * Position 0 (most recent)   -> +2000
//! * Position 1                  -> +1900
//! * Position N                  -> +2000 - 100 * N
//! * Not in history              ->  0
//!
//! The boost is added to the M3 tier score before the
//! `(score desc, text asc)` sort at the response boundary.

use paideia_as_shell_completion::history::UsageHistory;
use paideia_as_shell_completion::{
    CompletionEngine, CompletionRequest, complete, record_selection,
};

// r228m4-cmp-01 -- Empty history: prefix "l" against [ls, less]
// preserves the M3 ordering (both Tier 1b; ls scores 998, less scores
// 996). Regression guard that a zero boost does not perturb rank.
#[test]
fn r228m4_cmp_01_empty_history_matches_m3_order() {
    let engine = CompletionEngine::with_lists(
        vec!["ls".to_string(), "less".to_string()],
        Vec::new(),
    );
    let req = CompletionRequest {
        source: "l".to_string(),
        cursor_byte: 1,
    };
    let resp = complete(&engine, &req);
    assert_eq!(resp.candidates.len(), 2);
    assert_eq!(resp.candidates[0].text, "ls", "shorter Tier 1b still first");
    assert_eq!(resp.candidates[0].score, 998, "1000 - 2 (no boost)");
    assert_eq!(resp.candidates[1].text, "less");
    assert_eq!(resp.candidates[1].score, 996, "1000 - 4 (no boost)");
}

// r228m4-cmp-02 -- After `record_selection("ls")`, prefix "l" boosts
// ls by 2000 (position 0). less has no history entry and stays at
// its M3 score. Response ordering: ls first with 998 + 2000 = 2998.
#[test]
fn r228m4_cmp_02_single_record_boosts_by_2000() {
    let mut engine = CompletionEngine::with_lists(
        vec!["ls".to_string(), "less".to_string()],
        Vec::new(),
    );
    record_selection(&mut engine, "ls");
    let req = CompletionRequest {
        source: "l".to_string(),
        cursor_byte: 1,
    };
    let resp = complete(&engine, &req);
    assert_eq!(resp.candidates.len(), 2);
    assert_eq!(resp.candidates[0].text, "ls");
    assert_eq!(
        resp.candidates[0].score, 2998,
        "998 (Tier 1b for `ls`) + 2000 (recency position 0)"
    );
    assert_eq!(resp.candidates[1].text, "less");
    assert_eq!(
        resp.candidates[1].score, 996,
        "996 (Tier 1b for `less`) + 0 (not in history)"
    );
}

// r228m4-cmp-03 -- Two selections in order: `record("less")` then
// `record("ls")`. History becomes [ls, less] (front = most recent).
// Both candidates get a boost but ls's is higher:
//   ls   -> 998 (base) + 2000 (pos 0) = 2998
//   less -> 996 (base) + 1900 (pos 1) = 2896
// Response lists ls first.
#[test]
fn r228m4_cmp_03_two_records_position_taper() {
    let mut engine = CompletionEngine::with_lists(
        vec!["ls".to_string(), "less".to_string()],
        Vec::new(),
    );
    record_selection(&mut engine, "less");
    record_selection(&mut engine, "ls");
    let req = CompletionRequest {
        source: "l".to_string(),
        cursor_byte: 1,
    };
    let resp = complete(&engine, &req);
    assert_eq!(resp.candidates.len(), 2);
    assert_eq!(resp.candidates[0].text, "ls");
    assert_eq!(
        resp.candidates[0].score, 2998,
        "998 + 2000 (pos 0)",
    );
    assert_eq!(resp.candidates[1].text, "less");
    assert_eq!(
        resp.candidates[1].score, 2896,
        "996 + 1900 (pos 1)",
    );
}

// r228m4-cmp-04 -- Bounded capacity: with capacity 3, recording 4
// distinct items evicts the oldest. `len()` reports 3; the first
// recorded (`a`) is gone; the three most-recent survive.
#[test]
fn r228m4_cmp_04_capacity_eviction() {
    let mut h = UsageHistory::new(3);
    h.record("a");
    h.record("b");
    h.record("c");
    h.record("d");
    assert_eq!(h.len(), 3, "capacity 3 must trim to 3 entries");
    assert!(!h.contains("a"), "oldest `a` should be evicted");
    assert!(h.contains("b"));
    assert!(h.contains("c"));
    assert!(h.contains("d"));
}

// r228m4-cmp-05 -- Duplicate record: `record("a"); record("b");
// record("a")` yields history len 2 (not 3), with `a` at position 0
// and `b` at position 1. The prior `a` was removed before the fresh
// push to the front.
#[test]
fn r228m4_cmp_05_duplicate_record_promotes_no_dup() {
    let mut h = UsageHistory::new(8);
    h.record("a");
    h.record("b");
    h.record("a");
    assert_eq!(h.len(), 2, "duplicate must be de-duped, not appended");
    assert_eq!(h.recency_boost("a"), 2000, "a is now most recent");
    assert_eq!(h.recency_boost("b"), 1900, "b slid to position 1");
}

// r228m4-cmp-06 -- Unknown text: `recency_boost("unknown_text")` on a
// fresh (or any) history returns 0. Regression guard against a
// signed-integer confusion or an off-by-one in the `position` lookup.
#[test]
fn r228m4_cmp_06_unknown_text_boost_is_zero() {
    let h = UsageHistory::default();
    assert_eq!(h.recency_boost("unknown_text"), 0);

    let mut h2 = UsageHistory::new(4);
    h2.record("a");
    h2.record("b");
    assert_eq!(h2.recency_boost("c"), 0, "absent even after records");
}

// r228m4-cmp-07 -- End-to-end: after `record_selection("ls")`, the
// candidate returned by `complete` carries a score equal to its base
// M3 score plus 2000. Cross-checks the ranker's boost math against
// the underlying `UsageHistory` primitive without any assumption on
// the sort position (the assertion is on the returned score).
#[test]
fn r228m4_cmp_07_record_then_complete_score_math() {
    let mut engine = CompletionEngine::with_lists(
        vec!["ls".to_string()],
        Vec::new(),
    );
    record_selection(&mut engine, "ls");
    let req = CompletionRequest {
        source: "l".to_string(),
        cursor_byte: 1,
    };
    let resp = complete(&engine, &req);
    assert_eq!(resp.candidates.len(), 1);
    let cand = &resp.candidates[0];
    assert_eq!(cand.text, "ls");
    // Tier 1b for `ls`: 1000 - 2 = 998; position 0 boost: 2000.
    let base = 998;
    let boost = 2000;
    assert_eq!(
        cand.score,
        base + boost,
        "expected boosted score = M3 base ({base}) + recency ({boost})",
    );
}

// r228m4-cmp-08 -- `record_selection` free-fn surface: calling it
// deposits into `engine.history` such that `history.contains(text)`
// reports true. Also verifies the engine's `history` field is
// pub-accessible from outside the crate (M4 wire-up requirement).
#[test]
fn r228m4_cmp_08_record_selection_surface_fn() {
    let mut engine = CompletionEngine::empty();
    assert!(engine.history.is_empty(), "fresh engine has empty history");
    record_selection(&mut engine, "foo");
    assert!(
        engine.history.contains("foo"),
        "after record_selection, history should contain the text",
    );
    assert_eq!(engine.history.len(), 1);
    assert_eq!(engine.history.recency_boost("foo"), 2000);
}
