//! R228.M7 completion snippets (placeholder-based structured
//! insertions) -- fixture corpus.
//!
//! Each test is fingerprinted `r228m7-cmp-NN` so a git-log grep against
//! the CHANGELOG-r228m7 fragment lands on the exact fixture that
//! motivated a change. The 8-fixture plan is in paideia-as issue #1482.
//!
//! Fixtures cover the [`Candidate::snippet`] enrichment surface:
//!
//! * Snippet populated when `CommandFlags::descriptions` carries a
//!   `"<bare>__snippet"` key (fixtures 01, 06, 07).
//! * Snippet stays `None` for the every non-flag emitter (command,
//!   path, var; fixtures 03, 04, 05) and for a flag whose catalogue
//!   has no `"__snippet"` entry (fixtures 02, 08).
//! * Snippet body is stored verbatim -- the completion crate does no
//!   placeholder validation, so `"$1 hello"` surfaces unchanged
//!   (fixture 06).
//! * The `"__snippet"` suffix is load-bearing: a bare key `"l"`
//!   populates `type_hint`, never `snippet` (fixture 08).

use std::collections::HashMap;

use paideia_as_shell_completion::{
    CandidateKind, CommandFlags, CompletionEngine, CompletionRequest, PathProvider, complete,
};

/// Build the `descriptions` map for a flag whose bare name is `name`
/// and whose snippet template is `tmpl`. Kept as a tiny helper so each
/// fixture's map construction reads at a glance.
fn descriptions_with_snippet(name: &str, tmpl: &str) -> HashMap<String, String> {
    let mut m = HashMap::new();
    m.insert(format!("{name}__snippet"), tmpl.to_string());
    m
}

// r228m7-cmp-01 -- Flag `-l` whose `CommandFlags::descriptions` carries
// `{"l__snippet": "-l ${1:pattern}"}` surfaces as a Flag candidate
// with `snippet: Some("-l ${1:pattern}")`.
#[test]
fn r228m7_cmp_01_flag_with_snippet_populated() {
    let mut cf = CommandFlags::new().with_short(vec!["l".to_string()]);
    cf.descriptions = descriptions_with_snippet("l", "-l ${1:pattern}");
    let mut map: HashMap<String, CommandFlags> = HashMap::new();
    map.insert("grep".to_string(), cf);
    let engine = CompletionEngine::with_lists(vec!["grep".to_string()], Vec::new())
        .with_command_flags(map);
    let req = CompletionRequest {
        source: "grep -l".to_string(),
        cursor_byte: 7,
    };
    let resp = complete(&engine, &req);
    assert_eq!(resp.candidates.len(), 1, "expected only -l, got {:?}",
        resp.candidates.iter().map(|c| c.text.as_str()).collect::<Vec<_>>());
    let c = &resp.candidates[0];
    assert_eq!(c.text, "-l");
    assert_eq!(c.kind, CandidateKind::Flag);
    assert_eq!(c.snippet.as_deref(), Some("-l ${1:pattern}"));
}

// r228m7-cmp-02 -- Flag `-a` whose `CommandFlags::descriptions` has
// no `"a__snippet"` key surfaces with `snippet: None`. The `type_hint`
// stays `None` here too (empty descriptions map) so this fixture also
// guards against a copy-paste bug where the M7 enrichment might reuse
// the description text as a snippet fallback.
#[test]
fn r228m7_cmp_02_flag_without_snippet_descriptor_is_none() {
    let cf = CommandFlags::new().with_short(vec!["a".to_string()]);
    let mut map: HashMap<String, CommandFlags> = HashMap::new();
    map.insert("ls".to_string(), cf);
    let engine = CompletionEngine::with_lists(vec!["ls".to_string()], Vec::new())
        .with_command_flags(map);
    let req = CompletionRequest {
        source: "ls -a".to_string(),
        cursor_byte: 5,
    };
    let resp = complete(&engine, &req);
    assert_eq!(resp.candidates.len(), 1);
    let c = &resp.candidates[0];
    assert_eq!(c.text, "-a");
    assert_eq!(c.kind, CandidateKind::Flag);
    assert!(c.snippet.is_none(),
        "flag without __snippet key must carry snippet: None, got {:?}", c.snippet);
}

// r228m7-cmp-03 -- Command candidates never carry a snippet. M7 wires
// the field only on Flag emitters; a Command candidate emitted from the
// command-position dispatch always has `snippet: None`.
#[test]
fn r228m7_cmp_03_command_candidate_snippet_none() {
    let engine = CompletionEngine::with_lists(vec!["ls".to_string()], Vec::new());
    let req = CompletionRequest {
        source: "l".to_string(),
        cursor_byte: 1,
    };
    let resp = complete(&engine, &req);
    assert_eq!(resp.candidates.len(), 1);
    let c = &resp.candidates[0];
    assert_eq!(c.text, "ls");
    assert_eq!(c.kind, CandidateKind::Command);
    assert!(c.snippet.is_none(),
        "command candidate must carry snippet: None, got {:?}", c.snippet);
}

// r228m7-cmp-04 -- Path candidates never carry a snippet.
#[test]
fn r228m7_cmp_04_path_candidate_snippet_none() {
    let mut provider = PathProvider::new();
    provider.insert("/", vec!["bin".to_string()]);
    let engine = CompletionEngine::empty().with_path_provider(provider);
    let req = CompletionRequest {
        source: "/b".to_string(),
        cursor_byte: 2,
    };
    let resp = complete(&engine, &req);
    assert_eq!(resp.candidates.len(), 1);
    let c = &resp.candidates[0];
    assert_eq!(c.text, "bin");
    assert_eq!(c.kind, CandidateKind::Path);
    assert!(c.snippet.is_none(),
        "path candidate must carry snippet: None, got {:?}", c.snippet);
}

