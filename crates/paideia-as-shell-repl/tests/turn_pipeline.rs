//! R229.M1 turn-pipeline corpus: ten fixtures pinning the skeleton
//! of `eval_turn` against the four executor branches (parse-error,
//! datalog-block, unimplemented-cmd, unimplemented-lambda) plus the
//! session state's turn-counter / fingerprint / EDB survival
//! invariants.
//!
//! Each `assert!` names the fingerprint (`r229m1-turn-NN`) in its
//! message so R220.M10's fingerprint correlator attributes a
//! regression to a fixture without re-parsing the test name.

use paideia_as_shell_datalog::Value;
use paideia_as_shell_repl::{eval_turn, ReplState, ReplTurn, TurnResult};

/// `r229m1-turn-01`: empty source parses to an empty `Seq`; the
/// executor's generic-fallback branch renders `seq — <not yet
/// implemented>`. Fingerprint is the first-turn `0x0000…`.
#[test]
fn r229m1_turn_01_empty_source() {
    const FP: &str = "r229m1-turn-01";
    let mut state = ReplState::new();
    let turn = eval_turn(&mut state, String::new());

    assert_eq!(
        turn.fingerprint, "repl.turn.0000000000000000",
        "{FP}: first-turn fingerprint must be all zeros"
    );
    match turn.result {
        TurnResult::Value(ref v) => assert!(
            !v.is_empty(),
            "{FP}: empty-source turn must still produce a non-empty rendered value"
        ),
        TurnResult::Error(ref e) => panic!("{FP}: empty source parsed to error: {e}"),
    }
}

/// `r229m1-turn-02`: `datalog { p(a). }` runs the Datalog branch,
/// lowers the AST to a `Program`, then hits R226.M9's schema check
/// with the default (empty) `SchemaRegistry` — which rejects the
/// unregistered `p/1`. Rendered as a `typecheck:` error. (Updated
/// under R229.M2: the M1 render was "dlg: N facts loaded" against
/// `run_stratified_with_session`; M2 replaces that with the typed
/// query path and the empty registry rejects any user predicate.)
#[test]
fn r229m1_turn_02_datalog_block() {
    const FP: &str = "r229m1-turn-02";
    let mut state = ReplState::new();
    let turn = eval_turn(&mut state, "datalog { p(a). }".to_owned());

    match turn.result {
        TurnResult::Error(e) => {
            assert!(
                e.starts_with("typecheck:"),
                "{FP}: datalog branch under M2 must render 'typecheck:' prefix, got: {e:?}"
            );
            assert!(
                e.contains("error"),
                "{FP}: expected 'error' text in typecheck diagnostic, got: {e:?}"
            );
        }
        TurnResult::Value(v) => panic!("{FP}: expected typecheck error, got Value: {v}"),
    }
}

/// `r229m1-turn-03`: unterminated datalog block (`datalog { p( `)
/// surfaces a parse error prefixed with `parse:`.
#[test]
fn r229m1_turn_03_parse_error() {
    const FP: &str = "r229m1-turn-03";
    let mut state = ReplState::new();
    let turn = eval_turn(&mut state, "datalog { p( ".to_owned());

    match turn.result {
        TurnResult::Error(e) => assert!(
            e.starts_with("parse:"),
            "{FP}: parse failure must be prefixed 'parse:', got: {e:?}"
        ),
        TurnResult::Value(v) => panic!(
            "{FP}: malformed input parsed successfully to: {v:?}"
        ),
    }
}

