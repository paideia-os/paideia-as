//! R229.M9 turn-history + replay corpus.
//!
//! Eight fixtures (`r229m9-hist-01` .. `r229m9-hist-08`) pin the shape
//! of the bounded [`paideia_as_shell_repl::TurnHistory`] ring buffer,
//! the field [`paideia_as_shell_repl::ReplState::history`] the M9
//! milestone lands on the session-state struct, the automatic
//! `record` call [`paideia_as_shell_repl::eval_turn`] does on every
//! completed turn, and the [`paideia_as_shell_repl::replay_turn`]
//! primitive.
//!
//! The suite is organised around three invariants:
//!
//! * `state.history.len()` == the number of `eval_turn` calls, unless
//!   capacity has been exceeded (in which case the oldest entries
//!   evict from the front — pinned by fixture 04).
//! * The record inserted for turn `k` matches the value `eval_turn`
//!   returned for turn `k` (fixture 02 pins reference equality by
//!   `PartialEq`; the [`paideia_as_shell_repl::ReplTurn`] `Clone` /
//!   `PartialEq` derives make this direct).
//! * A replay is a real turn: `replay_turn(&mut state, k)` bumps the
//!   counter, appends a *new* entry to the history, and returns the
//!   fresh turn — not the historical one (fixture 05).
//!
//! Every `assert!` names its fingerprint (`r229m9-hist-NN`) in the
//! message so R220.M10's correlator attributes a regression to a
//! fixture without re-parsing the test name.

use paideia_as_shell_repl::{
    eval_turn, replay_turn, ReplState, ReplTurn, TurnHistory, TurnResult,
};

/// `r229m9-hist-01`: fresh `ReplState::new()` starts with an empty
/// history. Also pins the shape of `TurnHistory::default()` (capacity
/// 1000, empty).
#[test]
fn r229m9_hist_01_fresh_state_empty_history() {
    const FP: &str = "r229m9-hist-01";
    let state = ReplState::new();
    assert_eq!(
        state.history.len(),
        0,
        "{FP}: fresh ReplState must have an empty history (got len={})",
        state.history.len()
    );
    assert!(
        state.history.is_empty(),
        "{FP}: is_empty() must be true on a fresh history"
    );
    assert_eq!(
        state.history.capacity(),
        1000,
        "{FP}: default capacity must be 1000 (the R229.M9 heuristic), got {}",
        state.history.capacity()
    );
    assert!(
        state.history.latest().is_none(),
        "{FP}: latest() on an empty history must be None"
    );
    assert!(
        state.history.get(0).is_none(),
        "{FP}: get(0) on an empty history must be None"
    );
}

/// `r229m9-hist-02`: after a single `eval_turn`, the history has one
/// entry and `latest()` equals the returned turn. Pins the "record
/// after the ReplTurn is final" contract.
#[test]
fn r229m9_hist_02_single_eval_records_matching_entry() {
    const FP: &str = "r229m9-hist-02";
    let mut state = ReplState::new();
    let turn: ReplTurn = eval_turn(&mut state, "ls".to_owned());
    assert_eq!(
        state.history.len(),
        1,
        "{FP}: history.len() must be 1 after one eval_turn (got {})",
        state.history.len()
    );
    let latest = state.history.latest().unwrap_or_else(|| {
        panic!("{FP}: latest() must be Some after a recorded turn")
    });
    assert_eq!(
        latest, &turn,
        "{FP}: latest() must equal the ReplTurn eval_turn returned"
    );
    assert_eq!(
        state.history.get(0),
        Some(&turn),
        "{FP}: get(0) on a one-entry history must equal the returned turn"
    );
}

/// `r229m9-hist-03`: three sequential `eval_turn` calls populate
/// history in insertion order. Pins `get(0)` == oldest, `get(2)` ==
/// newest, `latest()` == `get(len-1)`.
#[test]
fn r229m9_hist_03_three_turns_insertion_order() {
    const FP: &str = "r229m9-hist-03";
    let mut state = ReplState::new();
    let a = eval_turn(&mut state, "ls".to_owned());
    let b = eval_turn(&mut state, "wc".to_owned());
    let c = eval_turn(&mut state, "cat".to_owned());
    assert_eq!(
        state.history.len(),
        3,
        "{FP}: three turns must yield history.len() == 3 (got {})",
        state.history.len()
    );
    assert_eq!(state.history.get(0), Some(&a), "{FP}: get(0) must be the oldest turn");
    assert_eq!(state.history.get(1), Some(&b), "{FP}: get(1) must be the middle turn");
    assert_eq!(state.history.get(2), Some(&c), "{FP}: get(2) must be the newest turn");
    assert_eq!(
        state.history.latest(),
        Some(&c),
        "{FP}: latest() must equal the most recently recorded turn"
    );
    assert_eq!(
        state.history.get(state.history.len() - 1),
        state.history.latest(),
        "{FP}: get(len-1) must equal latest()"
    );
}