// r228m7-cmp-05 -- Var candidates never carry a snippet.
#[test]
fn r228m7_cmp_05_var_candidate_snippet_none() {
    let engine = CompletionEngine::with_lists(
        Vec::new(),
        vec!["xyz".to_string()],
    );
    let req = CompletionRequest {
        source: "{ |x| xy".to_string(),
        cursor_byte: 8,
    };
    let resp = complete(&engine, &req);
    assert_eq!(resp.candidates.len(), 1);
    let c = &resp.candidates[0];
    assert_eq!(c.text, "xyz");
    assert_eq!(c.kind, CandidateKind::Var);
    assert!(c.snippet.is_none(),
        "var candidate must carry snippet: None, got {:?}", c.snippet);
}

// r228m7-cmp-06 -- Snippet body is stored verbatim; the completion
// crate does no placeholder validation. A descriptor whose value is
// `"$1 hello"` surfaces on the candidate unchanged, so a malformed
// template reaches the REPL / LSP renderer intact rather than being
// silently rewritten here.
#[test]
fn r228m7_cmp_06_snippet_body_verbatim() {
    let mut cf = CommandFlags::new().with_short(vec!["foo".to_string()]);
    cf.descriptions = descriptions_with_snippet("foo", "$1 hello");
    let mut map: HashMap<String, CommandFlags> = HashMap::new();
    map.insert("bar".to_string(), cf);
    let engine = CompletionEngine::with_lists(vec!["bar".to_string()], Vec::new())
        .with_command_flags(map);
    let req = CompletionRequest {
        source: "bar -foo".to_string(),
        cursor_byte: 8,
    };
    let resp = complete(&engine, &req);
    assert_eq!(resp.candidates.len(), 1);
    let c = &resp.candidates[0];
    assert_eq!(c.text, "-foo");
    assert_eq!(c.kind, CandidateKind::Flag);
    assert_eq!(c.snippet.as_deref(), Some("$1 hello"),
        "snippet body must round-trip byte-for-byte");
}

// r228m7-cmp-07 -- Multi-flag command: the descriptions map carries
// snippets for both `l__snippet` and `a__snippet`. Every candidate the
// short-mode emitter yields must carry `snippet: Some(_)` with the
// matching template.
#[test]
fn r228m7_cmp_07_multi_flag_both_carry_snippets() {
    let mut cf = CommandFlags::new().with_short(vec!["l".to_string(), "a".to_string()]);
    let mut d: HashMap<String, String> = HashMap::new();
    d.insert("l__snippet".to_string(), "-l ${1:pattern}".to_string());
    d.insert("a__snippet".to_string(), "-a ${1:archive}".to_string());
    cf.descriptions = d;
    let mut map: HashMap<String, CommandFlags> = HashMap::new();
    map.insert("ls".to_string(), cf);
    let engine = CompletionEngine::with_lists(vec!["ls".to_string()], Vec::new())
        .with_command_flags(map);
    let req = CompletionRequest {
        source: "ls -".to_string(),
        cursor_byte: 4,
    };
    let resp = complete(&engine, &req);
    assert_eq!(resp.candidates.len(), 2, "expected -a and -l");
    for c in &resp.candidates {
        assert_eq!(c.kind, CandidateKind::Flag);
        match c.text.as_str() {
            "-l" => assert_eq!(c.snippet.as_deref(), Some("-l ${1:pattern}")),
            "-a" => assert_eq!(c.snippet.as_deref(), Some("-a ${1:archive}")),
            other => panic!("unexpected flag candidate text: {other:?}"),
        }
    }
}

// r228m7-cmp-08 -- The `"__snippet"` suffix is load-bearing. A bare
// descriptions key `"l"` populates `type_hint` (the M5 convention)
// and MUST NOT be aliased into `snippet` -- otherwise a REPL that
// registers descriptions the M5 way would accidentally surface every
// help string as a template.
#[test]
fn r228m7_cmp_08_bare_key_does_not_populate_snippet() {
    let mut cf = CommandFlags::new().with_short(vec!["l".to_string()]);
    let mut d: HashMap<String, String> = HashMap::new();
    d.insert("l".to_string(), "list contents in long form".to_string());
    cf.descriptions = d;
    let mut map: HashMap<String, CommandFlags> = HashMap::new();
    map.insert("ls".to_string(), cf);
    let engine = CompletionEngine::with_lists(vec!["ls".to_string()], Vec::new())
        .with_command_flags(map);
    let req = CompletionRequest {
        source: "ls -l".to_string(),
        cursor_byte: 5,
    };
    let resp = complete(&engine, &req);
    assert_eq!(resp.candidates.len(), 1);
    let c = &resp.candidates[0];
    assert_eq!(c.text, "-l");
    assert_eq!(c.kind, CandidateKind::Flag);
    // type_hint IS populated from the bare `l` key -- that is the M5
    // convention this fixture must not disturb.
    assert_eq!(c.type_hint.as_deref(), Some("list contents in long form"));
    // snippet MUST stay None -- the bare `l` key is not `l__snippet`.
    assert!(c.snippet.is_none(),
        "bare descriptions key must NOT populate snippet, got {:?}", c.snippet);
}