/// `r229m1-turn-04`: `ls | wc` dispatches to the Cmd/Pipe branch.
/// R229.M1 stub message was `"not yet implemented"`; R229.M3 upgraded
/// it to `"pipe: N stages"`; R229.M4 makes the arm run for real —
/// against an empty registry `ls` at stage 0 fails with
/// `UnknownCommand`, so the pipeline halts and the turn surfaces
/// `TurnResult::Error("pipe: pipeline halted at stage 0 (unknown
/// command: ls)")`. The fingerprint stays `r229m1-turn-04` because the
/// architectural fact under test — a `Pipe` node routes to the pipe
/// arm, not to Cmd's `non-name head` or the generic stub — has not
/// changed.
#[test]
fn r229m1_turn_04_pipeline_stub() {
    const FP: &str = "r229m1-turn-04";
    let mut state = ReplState::new();
    let turn = eval_turn(&mut state, "ls | wc".to_owned());

    match turn.result {
        TurnResult::Error(e) => {
            assert!(
                e.starts_with("pipe:"),
                "{FP}: pipe halt must carry the `pipe:` stage tag, got: {e:?}"
            );
            assert!(
                e.contains("halted at stage 0"),
                "{FP}: empty registry halts at stage 0, got: {e:?}"
            );
            assert!(
                e.contains("unknown command: ls"),
                "{FP}: halt reason must name the missing command, got: {e:?}"
            );
        }
        TurnResult::Value(v) => panic!(
            "{FP}: empty registry must halt the pipeline, got Value: {v}"
        ),
    }
}

/// `r229m1-turn-05`: `{ |x| x }` — a lambda literal — dispatches to
/// the Lambda branch. Under R229.M1..M4 this rendered the placeholder
/// `lambda: <not yet implemented>`; R229.M5 makes the branch really
/// evaluate the lambda, so the identity closure renders as the value
/// literal `<closure>` (see [`paideia_as_shell_repl::Value::Fn`] and
/// the Lambda arm of `turn::execute`). The fixture id stays
/// `r229m1-turn-05` because the architectural fact — a Lambda node
/// routes to the Lambda arm — has not changed.
#[test]
fn r229m1_turn_05_lambda_stub() {
    const FP: &str = "r229m1-turn-05";
    let mut state = ReplState::new();
    let turn = eval_turn(&mut state, "{ |x| x }".to_owned());

    match turn.result {
        TurnResult::Value(v) => assert_eq!(
            v, "<closure>",
            "{FP}: identity lambda must render `<closure>` under M5, got: {v:?}"
        ),
        TurnResult::Error(e) => panic!("{FP}: lambda parse errored: {e}"),
    }
}

/// `r229m1-turn-06`: two calls advance `turn_counter` to 2.
#[test]
fn r229m1_turn_06_counter_increments() {
    const FP: &str = "r229m1-turn-06";
    let mut state = ReplState::new();
    assert_eq!(state.turn_counter, 0, "{FP}: fresh state starts at 0");
    let _ = eval_turn(&mut state, "ls".to_owned());
    assert_eq!(state.turn_counter, 1, "{FP}: after one turn counter must be 1");
    let _ = eval_turn(&mut state, "ls".to_owned());
    assert_eq!(state.turn_counter, 2, "{FP}: after two turns counter must be 2");
}

/// `r229m1-turn-07`: the fingerprint literal matches
/// `repl.turn.[0-9a-f]{16}`.
#[test]
fn r229m1_turn_07_fingerprint_format() {
    const FP: &str = "r229m1-turn-07";
    let mut state = ReplState::new();
    let turn = eval_turn(&mut state, "ls".to_owned());

    let fp = &turn.fingerprint;
    assert!(fp.starts_with("repl.turn."), "{FP}: prefix wrong: {fp}");
    let tail = &fp["repl.turn.".len()..];
    assert_eq!(tail.len(), 16, "{FP}: hex tail must be 16 chars, got {tail}");
    assert!(
        tail.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
        "{FP}: hex tail must be lowercase hex only, got {tail}"
    );
}

