//! R228.M6 path completion (filesystem walker for `/path/<cursor>`) --
//! fixture corpus.
//!
//! Each test is fingerprinted `r228m6-cmp-NN` so a git-log grep against
//! the CHANGELOG-r228m6 fragment lands on the exact fixture that
//! motivated a change. The 8-fixture plan is in paideia-as issue #1480.
//!
//! Fixtures cover the four path-in-progress shapes the completion
//! engine's `try_path_completion` helper recognises:
//!
//! * `/^`                    -- root, empty name prefix.
//! * `/b^`                   -- root, partial name prefix `b`.
//! * `/usr/^`                -- nested dir `/usr`, empty name prefix.
//! * `/usr/lo^`              -- nested dir `/usr`, partial prefix `lo`.
//!
//! plus the three invariants: missing dir yields zero candidates but
//! still claims the request; every emitted candidate carries
//! [`CandidateKind::Path`]; scores agree with the M3 ranker tiers; and
//! a non-path source falls through to the M1..M5 branches.

use paideia_as_shell_completion::{
    CandidateKind, CompletionEngine, CompletionRequest, PathProvider, complete,
};

/// Build an engine whose path provider seeds the root with three
/// classic FHS entries and `/usr` with two children. Used by fixtures
/// 01, 02, 03, 04, 06, 07 so shape assertions are independent of
/// catalogue setup drift.
fn engine_with_fhs_paths() -> CompletionEngine {
    let mut provider = PathProvider::new();
    provider.insert(
        "/",
        vec!["bin".to_string(), "usr".to_string(), "etc".to_string()],
    );
    provider.insert(
        "/usr",
        vec!["local".to_string(), "share".to_string()],
    );
    CompletionEngine::empty().with_path_provider(provider)
}

// r228m6-cmp-01 -- Root directory listing, empty name prefix.
// Source `"/"` cursor 1 with provider entries `"/"` -> [bin, usr, etc]
// should surface all three as Path candidates. Overwrite span 1..1
// (insertion at the cursor, past the leading slash).
#[test]
fn r228m6_cmp_01_root_empty_prefix_three_candidates() {
    let engine = engine_with_fhs_paths();
    let req = CompletionRequest {
        source: "/".to_string(),
        cursor_byte: 1,
    };
    let resp = complete(&engine, &req);
    assert_eq!(
        resp.candidates.len(),
        3,
        "expected bin, usr, etc, got {:?}",
        resp.candidates
            .iter()
            .map(|c| c.text.as_str())
            .collect::<Vec<_>>()
    );
    let texts: Vec<&str> = resp.candidates.iter().map(|c| c.text.as_str()).collect();
    assert!(texts.contains(&"bin"), "missing bin: {texts:?}");
    assert!(texts.contains(&"usr"), "missing usr: {texts:?}");
    assert!(texts.contains(&"etc"), "missing etc: {texts:?}");
    assert_eq!(resp.prefix_start, 1, "insertion sits past the leading slash");
    assert_eq!(resp.prefix_end, 1);
    for c in &resp.candidates {
        assert_eq!(c.kind, CandidateKind::Path);
    }
}

// r228m6-cmp-02 -- Root directory listing, partial name prefix.
// Source `"/b"` cursor 2 should surface exactly `bin` (Tier 1b prefix
// match on `bin` given `b`). Overwrite span 1..2 replaces `b` with
// `bin` on acceptance.
#[test]
fn r228m6_cmp_02_root_partial_prefix_one_candidate() {
    let engine = engine_with_fhs_paths();
    let req = CompletionRequest {
        source: "/b".to_string(),
        cursor_byte: 2,
    };
    let resp = complete(&engine, &req);
    assert_eq!(
        resp.candidates.len(),
        1,
        "expected only bin, got {:?}",
        resp.candidates
            .iter()
            .map(|c| c.text.as_str())
            .collect::<Vec<_>>()
    );
    assert_eq!(resp.candidates[0].text, "bin");
    assert_eq!(resp.candidates[0].kind, CandidateKind::Path);
    assert_eq!(resp.prefix_start, 1, "overwrite starts past the leading slash");
    assert_eq!(resp.prefix_end, 2);
}

// r228m6-cmp-03 -- Nested directory listing, empty name prefix.
// Source `"/usr/"` cursor 5 with provider entries `"/usr"` -> [local,
// share] should surface both. Overwrite span 5..5 (insertion at cursor,
// past the trailing slash).
#[test]
fn r228m6_cmp_03_nested_dir_empty_prefix_two_candidates() {
    let engine = engine_with_fhs_paths();
    let req = CompletionRequest {
        source: "/usr/".to_string(),
        cursor_byte: 5,
    };
    let resp = complete(&engine, &req);
    assert_eq!(
        resp.candidates.len(),
        2,
        "expected local and share, got {:?}",
        resp.candidates
            .iter()
            .map(|c| c.text.as_str())
            .collect::<Vec<_>>()
    );
    let texts: Vec<&str> = resp.candidates.iter().map(|c| c.text.as_str()).collect();
    assert!(texts.contains(&"local"), "missing local: {texts:?}");
    assert!(texts.contains(&"share"), "missing share: {texts:?}");
    assert_eq!(resp.prefix_start, 5, "insertion sits past the trailing slash");
    assert_eq!(resp.prefix_end, 5);
    for c in &resp.candidates {
        assert_eq!(c.kind, CandidateKind::Path);
    }
}

