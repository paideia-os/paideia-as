//! R228.M5 flag completion (short `-a` / long `--long`) -- fixture
//! corpus.
//!
//! Each test is fingerprinted `r228m5-cmp-NN` so a git-log grep against
//! the CHANGELOG-r228m5 fragment lands on the exact fixture that
//! motivated a change. The 8-fixture plan is in paideia-as issue #1478.
//!
//! Fixtures cover the four dash-in-progress shapes the completion
//! engine's [`try_flag_completion`] helper recognises:
//!
//! * `Op("-") ^`                — short mode, empty prefix.
//! * `Op("-") Op("-") ^`        — long mode, empty prefix.
//! * `Op("-") Ident^`           — short mode, prefix = ident.
//! * `Op("-") Op("-") Ident^`   — long mode, prefix = ident.
//!
//! plus the three no-claim / empty-response invariants (no dash at
//! all, no [`CommandFlags`] entry for the command, dash after another
//! argument).

use std::collections::HashMap;

use paideia_as_shell_completion::{
    CandidateKind, CommandFlags, CompletionEngine, CompletionRequest, complete,
};

/// Build an engine with a single command `ls` whose flag catalogue is
/// `short=[a, l]`, `long=[all, long]`. Used by fixtures 01, 02, 03, 05,
/// 06 so the shape assertions are independent of catalogue setup drift.
fn engine_with_ls_flags() -> CompletionEngine {
    let mut map: HashMap<String, CommandFlags> = HashMap::new();
    map.insert(
        "ls".to_string(),
        CommandFlags::new()
            .with_short(vec!["a".to_string(), "l".to_string()])
            .with_long(vec!["all".to_string(), "long".to_string()]),
    );
    CompletionEngine::with_lists(vec!["ls".to_string()], Vec::new())
        .with_command_flags(map)
}

// r228m5-cmp-01 -- Short-mode, empty prefix. Source `"ls -"` cursor 4
// with short=[a, l] should surface both `-a` and `-l` as Flag
// candidates. Overwrite span 3..4 (just the dash) so the caller
// replaces `-` with `-a`.
#[test]
fn r228m5_cmp_01_short_empty_prefix_two_candidates() {
    let engine = engine_with_ls_flags();
    let req = CompletionRequest {
        source: "ls -".to_string(),
        cursor_byte: 4,
    };
    let resp = complete(&engine, &req);
    assert_eq!(resp.candidates.len(), 2, "expected -a and -l");
    let texts: Vec<&str> = resp.candidates.iter().map(|c| c.text.as_str()).collect();
    assert!(texts.contains(&"-a"), "missing -a: {texts:?}");
    assert!(texts.contains(&"-l"), "missing -l: {texts:?}");
    assert_eq!(resp.prefix_start, 3, "overwrite starts at the leading dash");
    assert_eq!(resp.prefix_end, 4, "overwrite ends at the cursor");
    for c in &resp.candidates {
        assert_eq!(c.kind, CandidateKind::Flag);
    }
}

// r228m5-cmp-02 -- Long-mode, partial prefix. Source `"ls --lo"`
// cursor 7 with long=[all, long] should surface exactly `--long`
// (Tier 1b prefix match on bare `long` given `lo`). Overwrite span
// 3..7 (the `--lo` triple).
#[test]
fn r228m5_cmp_02_long_partial_prefix_one_candidate() {
    let engine = engine_with_ls_flags();
    let req = CompletionRequest {
        source: "ls --lo".to_string(),
        cursor_byte: 7,
    };
    let resp = complete(&engine, &req);
    assert_eq!(
        resp.candidates.len(),
        1,
        "expected only --long, got {:?}",
        resp.candidates
            .iter()
            .map(|c| c.text.as_str())
            .collect::<Vec<_>>()
    );
    assert_eq!(resp.candidates[0].text, "--long");
    assert_eq!(resp.candidates[0].kind, CandidateKind::Flag);
    assert_eq!(resp.prefix_start, 3, "overwrite starts at the first dash");
    assert_eq!(resp.prefix_end, 7);
}

// r228m5-cmp-03 -- No dash typed. Source `"ls "` cursor 3 should NOT
// emit Flag candidates: the M5 helper only fires on a dash-in-
// progress. Falls through to the M1 "no active token" branch, which
// returns an empty response because `ls` is not at command position
// (nothing to complete against for argument position at M1..M5).
#[test]
fn r228m5_cmp_03_no_dash_no_flag_candidates() {
    let engine = engine_with_ls_flags();
    let req = CompletionRequest {
        source: "ls ".to_string(),
        cursor_byte: 3,
    };
    let resp = complete(&engine, &req);
    for c in &resp.candidates {
        assert_ne!(
            c.kind,
            CandidateKind::Flag,
            "no dash typed, yet got a Flag candidate: {c:?}",
        );
    }
}

// r228m5-cmp-04 -- Command with no CommandFlags entry. Engine has
// `ls` in `commands` but the `command_flags` map is empty for `ls`.
// The M5 helper still claims the request (short-mode, empty prefix)
// and returns 0 candidates rather than falling through to the
// argument-position empty branch. Test asserts 0 candidates total.
#[test]
fn r228m5_cmp_04_no_command_flags_entry_zero_candidates() {
    let engine = CompletionEngine::with_lists(vec!["ls".to_string()], Vec::new());
    let req = CompletionRequest {
        source: "ls -".to_string(),
        cursor_byte: 4,
    };
    let resp = complete(&engine, &req);
    assert_eq!(
        resp.candidates.len(),
        0,
        "no flag catalogue for ls -> zero candidates, got {:?}",
        resp.candidates
    );
}

