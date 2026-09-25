//! R228.M8 completion argument-position — fixture corpus.
//!
//! Each test is fingerprinted `r228m8-cmp-NN` so a git-log grep against
//! the CHANGELOG-r228m8 fragment lands on the exact fixture that
//! motivated a change. The 8-fixture plan is in paideia-as issue #1484.
//!
//! Fixtures cover the [`ArgPosition`] classifier and its wire-in
//! through [`complete`]:
//!
//! * `arg_position_at` returns the (command, arg_index) tuple for
//!   post-command whitespace and mid-argument cursors (01, 03, 05, 06).
//! * `arg_position_at` returns `None` for still-typing-the-command
//!   (02) and empty source (04).
//! * `complete` emits an [`CandidateKind::Argument`] placeholder when
//!   no earlier branch claims and the cursor is in argument position
//!   (07).
//! * Flag branch wins over the argument-fallback when the active
//!   token shapes as a dash-in-progress (08).

use std::collections::HashMap;

use paideia_as_shell_completion::{
    ArgPosition, CandidateKind, CommandFlags, CompletionEngine, CompletionRequest,
    arg_position_at, complete,
};

// r228m8-cmp-01 -- Two completed arguments after `ls`; cursor past the
// trailing space classifies as slot #2 (the third argument the user is
// about to type).
#[test]
fn r228m8_cmp_01_two_args_cursor_on_trailing_ws() {
    let pos = arg_position_at("ls foo bar ", 11);
    assert_eq!(
        pos,
        Some(ArgPosition {
            command: "ls".to_string(),
            arg_index: 2,
        })
    );
}

// r228m8-cmp-02 -- Cursor at the end of the command token itself is
// still command position; `arg_position_at` must return None so the
// caller does NOT route through the argument-fallback.
#[test]
fn r228m8_cmp_02_still_in_command_position_is_none() {
    assert_eq!(arg_position_at("ls", 2), None);
}

// r228m8-cmp-03 -- Cursor immediately after the trailing space with
// no argument tokens yet: slot #0.
#[test]
fn r228m8_cmp_03_bare_command_plus_space_is_slot_zero() {
    let pos = arg_position_at("ls ", 3);
    assert_eq!(
        pos,
        Some(ArgPosition {
            command: "ls".to_string(),
            arg_index: 0,
        })
    );
}

// r228m8-cmp-04 -- Empty source produces no classification.
#[test]
fn r228m8_cmp_04_empty_source_is_none() {
    assert_eq!(arg_position_at("", 0), None);
}

// r228m8-cmp-05 -- Pipeline crossing. After the `|`, the command
// resolves to the post-pipe ident `ls`; the args-before-pipe (`foo`,
// `bar`) belong to the previous stage and MUST NOT bleed into the
// arg_index count.
#[test]
fn r228m8_cmp_05_pipeline_crossing_post_pipe_command() {
    let pos = arg_position_at("foo bar | ls ", 13);
    assert_eq!(
        pos,
        Some(ArgPosition {
            command: "ls".to_string(),
            arg_index: 0,
        })
    );
}

// r228m8-cmp-06 -- Multi-stage pipeline. Cursor sits in the third
// stage headed by `c`; the earlier `a` / `b` stages do not contribute
// to either the command name or the arg_index.
#[test]
fn r228m8_cmp_06_multistage_pipeline_last_command_wins() {
    let pos = arg_position_at("a | b | c ", 10);
    assert_eq!(
        pos,
        Some(ArgPosition {
            command: "c".to_string(),
            arg_index: 0,
        })
    );
}

// r228m8-cmp-07 -- `complete` emits an Argument placeholder candidate
// when no earlier branch (flag/field/path/command/var/keyword) claims
// and `arg_position_at` returns Some. The placeholder's text carries
// the M8 baseline label `<command> arg #<index>`; R228.M9 replaces it
// with per-arg-position typed candidates.
#[test]
fn r228m8_cmp_07_complete_emits_argument_placeholder() {
    let engine = CompletionEngine::with_lists(vec!["ls".to_string()], Vec::new());
    let req = CompletionRequest {
        source: "ls foo ".to_string(),
        cursor_byte: 7,
    };
    let resp = complete(&engine, &req);
    assert_eq!(resp.candidates.len(), 1, "expected a single Argument placeholder");
    let c = &resp.candidates[0];
    assert_eq!(c.kind, CandidateKind::Argument);
    assert_eq!(c.text, "<ls arg #1>");
    assert!(c.snippet.is_none(), "placeholder must carry snippet: None");
    // Insertion span at the cursor -- the placeholder is not literal
    // insertion text.
    assert_eq!(resp.prefix_start, 7);
    assert_eq!(resp.prefix_end, 7);
}

// r228m8-cmp-08 -- Flag branch wins when the active token starts with
// a dash. Even though the cursor is nominally in argument position
// (past the command name), the flag-completion helper claims the
// request FIRST and the argument-fallback never fires; no Argument
// candidate must appear in the response.
#[test]
fn r228m8_cmp_08_flag_wins_no_argument_placeholder() {
    let cf = CommandFlags::new().with_short(vec!["a".to_string()]);
    let mut map: HashMap<String, CommandFlags> = HashMap::new();
    map.insert("ls".to_string(), cf);
    let engine = CompletionEngine::with_lists(vec!["ls".to_string()], Vec::new())
        .with_command_flags(map);
    let req = CompletionRequest {
        source: "ls -".to_string(),
        cursor_byte: 4,
    };
    let resp = complete(&engine, &req);
    assert!(
        !resp.candidates.is_empty(),
        "sanity: flag branch should emit -a for the seeded catalogue"
    );
    for c in &resp.candidates {
        assert_ne!(
            c.kind,
            CandidateKind::Argument,
            "flag branch claimed; no Argument placeholder must appear: {c:?}"
        );
    }
}