/// `r229m9-hist-04`: capacity enforcement — a `TurnHistory::new(2)`
/// records three turns and keeps only the last two, oldest evicted
/// from the front. The eviction happens as part of `record`, not on a
/// later `get`.
#[test]
fn r229m9_hist_04_capacity_evicts_oldest() {
    const FP: &str = "r229m9-hist-04";
    let mut state = ReplState::new();
    // Swap in a small-capacity buffer before the first turn — the
    // public field on ReplState is documented as replaceable for
    // exactly this driver-tunes-window scenario.
    state.history = TurnHistory::new(2);
    let _a = eval_turn(&mut state, "ls".to_owned());
    let b = eval_turn(&mut state, "wc".to_owned());
    let c = eval_turn(&mut state, "cat".to_owned());
    assert_eq!(
        state.history.len(),
        2,
        "{FP}: capacity-2 buffer must retain only 2 entries after 3 records (got {})",
        state.history.len()
    );
    assert_eq!(
        state.history.capacity(),
        2,
        "{FP}: capacity must remain 2 (never grows)"
    );
    assert_eq!(
        state.history.get(0),
        Some(&b),
        "{FP}: after eviction, get(0) must be the second recorded turn (was oldest surviving)"
    );
    assert_eq!(
        state.history.get(1),
        Some(&c),
        "{FP}: after eviction, get(1) must be the most recent turn"
    );
    assert!(
        state.history.get(2).is_none(),
        "{FP}: no entry beyond capacity should be readable"
    );
    // Sanity: the session's turn_counter keeps growing past the
    // history's live window — the counter is monotone, the history
    // bounded.
    assert_eq!(
        state.turn_counter, 3,
        "{FP}: turn_counter must still be 3 (monotone; history bound is separate)"
    );
}

/// `r229m9-hist-05`: `replay_turn(&mut state, 0)` returns Some, bumps
/// the counter, and appends a new entry to history. After one initial
/// eval + one replay: turn_counter == 2, history.len() == 2.
#[test]
fn r229m9_hist_05_replay_is_a_fresh_turn() {
    const FP: &str = "r229m9-hist-05";
    let mut state = ReplState::new();
    let original = eval_turn(&mut state, "ls".to_owned());
    assert_eq!(state.turn_counter, 1, "{FP}: after one eval_turn, counter == 1");
    assert_eq!(state.history.len(), 1, "{FP}: after one eval_turn, history.len() == 1");

    let replayed = replay_turn(&mut state, 0)
        .unwrap_or_else(|| panic!("{FP}: replay_turn on a recorded index must be Some"));
    assert_eq!(
        state.turn_counter, 2,
        "{FP}: after replay, counter must be 2 (replay is a real turn)"
    );
    assert_eq!(
        state.history.len(),
        2,
        "{FP}: after replay, history must have 2 entries"
    );
    // The replayed turn has a fresh (larger) fingerprint but the same
    // source string — insertion-order confirms it landed at the back.
    assert_eq!(
        replayed.source, original.source,
        "{FP}: replayed source must match the original recorded source"
    );
    assert_ne!(
        replayed.fingerprint, original.fingerprint,
        "{FP}: replayed fingerprint must differ (new turn id)"
    );
    assert_eq!(
        state.history.latest(),
        Some(&replayed),
        "{FP}: latest() must be the replayed turn"
    );
    assert_eq!(
        state.history.get(0),
        Some(&original),
        "{FP}: original at index 0 must be preserved"
    );
}

/// `r229m9-hist-06`: `replay_turn` on an out-of-range index returns
/// None and mutates nothing (counter and history unchanged).
#[test]
fn r229m9_hist_06_replay_out_of_range_returns_none() {
    const FP: &str = "r229m9-hist-06";
    let mut state = ReplState::new();
    let _ = eval_turn(&mut state, "ls".to_owned());
    let counter_before = state.turn_counter;
    let len_before = state.history.len();
    let result = replay_turn(&mut state, 1000);
    assert!(
        result.is_none(),
        "{FP}: replay_turn on an out-of-range index must return None"
    );
    assert_eq!(
        state.turn_counter, counter_before,
        "{FP}: a None-returning replay must not bump the counter"
    );
    assert_eq!(
        state.history.len(),
        len_before,
        "{FP}: a None-returning replay must not append to history"
    );
}

