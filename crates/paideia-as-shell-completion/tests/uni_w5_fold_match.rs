//! UNI-W5 wire-up fixtures (paideia-as#1599) for the ranker's
//! case-insensitive tier.
//!
//! Tier 2 now folds through `paideia_as_unicode::fold_default`
//! (UCD `CaseFolding.txt` C + F rows) rather than
//! `str::to_lowercase` (simple casing only). The two tiers disagree
//! on every codepoint whose full-case fold expands or diverges from
//! simple lowercasing — Latin-1 sharp s (`ß` → `ss`), Greek sigma,
//! Turkic I/i under their non-default locale, etc. The default
//! locale (which the ranker uses) covers `ß`, which is the
//! canonical fixture.

use paideia_as_shell_completion::matching::score_match;
use paideia_as_shell_completion::{
    CandidateKind, CompletionEngine, CompletionRequest, complete,
};

/// uni-w5-cmp-01 — `ß` prefixes `SS` under full case folding.
///
/// Default fold of `ß` (U+00DF) is `ss` (two ASCII `s`); default fold
/// of `SS` is `ss`. Tier 2 therefore fires with score
/// `CASE_INSENSITIVE_BASE - cand_char_len` == `500 - 2` == `498`.
/// Pre-UNI-W5 the ranker used `.to_lowercase()`: `"ß".to_lowercase()
/// == "ß"` and `"SS".to_lowercase() == "ss"`, which don't share a
/// prefix — Tier 2 missed, Tier 3 subsequence fired only if
/// characters subsequence-matched (they do not: `ß` is not in `SS`),
/// so the pre-wire ranker returned `None`.
#[test]
fn uni_w5_cmp_01_full_fold_ss_matches_sharp_s() {
    let score = score_match("ß", "SS");
    assert_eq!(
        score,
        Some(498),
        "`ß` prefixes `SS` under full case folding (score = 500 - 2)"
    );
}

/// uni-w5-cmp-02 — default-locale fold does NOT equate `I` and `ı`.
///
/// Default fold of `I` is `i`; default fold of `ı` (U+0131 dotless i)
/// is `ı`. The two differ, so `score_match("I", "ı")` does NOT fire
/// Tier 2 (and neither Tier 1a nor Tier 1b fire, nor does the
/// subsequence tier — the two single-char strings share no
/// characters). The pair IS Tier-2 equivalent under Turkic locale;
/// this guard pins the ranker to Default, matching the SH-D9.4
/// shell-command-matching contract.
#[test]
fn uni_w5_cmp_02_default_fold_rejects_turkic_pair() {
    assert_eq!(
        score_match("I", "ı"),
        None,
        "default-locale fold must not equate Turkic dotted/dotless I"
    );
}

/// uni-w5-cmp-03 — `complete()` surfaces a `straße` candidate for
/// the uppercase prefix `STRA` via Tier 2.
///
/// End-to-end wire-up: the engine's commands list carries `straße`;
/// the user types `STRA`. Default fold of `STRA` is `stra`; default
/// fold of `straße` is `strasse`. The second string starts with the
/// first, so Tier 2 fires with score `500 - chars().count()` ==
/// `500 - 6` == `494`. The response contains exactly one `Command`
/// candidate.
#[test]
fn uni_w5_cmp_03_complete_tier_two_full_fold() {
    let engine = CompletionEngine::with_lists(
        vec!["straße".to_string()],
        Vec::new(),
    );
    let req = CompletionRequest {
        source: "STRA".to_string(),
        cursor_byte: 4,
    };
    let resp = complete(&engine, &req);
    assert_eq!(
        resp.candidates.len(),
        1,
        "`STRA` must Tier-2 match `straße` under full case folding"
    );
    assert_eq!(resp.candidates[0].text, "straße");
    assert_eq!(resp.candidates[0].kind, CandidateKind::Command);
    assert_eq!(
        resp.candidates[0].score, 494,
        "Tier 2 score = 500 - chars().count()"
    );
}