// r228m6-cmp-04 -- Nested directory listing, partial name prefix.
// Source `"/usr/lo"` cursor 7 should surface exactly `local` (Tier 1b
// prefix match). Overwrite span 5..7 replaces `lo` with `local`.
#[test]
fn r228m6_cmp_04_nested_dir_partial_prefix_one_candidate() {
    let engine = engine_with_fhs_paths();
    let req = CompletionRequest {
        source: "/usr/lo".to_string(),
        cursor_byte: 7,
    };
    let resp = complete(&engine, &req);
    assert_eq!(
        resp.candidates.len(),
        1,
        "expected only local, got {:?}",
        resp.candidates
            .iter()
            .map(|c| c.text.as_str())
            .collect::<Vec<_>>()
    );
    assert_eq!(resp.candidates[0].text, "local");
    assert_eq!(resp.candidates[0].kind, CandidateKind::Path);
    assert_eq!(
        resp.prefix_start, 5,
        "overwrite starts past the last slash in the path text"
    );
    assert_eq!(resp.prefix_end, 7);
}

// r228m6-cmp-05 -- Missing directory in provider. Source `"/nope/"`
// cursor 6 whose enclosing dir `"/nope"` is absent from the provider
// should still CLAIM the request (path-in-progress detected) and
// return 0 candidates rather than fall through to another branch.
#[test]
fn r228m6_cmp_05_missing_dir_zero_candidates_but_claims() {
    let engine = engine_with_fhs_paths();
    let req = CompletionRequest {
        source: "/nope/".to_string(),
        cursor_byte: 6,
    };
    let resp = complete(&engine, &req);
    assert_eq!(
        resp.candidates.len(),
        0,
        "missing dir -> zero candidates, got {:?}",
        resp.candidates
    );
    // Insertion at the cursor, past the trailing slash.
    assert_eq!(resp.prefix_start, 6);
    assert_eq!(resp.prefix_end, 6);
}

// r228m6-cmp-06 -- Kind assertion. Every returned Path candidate MUST
// carry `kind: CandidateKind::Path` (regression guard against a copy-
// paste from the field- or command-emitters that would leak the wrong
// kind).
#[test]
fn r228m6_cmp_06_kind_is_path_on_every_candidate() {
    let engine = engine_with_fhs_paths();
    let req = CompletionRequest {
        source: "/".to_string(),
        cursor_byte: 1,
    };
    let resp = complete(&engine, &req);
    assert!(
        !resp.candidates.is_empty(),
        "sanity: fhs paths setup should emit candidates"
    );
    for c in &resp.candidates {
        assert_eq!(
            c.kind,
            CandidateKind::Path,
            "expected Path kind, got {:?} for {:?}",
            c.kind, c.text
        );
    }
}

// r228m6-cmp-07 -- Scoring agrees with the M3 ranker tiers. Source
// `"/b"` cursor 2 against provider entries `/` -> [bin, usr, etc]:
// only `bin` starts with `b` (Tier 1b exact prefix: 1000 - 3 = 997);
// `usr` and `etc` do not match at all (Tier 3 subsequence would
// require the prefix `b` to appear in the candidate, which it does
// not, so no candidate). Verifies the score matches Tier 1b exactly.
#[test]
fn r228m6_cmp_07_score_matches_m3_tier_formula() {
    let engine = engine_with_fhs_paths();
    let req = CompletionRequest {
        source: "/b".to_string(),
        cursor_byte: 2,
    };
    let resp = complete(&engine, &req);
    assert_eq!(resp.candidates.len(), 1, "only bin matches prefix `b`");
    let candidate = &resp.candidates[0];
    assert_eq!(candidate.text, "bin");
    // Tier 1b exact case-sensitive prefix: 1000 - char_len("bin") = 997.
    assert_eq!(
        candidate.score, 997,
        "expected Tier 1b score 997 for bin, got {}",
        candidate.score
    );
}

// r228m6-cmp-08 -- Non-path source falls through to other branches.
// Source `"ls"` cursor 2 has no leading `/`, so the path helper
// returns None; the M1 command-position branch then emits Command
// candidates (`ls` is seeded on the engine). Asserts no Path
// candidates are present.
#[test]
fn r228m6_cmp_08_non_path_falls_through_to_command_branch() {
    // Layer a `PathProvider` on an engine that also has commands, so
    // the fall-through target is populated.
    let mut provider = PathProvider::new();
    provider.insert("/", vec!["bin".to_string()]);
    let engine = CompletionEngine::with_lists(vec!["ls".to_string()], Vec::new())
        .with_path_provider(provider);
    let req = CompletionRequest {
        source: "ls".to_string(),
        cursor_byte: 2,
    };
    let resp = complete(&engine, &req);
    // No Path candidates -- the source has no leading `/`.
    for c in &resp.candidates {
        assert_ne!(
            c.kind,
            CandidateKind::Path,
            "non-path source emitted a Path candidate: {c:?}"
        );
    }
    // The command branch must still fire and surface `ls`.
    assert!(
        resp.candidates.iter().any(|c| c.text == "ls"
            && c.kind == CandidateKind::Command),
        "expected Command(ls) from the fall-through branch, got {:?}",
        resp.candidates
    );
}
