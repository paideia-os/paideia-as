//! R228.M1 tab-completion substrate — fixture corpus.
//!
//! Each test is fingerprinted `r228m1-cmp-NN` so a git-log grep against
//! the CHANGELOG-r228m1 fragment lands on the exact fixture that
//! motivated a change. The fingerprints match the 8-fixture plan in
//! paideia-as issue #1468.

use paideia_as_shell_completion::{
    Candidate, CandidateKind, CompletionEngine, CompletionRequest, complete,
};

// r228m1-cmp-01 -- empty source, empty engine, cursor at 0.
// Result: no candidates, insertion span at 0.
#[test]
fn r228m1_cmp_01_empty_source() {
    let engine = CompletionEngine::empty();
    let req = CompletionRequest {
        source: String::new(),
        cursor_byte: 0,
    };
    let resp = complete(&engine, &req);
    assert!(resp.candidates.is_empty(), "expected no candidates on empty source");
    assert_eq!(resp.prefix_start, 0);
    assert_eq!(resp.prefix_end, 0);
}

// r228m1-cmp-02 -- single-letter prefix `l` with two commands starting
// with `l`. Both should surface as Command candidates.
#[test]
fn r228m1_cmp_02_two_commands_share_prefix() {
    let engine = CompletionEngine::with_lists(
        vec!["ls".to_string(), "less".to_string()],
        Vec::new(),
    );
    let req = CompletionRequest {
        source: "l".to_string(),
        cursor_byte: 1,
    };
    let resp = complete(&engine, &req);
    assert_eq!(resp.candidates.len(), 2, "expected both ls and less");
    for cand in &resp.candidates {
        assert_eq!(cand.kind, CandidateKind::Command);
        assert!(cand.text.starts_with('l'));
    }
    let texts: Vec<&str> = resp.candidates.iter().map(|c| c.text.as_str()).collect();
    assert!(texts.contains(&"ls"));
    assert!(texts.contains(&"less"));
}

// r228m1-cmp-03 -- cursor past a completed command name, sitting on
// whitespace. M1 does not do argument completion, so no candidates.
#[test]
fn r228m1_cmp_03_argument_position_empty_at_m1() {
    let engine = CompletionEngine::with_lists(
        vec!["ls".to_string()],
        Vec::new(),
    );
    let req = CompletionRequest {
        source: "ls ".to_string(),
        cursor_byte: 3,
    };
    let resp = complete(&engine, &req);
    assert!(
        resp.candidates.is_empty(),
        "M1 does not offer argument-position completions; got {:?}",
        resp.candidates
    );
    // Insertion span at the cursor.
    assert_eq!(resp.prefix_start, 3);
    assert_eq!(resp.prefix_end, 3);
}

// r228m1-cmp-04 -- Lambda-context completion. `{ |x| xy` puts the
// cursor inside a lambda body with partial ident `xy`. Under M1 only
// `xyz` (starts_with "xy") matched; under M3 `xzz` also matches via
// the subsequence tier (x@0, y???--no y in xzz -- so NO match).
// Actually xzz has no 'y', so subsequence still fails; only xyz
// matches. Score for `xyz` on prefix `xy`: exact-prefix tier,
// 1000 - 3 = 997.
#[test]
fn r228m1_cmp_04_lambda_var_prefix() {
    let engine = CompletionEngine::with_lists(
        Vec::new(),
        vec!["xyz".to_string(), "xzz".to_string()],
    );
    let req = CompletionRequest {
        source: "{ |x| xy".to_string(),
        cursor_byte: 8,
    };
    let resp = complete(&engine, &req);
    assert_eq!(resp.candidates.len(), 1, "only xyz matches 'xy'");
    assert_eq!(
        resp.candidates[0],
        Candidate {
            text: "xyz".to_string(),
            kind: CandidateKind::Var,
            display: None,
            type_hint: None,
            score: 997,
            snippet: None,
        }
    );
    // Span covers just the `xy` token (bytes 6..8).
    assert_eq!(resp.prefix_start, 6);
    assert_eq!(resp.prefix_end, 8);
}

// r228m1-cmp-05 -- Datalog-context completion. Cursor on partial ident
// `n` inside `datalog { ... }` should offer the `not` keyword.
#[test]
fn r228m1_cmp_05_datalog_keyword_prefix() {
    let engine = CompletionEngine::empty();
    let req = CompletionRequest {
        source: "datalog { n".to_string(),
        cursor_byte: 12, // clamped to source.len() (11) defensively
    };
    let resp = complete(&engine, &req);
    assert_eq!(resp.candidates.len(), 1, "only 'not' matches 'n'");
    assert_eq!(
        resp.candidates[0],
        Candidate {
            text: "not".to_string(),
            kind: CandidateKind::Keyword,
            display: None,
            type_hint: None,
            score: 997, // Tier 1b: 1000 - 3.
            snippet: None,
        }
    );
}

// r228m1-cmp-06 -- Overwrite span math. `l` + [ls] should overwrite
// bytes 0..1 (the whole existing `l`) with the chosen candidate.
#[test]
fn r228m1_cmp_06_prefix_span_covers_token() {
    let engine = CompletionEngine::with_lists(
        vec!["ls".to_string()],
        Vec::new(),
    );
    let req = CompletionRequest {
        source: "l".to_string(),
        cursor_byte: 1,
    };
    let resp = complete(&engine, &req);
    assert_eq!(resp.prefix_start, 0);
    assert_eq!(resp.prefix_end, 1);
    assert_eq!(resp.candidates.len(), 1);
    assert_eq!(resp.candidates[0].text, "ls");
}

// r228m1-cmp-07 -- After a pipe with trailing whitespace, the cursor
// sits at command position (a new pipeline stage is starting). All
// registered commands surface as an insertion.
#[test]
fn r228m1_cmp_07_after_pipe_offers_commands() {
    let engine = CompletionEngine::with_lists(
        vec!["ls".to_string(), "cat".to_string()],
        Vec::new(),
    );
    let req = CompletionRequest {
        source: "ls | ".to_string(),
        cursor_byte: 5,
    };
    let resp = complete(&engine, &req);
    assert_eq!(resp.candidates.len(), 2, "both commands should surface");
    for cand in &resp.candidates {
        assert_eq!(cand.kind, CandidateKind::Command);
    }
    // Insertion at the cursor.
    assert_eq!(resp.prefix_start, 5);
    assert_eq!(resp.prefix_end, 5);
}

// r228m1-cmp-08 -- Case-fold fallback landed at R228.M3. Under M1 this
// fixture asserted an empty result (case-sensitive `starts_with` only);
// the M3 ranker's Tier 2 now surfaces `ls` as a case-insensitive prefix
// match with a lower score than an exact-prefix hit would earn. The
// fixture is retained (renamed only in comment intent) so a git-log
// grep against `r228m1-cmp-08` still lands on the tier-crossover
// regression case.
#[test]
fn r228m1_cmp_08_case_sensitive_at_m1() {
    let engine = CompletionEngine::with_lists(
        vec!["ls".to_string()],
        Vec::new(),
    );
    let req = CompletionRequest {
        source: "L".to_string(),
        cursor_byte: 1,
    };
    let resp = complete(&engine, &req);
    assert_eq!(
        resp.candidates.len(),
        1,
        "M3 case-insensitive fallback: `L` matches `ls`; got {:?}",
        resp.candidates
    );
    let cand = &resp.candidates[0];
    assert_eq!(cand.text, "ls");
    assert_eq!(cand.kind, CandidateKind::Command);
    assert_eq!(cand.score, 498, "Tier 2: 500 - 2 (len of `ls`)");
}