// r228m5-cmp-05 -- Kind assertion. Every returned Flag candidate MUST
// carry `kind: CandidateKind::Flag` (regression guard against a copy-
// paste from the command-emitter that would leak `Command` kind).
#[test]
fn r228m5_cmp_05_kind_is_flag_on_every_candidate() {
    let engine = engine_with_ls_flags();
    let req = CompletionRequest {
        source: "ls -".to_string(),
        cursor_byte: 4,
    };
    let resp = complete(&engine, &req);
    assert!(!resp.candidates.is_empty(), "sanity: setup should emit candidates");
    for c in &resp.candidates {
        assert_eq!(
            c.kind,
            CandidateKind::Flag,
            "expected Flag kind, got {:?} for {:?}",
            c.kind, c.text
        );
    }
}

// r228m5-cmp-06 -- Flag after another argument. Source `"ls x -"`
// cursor 6: tokens are `Ident("ls"), Ident("x"), Op("-")`. The dash
// still fires the flag branch because the command name `ls` is
// discoverable by walking back past `x` (which is at argument
// position) to the first command-position ident.
#[test]
fn r228m5_cmp_06_flag_after_argument_still_fires() {
    let engine = engine_with_ls_flags();
    let req = CompletionRequest {
        source: "ls x -".to_string(),
        cursor_byte: 6,
    };
    let resp = complete(&engine, &req);
    assert_eq!(
        resp.candidates.len(),
        2,
        "expected -a and -l after an argument, got {:?}",
        resp.candidates
    );
    let texts: Vec<&str> = resp.candidates.iter().map(|c| c.text.as_str()).collect();
    assert!(texts.contains(&"-a"));
    assert!(texts.contains(&"-l"));
    for c in &resp.candidates {
        assert_eq!(c.kind, CandidateKind::Flag);
    }
    // Overwrite span is the dash alone.
    assert_eq!(resp.prefix_start, 5);
    assert_eq!(resp.prefix_end, 6);
}

// r228m5-cmp-07 -- Long-mode disambiguation. Long list `[long, limit]`
// against prefix `"ls --l"` cursor 6 should surface both `--long` and
// `--limit` (both start with `l`). Regression guard that the ranker
// applies to the BARE name (not the dash-prefixed candidate text).
#[test]
fn r228m5_cmp_07_long_prefix_two_candidates() {
    let mut map: HashMap<String, CommandFlags> = HashMap::new();
    map.insert(
        "ls".to_string(),
        CommandFlags::new().with_long(vec!["long".to_string(), "limit".to_string()]),
    );
    let engine = CompletionEngine::with_lists(vec!["ls".to_string()], Vec::new())
        .with_command_flags(map);
    let req = CompletionRequest {
        source: "ls --l".to_string(),
        cursor_byte: 6,
    };
    let resp = complete(&engine, &req);
    assert_eq!(
        resp.candidates.len(),
        2,
        "expected --long and --limit, got {:?}",
        resp.candidates
            .iter()
            .map(|c| c.text.as_str())
            .collect::<Vec<_>>()
    );
    let texts: Vec<&str> = resp.candidates.iter().map(|c| c.text.as_str()).collect();
    assert!(texts.contains(&"--long"));
    assert!(texts.contains(&"--limit"));
    for c in &resp.candidates {
        assert_eq!(c.kind, CandidateKind::Flag);
    }
}

// r228m5-cmp-08 -- Empty-prefix match tier on the short list. Source
// `"ls -"` cursor 4 with short=[a, b, c] should surface all three
// candidates. Each candidate's score reflects the Tier 1a empty-
// prefix formula applied to the BARE name: `1000 - 1` = 999 for
// each single-letter flag. This is the same tier as the exact
// case-sensitive prefix (Tier 1b), so a mixed corpus that later
// adds a `-ab` entry would rank the shorter flag first.
#[test]
fn r228m5_cmp_08_empty_prefix_tier_score() {
    let mut map: HashMap<String, CommandFlags> = HashMap::new();
    map.insert(
        "ls".to_string(),
        CommandFlags::new().with_short(vec![
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
        ]),
    );
    let engine = CompletionEngine::with_lists(vec!["ls".to_string()], Vec::new())
        .with_command_flags(map);
    let req = CompletionRequest {
        source: "ls -".to_string(),
        cursor_byte: 4,
    };
    let resp = complete(&engine, &req);
    assert_eq!(resp.candidates.len(), 3, "expected -a, -b, -c");
    // Empty-prefix Tier 1a: 1000 - char_len(bare_name) = 1000 - 1 = 999.
    for c in &resp.candidates {
        assert_eq!(
            c.score, 999,
            "expected Tier 1a empty-prefix score 999 for {:?}, got {}",
            c.text, c.score
        );
        assert_eq!(c.kind, CandidateKind::Flag);
    }
    // Sort order is (score desc, text asc); all equal scores so alpha.
    assert_eq!(resp.candidates[0].text, "-a");
    assert_eq!(resp.candidates[1].text, "-b");
    assert_eq!(resp.candidates[2].text, "-c");
}