/// `r229m1-turn-08`: two sequential turns produce distinct
/// fingerprints (differ in the id suffix).
#[test]
fn r229m1_turn_08_fingerprints_distinct() {
    const FP: &str = "r229m1-turn-08";
    let mut state = ReplState::new();
    let a: ReplTurn = eval_turn(&mut state, "ls".to_owned());
    let b: ReplTurn = eval_turn(&mut state, "ls".to_owned());
    assert_ne!(
        a.fingerprint, b.fingerprint,
        "{FP}: sequential turn fingerprints must differ"
    );
    assert_eq!(
        a.fingerprint, "repl.turn.0000000000000000",
        "{FP}: first turn is id 0"
    );
    assert_eq!(
        b.fingerprint, "repl.turn.0000000000000001",
        "{FP}: second turn is id 1"
    );
}

/// `r229m1-turn-09`: `SessionEdb` survives across turns as an owned
/// field of `ReplState`. Under R229.M2 the datalog executor no longer
/// consults the session overlay (the R226.M9 typed query surface
/// doesn't yet accept one — a follow-on milestone re-wires it), so
/// this test now verifies session-EDB *field survival* rather than
/// session-overlay tuple counting: a fact asserted after turn 1 is
/// still visible on `state.session_edb` after turn 2's datalog block
/// runs. Turn 1's empty block passes typecheck vacuously and renders
/// `dlg: 0 facts`; turn 2's `q(b)` block trips the empty-registry
/// schema check and renders a `typecheck:` error, but neither turn
/// mutates the session overlay.
#[test]
fn r229m1_turn_09_session_edb_preserved() {
    const FP: &str = "r229m1-turn-09";
    let mut state = ReplState::new();

    // Turn 1: an empty program has no atoms to check, so typecheck
    // passes trivially and the run_query_typed dispatches to run_query
    // with an empty program → empty binding set. Renders "dlg: 0 facts".
    let t1 = eval_turn(&mut state, "datalog { }".to_owned());
    match &t1.result {
        TurnResult::Value(v) => assert!(
            v.contains("0 facts"),
            "{FP}: turn 1 empty block must render '0 facts', got: {v:?}"
        ),
        TurnResult::Error(e) => panic!("{FP}: turn 1 errored: {e}"),
    }

    // Assert programmatically per R226.M8 — this mutates
    // `state.session_edb` directly (the R229.M1 skeleton has no
    // user-typed session-mutation syntax yet; R222 adds it).
    state
        .session_edb
        .assert("p", vec![Value::Ident("a".into())]);
    let session_len_before_t2 = state.session_edb.total_tuple_count();

    // Turn 2: a block declaring `q(b)`; empty-registry schema check
    // rejects the unregistered `q/1`. The executor does not touch
    // `state.session_edb` on any path (M2's typed query surface has
    // no session overlay), so the assertion above survives.
    let t2 = eval_turn(&mut state, "datalog { q(b). }".to_owned());
    match &t2.result {
        TurnResult::Error(e) => assert!(
            e.starts_with("typecheck:"),
            "{FP}: turn 2 must render 'typecheck:' under M2, got: {e:?}"
        ),
        TurnResult::Value(v) => panic!("{FP}: expected typecheck error, got Value: {v}"),
    }
    assert_eq!(
        state.session_edb.total_tuple_count(),
        session_len_before_t2,
        "{FP}: session EDB must survive a turn that touches datalog"
    );
    assert!(
        session_len_before_t2 >= 1,
        "{FP}: sanity: session EDB should carry the p(a) we asserted"
    );
}

/// `r229m1-turn-10`: sanity — ten turns don't leak state, and the
/// counter reaches 10.
#[test]
fn r229m1_turn_10_ten_turn_sanity() {
    const FP: &str = "r229m1-turn-10";
    let mut state = ReplState::new();
    for i in 0..10 {
        let turn = eval_turn(&mut state, format!("ls {i}"));
        // Fingerprint id is the pre-increment counter value.
        assert_eq!(
            turn.fingerprint,
            format!("repl.turn.{:016x}", i),
            "{FP}: iteration {i} fingerprint mismatch"
        );
    }
    assert_eq!(state.turn_counter, 10, "{FP}: ten turns must advance counter to 10");
}
