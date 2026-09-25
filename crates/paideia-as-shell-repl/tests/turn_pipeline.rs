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

/// `r229m1-turn-02`: `datalog { p(a). }` runs the Datalog branch and
/// renders the `dlg:` prefix.
#[test]
fn r229m1_turn_02_datalog_block() {
    const FP: &str = "r229m1-turn-02";
    let mut state = ReplState::new();
    let turn = eval_turn(&mut state, "datalog { p(a). }".to_owned());

    match turn.result {
        TurnResult::Value(v) => {
            assert!(
                v.starts_with("dlg:"),
                "{FP}: datalog branch must render 'dlg:' prefix, got: {v:?}"
            );
            assert!(
                v.contains("facts loaded"),
                "{FP}: expected 'facts loaded' text, got: {v:?}"
            );
        }
        TurnResult::Error(e) => panic!("{FP}: datalog block failed: {e}"),
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

/// `r229m1-turn-04`: `ls | wc` dispatches to the Cmd/Pipe stub
/// branch.
#[test]
fn r229m1_turn_04_pipeline_stub() {
    const FP: &str = "r229m1-turn-04";
    let mut state = ReplState::new();
    let turn = eval_turn(&mut state, "ls | wc".to_owned());

    match turn.result {
        TurnResult::Value(v) => assert!(
            v.contains("not yet implemented"),
            "{FP}: pipeline stub must say 'not yet implemented', got: {v:?}"
        ),
        TurnResult::Error(e) => panic!("{FP}: pipeline parse errored: {e}"),
    }
}

/// `r229m1-turn-05`: `{ |x| x }` — a lambda literal — dispatches to
/// the Lambda stub branch.
#[test]
fn r229m1_turn_05_lambda_stub() {
    const FP: &str = "r229m1-turn-05";
    let mut state = ReplState::new();
    let turn = eval_turn(&mut state, "{ |x| x }".to_owned());

    match turn.result {
        TurnResult::Value(v) => assert!(
            v.starts_with("lambda:") && v.contains("not yet implemented"),
            "{FP}: lambda stub must be prefixed 'lambda:' and mention 'not yet implemented', got: {v:?}"
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

/// `r229m1-turn-09`: session EDB survives across turns. Assert a fact
/// programmatically after turn 1; turn 2's datalog block should see it
/// in `total_tuple_count`. We assert one fact after the first turn, so
/// the second turn's fixpoint count must be at least 2 (its own `q(b)`
/// plus the session-EDB `p(a)`).
#[test]
fn r229m1_turn_09_session_edb_preserved() {
    const FP: &str = "r229m1-turn-09";
    let mut state = ReplState::new();

    // Turn 1: an empty program's fixpoint yields only the session-EDB
    // overlay. Session is empty here, so 0 facts.
    let t1 = eval_turn(&mut state, "datalog { }".to_owned());
    match &t1.result {
        TurnResult::Value(v) => assert!(
            v.contains("0 facts"),
            "{FP}: turn 1 with empty session must render '0 facts', got: {v:?}"
        ),
        TurnResult::Error(e) => panic!("{FP}: turn 1 errored: {e}"),
    }

    // Assert programmatically per R226.M8 (parser-level `assert` /
    // `retract` REPL commands land in R222; the R229.M1 skeleton has
    // no user-typed session-mutation syntax yet).
    state
        .session_edb
        .assert("p", vec![Value::Ident("a".into())]);

    // Turn 2: a block declaring q(b); the merged EDB carries `p(a)`
    // from session plus `q(b)` from the block: 2 total.
    let t2 = eval_turn(&mut state, "datalog { q(b). }".to_owned());
    match &t2.result {
        TurnResult::Value(v) => assert!(
            v.contains("2 facts"),
            "{FP}: turn 2 with session overlay must render '2 facts loaded', got: {v:?}"
        ),
        TurnResult::Error(e) => panic!("{FP}: turn 2 errored: {e}"),
    }
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