/// `r229m9-hist-07`: FIFO invariant — for any non-empty history,
/// `get(0)` is the oldest surviving entry and `latest()` ==
/// `get(len-1)`. Exercised across a five-turn sequence (well under the
/// default capacity, so no eviction interferes with the "oldest is
/// index 0" reading).
#[test]
fn r229m9_hist_07_fifo_order_invariant() {
    const FP: &str = "r229m9-hist-07";
    let mut state = ReplState::new();
    let sources = ["ls", "wc", "cat", "ps", "df"];
    let mut turns = Vec::new();
    for src in sources {
        turns.push(eval_turn(&mut state, src.to_owned()));
    }
    assert_eq!(
        state.history.len(),
        5,
        "{FP}: five turns must land in history (got {})",
        state.history.len()
    );
    for (i, expected) in turns.iter().enumerate() {
        assert_eq!(
            state.history.get(i),
            Some(expected),
            "{FP}: get({i}) must equal the {i}-th recorded turn"
        );
    }
    assert_eq!(
        state.history.get(0),
        Some(turns.first().unwrap()),
        "{FP}: get(0) must be the oldest recorded turn"
    );
    assert_eq!(
        state.history.latest(),
        Some(turns.last().unwrap()),
        "{FP}: latest() must be the most recently recorded turn"
    );
    assert_eq!(
        state.history.get(state.history.len() - 1),
        state.history.latest(),
        "{FP}: get(len-1) must equal latest()"
    );
}

/// `r229m9-hist-08`: 10-turn mixed stress across the sub-languages
/// `eval_turn` accepts (cmd, pipe, datalog, lambda, plus a `let`-shape
/// source that fails at the lexer — the parse-error turn still records,
/// which is the M9 "every attempted turn is in insertion order"
/// contract). All 10 attempts land in history.
#[test]
fn r229m9_hist_08_mixed_ten_turn_stress() {
    const FP: &str = "r229m9-hist-08";
    let mut state = ReplState::new();

    // Cmd shapes (dispatch to Cmd arm; unregistered → cmd:… Error, but
    // record still happens).
    let _ = eval_turn(&mut state, "ls".to_owned());
    let _ = eval_turn(&mut state, "wc".to_owned());
    // Pipe shape (dispatches to Pipe arm).
    let _ = eval_turn(&mut state, "ls | wc".to_owned());
    // Datalog block (dispatches to Datalog arm; empty registry → typecheck
    // error, but recorded).
    let _ = eval_turn(&mut state, "datalog { p(a). }".to_owned());
    // Lambda shape (dispatches to Lambda arm, evaluates cleanly).
    let _ = eval_turn(&mut state, "{ 42 }".to_owned());
    let _ = eval_turn(&mut state, "{ |x| x }".to_owned());
    // Bare thunk with a literal string.
    let _ = eval_turn(&mut state, "{ \"hi\" }".to_owned());
    // A `let` source (parse-error under R221.M5 lexer — the record
    // still happens per contract).
    let _ = eval_turn(&mut state, "let x = 42".to_owned());
    // Empty source (generic-fallback arm renders a Value).
    let _ = eval_turn(&mut state, String::new());
    // Another cmd for the tenth.
    let _ = eval_turn(&mut state, "cat".to_owned());

    assert_eq!(
        state.history.len(),
        10,
        "{FP}: ten mixed turns must all record (got {})",
        state.history.len()
    );
    assert_eq!(
        state.turn_counter, 10,
        "{FP}: counter must reach 10 (monotone across the mix)"
    );
    // Insertion order: get(0)..get(9) must project onto the same source
    // sequence, in order.
    let expected_sources = [
        "ls",
        "wc",
        "ls | wc",
        "datalog { p(a). }",
        "{ 42 }",
        "{ |x| x }",
        "{ \"hi\" }",
        "let x = 42",
        "",
        "cat",
    ];
    for (i, expected) in expected_sources.iter().enumerate() {
        let entry = state.history.get(i).unwrap_or_else(|| {
            panic!("{FP}: history.get({i}) must be Some")
        });
        assert_eq!(
            &entry.source, expected,
            "{FP}: history.get({i}).source must match the recorded input"
        );
        // Every entry must carry a well-formed TurnResult (either shape
        // is fine — the M9 contract is "recorded", not "succeeded").
        match &entry.result {
            TurnResult::Value(_) | TurnResult::Error(_) => {}
        }
    }
}
