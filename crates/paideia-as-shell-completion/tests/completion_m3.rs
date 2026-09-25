//! R228.M3 completion-ranker + fuzzy-matcher fixture corpus.
//!
//! Each test is fingerprinted `r228m3-cmp-NN` so a git-log grep against
//! the CHANGELOG-r228m3 fragment lands on the exact fixture that
//! motivated a change. The 8-fixture plan is in paideia-as issue
//! #1472.
//!
//! Score tiers exercised (see
//! [`paideia_as_shell_completion::matching`] for the formula):
//!
//! * Tier 1a (empty prefix)            — score = 1000 - candidate.len()
//! * Tier 1b (exact case-sensitive)    — score = 1000 - candidate.len()
//! * Tier 2  (case-insensitive prefix) — score =  500 - candidate.len()
//! * Tier 3  (subsequence)             — score =  100 - gap - candidate.len()
//!
//! Total order on the returned candidates: `(score desc, text asc)`.

use paideia_as_shell_completion::matching::score_match;
use paideia_as_shell_completion::{
    CandidateKind, CompletionEngine, CompletionRequest, complete,
};

// r228m3-cmp-01 -- Exact-prefix tier orders by length (shorter first)
// then alphabetically. `l` against [ls, lock]: both Tier 1b; ls scores
// 998, lock scores 996; the response lists ls before lock.
#[test]
fn r228m3_cmp_01_exact_prefix_shorter_first() {
    let engine = CompletionEngine::with_lists(
        vec!["lock".to_string(), "ls".to_string()],
        Vec::new(),
    );
    let req = CompletionRequest {
        source: "l".to_string(),
        cursor_byte: 1,
    };
    let resp = complete(&engine, &req);
    assert_eq!(resp.candidates.len(), 2, "both commands prefix-match `l`");
    assert_eq!(resp.candidates[0].text, "ls", "shorter Tier 1b ranks first");
    assert_eq!(resp.candidates[0].score, 998, "1000 - 2");
    assert_eq!(resp.candidates[1].text, "lock");
    assert_eq!(resp.candidates[1].score, 996, "1000 - 4");
}

// r228m3-cmp-02 -- Case-insensitive fallback: uppercase `L` against
// commands=[ls] surfaces one candidate with Tier 2 score 498. Under
// M1 this fixture yielded an empty response; the M1 fixture
// `r228m1-cmp-08` was rewritten in step to assert the M3 behavior.
#[test]
fn r228m3_cmp_02_case_insensitive_fallback() {
    let engine = CompletionEngine::with_lists(
        vec!["ls".to_string()],
        Vec::new(),
    );
    let req = CompletionRequest {
        source: "L".to_string(),
        cursor_byte: 1,
    };
    let resp = complete(&engine, &req);
    assert_eq!(resp.candidates.len(), 1);
    let cand = &resp.candidates[0];
    assert_eq!(cand.text, "ls");
    assert_eq!(cand.score, 498, "Tier 2: 500 - 2");
    assert_eq!(cand.kind, CandidateKind::Command);
}

// r228m3-cmp-03 -- Subsequence tier: `gt` against [git, get, growth].
// None starts_with "gt" (case-sensitive or fold), so all three fall
// through to Tier 3. Expected scores:
//   git   -> g@0 t@2 span=3 gap=1 -> 100 - 1 - 3 = 96
//   get   -> g@0 t@2 span=3 gap=1 -> 100 - 1 - 3 = 96
//   growth-> g@0 t@4 span=5 gap=3 -> 100 - 3 - 6 = 91
// Sort by (score desc, text asc) -> [get, git, growth].
#[test]
fn r228m3_cmp_03_subsequence_tier_and_tiebreak() {
    let engine = CompletionEngine::with_lists(
        vec!["git".to_string(), "get".to_string(), "growth".to_string()],
        Vec::new(),
    );
    let req = CompletionRequest {
        source: "gt".to_string(),
        cursor_byte: 2,
    };
    let resp = complete(&engine, &req);
    assert_eq!(resp.candidates.len(), 3, "all three match via subsequence");
    let ordered: Vec<(&str, i32)> = resp
        .candidates
        .iter()
        .map(|c| (c.text.as_str(), c.score))
        .collect();
    assert_eq!(
        ordered,
        vec![("get", 96), ("git", 96), ("growth", 91)],
        "expected (score desc, text asc); got {ordered:?}"
    );
}

// r228m3-cmp-04 -- Empty-prefix Tier 1a: `score_match("", "anything")`
// is `Some(1000 - 8)`. Exercises the pure fn directly (no engine).
#[test]
fn r228m3_cmp_04_empty_prefix_pure_fn() {
    assert_eq!(score_match("", "anything"), Some(992), "1000 - 8");
    assert_eq!(score_match("", ""), Some(1000));
    assert_eq!(score_match("", "x"), Some(999));
}

// r228m3-cmp-05 -- No match: prefix that cannot be a subsequence of
// candidate returns None. `score_match("xyz", "abc")` misses on every
// tier (no starts_with, no case-fold, no subsequence).
#[test]
fn r228m3_cmp_05_no_match_returns_none() {
    assert_eq!(score_match("xyz", "abc"), None);
    // Also: prefix longer than any subsequence in candidate.
    assert_eq!(score_match("abcd", "ab"), None);
}

// r228m3-cmp-06 -- Score tie broken by alphabetic text order. Empty
// source at command position with commands=[foo, bar] gives both
// candidates Tier 1a score 997 (1000 - 3); the response lists them
// alphabetically: bar before foo.
#[test]
fn r228m3_cmp_06_score_tie_alphabetic_tiebreak() {
    let engine = CompletionEngine::with_lists(
        vec!["foo".to_string(), "bar".to_string()],
        Vec::new(),
    );
    let req = CompletionRequest {
        source: String::new(),
        cursor_byte: 0,
    };
    let resp = complete(&engine, &req);
    assert_eq!(resp.candidates.len(), 2);
    assert_eq!(resp.candidates[0].text, "bar", "alphabetic tie-break: bar < foo");
    assert_eq!(resp.candidates[0].score, 997, "Tier 1a: 1000 - 3");
    assert_eq!(resp.candidates[1].text, "foo");
    assert_eq!(resp.candidates[1].score, 997);
}

// r228m3-cmp-07 -- Var candidates also flow through the ranker. In
// lambda context, prefix `x` against known_vars=[xyz, xww, foo]:
// both x-vars are Tier 1b (1000 - 3 = 997), foo doesn't match at all
// (no `x` in subsequence). Tie between xww and xyz broken by
// alphabetic text order: xww before xyz.
#[test]
fn r228m3_cmp_07_var_ranker_orders_alphabetically_on_tie() {
    let engine = CompletionEngine::with_lists(
        Vec::new(),
        vec!["xyz".to_string(), "xww".to_string(), "foo".to_string()],
    );
    // `{ |a| x` -- cursor at end of `x` ident inside lambda body.
    // Byte offsets: `{`=0 ` `=1 `|`=2 `a`=3 `|`=4 ` `=5 `x`=6, len=7.
    let req = CompletionRequest {
        source: "{ |a| x".to_string(),
        cursor_byte: 7,
    };
    let resp = complete(&engine, &req);
    assert_eq!(resp.candidates.len(), 2, "only the two x-vars match");
    assert_eq!(resp.candidates[0].text, "xww", "alphabetic tie-break: xww < xyz");
    assert_eq!(resp.candidates[0].kind, CandidateKind::Var);
    assert_eq!(resp.candidates[0].score, 997);
    assert_eq!(resp.candidates[1].text, "xyz");
    assert_eq!(resp.candidates[1].score, 997);
}

// r228m3-cmp-08 -- Score field is populated non-negative for every
// candidate a realistic engine surfaces. Empty prefix + short
// commands sit safely in Tier 1a (997/996); subsequence hits on
// short candidates land in Tier 3 near 100. Regression guard against
// a future ranker change that accidentally emits a negative score
// for a common case.
#[test]
fn r228m3_cmp_08_scores_are_non_negative_for_realistic_engine() {
    let engine = CompletionEngine::with_lists(
        vec![
            "ls".to_string(),
            "cat".to_string(),
            "grep".to_string(),
            "growth".to_string(),
        ],
        Vec::new(),
    );
    // Empty source -> command position -> Tier 1a for every command.
    let resp_empty = complete(
        &engine,
        &CompletionRequest {
            source: String::new(),
            cursor_byte: 0,
        },
    );
    for cand in &resp_empty.candidates {
        assert!(
            cand.score >= 0,
            "Tier 1a score must be non-negative: {} -> {}",
            cand.text,
            cand.score,
        );
    }

    // Subsequence prefix `gh` against the command catalogue: `growth`
    // matches with g@0 h@5 span=6 gap=4 -> 100 - 4 - 6 = 90.
    let resp_sub = complete(
        &engine,
        &CompletionRequest {
            source: "gh".to_string(),
            cursor_byte: 2,
        },
    );
    for cand in &resp_sub.candidates {
        assert!(
            cand.score >= 0,
            "Tier 3 score must stay non-negative for short candidates: \
             {} -> {}",
            cand.text,
            cand.score,
        );
    }
}
