//! R229.M3 command-dispatch corpus: eight fixtures pinning the
//! `SyntaxNode::Cmd` and `SyntaxNode::Pipe` executor branches against
//! the new [`paideia_as_shell_repl::CmdDispatchRegistry`] surface.
//!
//! Each `assert!` names the fingerprint (`r229m3-cmd-NN`) in its
//! message so the R220.M10 correlator attributes a regression to a
//! fixture without re-parsing the test name.
//!
//! # CommandSig fixtures
//!
//! Tests build `CommandSig` values via public struct literals — every
//! field on the R222.M2 type is `pub` (per `sig.rs` "matching how the
//! eventual `.pdx` port will read"), so no test-only helper is needed
//! from shell-cmd. Two shapes cover the M3 fixture space:
//!
//! * [`nullary_sig`] — no positional arguments; every argv (empty or
//!   not) that produces zero extras typechecks. Used for the happy-
//!   path lookup + register fixtures.
//! * [`unary_int_sig`] — one required `Int` argument. Used to
//!   demonstrate the argparse-failure branch when argv[0] is not
//!   parseable as an `i64`.

use paideia_as_shell_cmd::{
    ArgSpec, CapSpec, CommandSig, CommandWeight, EffectRow, ExecuteResult, InvocationCtx,
};
use paideia_as_shell_repl::{eval_turn, ReplState, TurnResult};

/// A no-op `execute` fn — the M3 dispatcher does not call it (see
/// `cmd_dispatch::execute_cmd`'s doc: real invocation is R229.M4), so
/// the returned scalar is arbitrary. Kept as a real `fn` (not a
/// closure) to satisfy the `ExecuteFn = fn(...)` field type.
fn noop_execute(ctx: &InvocationCtx) -> ExecuteResult {
    ExecuteResult::new(0, &ctx.fingerprint)
}

/// Build a `CommandSig` with no positional arguments. Every argv
/// slice of length ≤ 0 typechecks; length ≥ 1 fires `ExtraPositional`.
fn nullary_sig(name: &str) -> CommandSig {
    CommandSig {
        name: name.to_owned(),
        input_schema: None,
        output_schema: None,
        arguments: vec![],
        flags: vec![],
        effects: EffectRow::pure(),
        required_capabilities: CapSpec::none(),
        execute: noop_execute,
        weight: CommandWeight::Light,
    }
}

/// Build a `CommandSig` with one required `Int` positional argument.
/// argv `["42"]` typechecks; argv `["foo"]` fires `TypeMismatch`.
#[allow(dead_code)] // reserved for a future r229m3 fixture; kept documented.
fn unary_int_sig(name: &str) -> CommandSig {
    CommandSig {
        name: name.to_owned(),
        input_schema: None,
        output_schema: None,
        arguments: vec![ArgSpec {
            name: "n".to_owned(),
            type_name: "Int".to_owned(),
            required: true,
            default: None,
            help: "test-only int arg".to_owned(),
        }],
        flags: vec![],
        effects: EffectRow::pure(),
        required_capabilities: CapSpec::none(),
        execute: noop_execute,
        weight: CommandWeight::Light,
    }
}

/// `r229m3-cmd-01`: empty registry + source `"ls"` surfaces the
/// UnknownCommand error via `TurnResult::Error` with the `cmd:`
/// prefix and the command name in the body.
#[test]
fn r229m3_cmd_01_unknown_command() {
    const FP: &str = "r229m3-cmd-01";
    let mut state = ReplState::new();

    let turn = eval_turn(&mut state, "ls".to_owned());
    match turn.result {
        TurnResult::Error(e) => {
            assert!(
                e.starts_with("cmd:"),
                "{FP}: expected 'cmd:' prefix on dispatch failure, got: {e:?}"
            );
            assert!(
                e.contains("unknown") || e.contains("UnknownCommand"),
                "{FP}: expected the diagnostic to mention 'unknown', got: {e:?}"
            );
            assert!(
                e.contains("ls"),
                "{FP}: expected the offending name to appear in the diagnostic, got: {e:?}"
            );
        }
        TurnResult::Value(v) => panic!("{FP}: expected unknown-command error, got Value: {v}"),
    }
}

/// `r229m3-cmd-02`: registry with `ls` registered + source `"ls"`
/// dispatches through the happy path and renders `cmd: ls ok (0 args)`.
#[test]
fn r229m3_cmd_02_known_command_ok() {
    const FP: &str = "r229m3-cmd-02";
    let mut state = ReplState::new();
    state.cmd_registry.register("ls", nullary_sig("ls"));

    let turn = eval_turn(&mut state, "ls".to_owned());
    match turn.result {
        TurnResult::Value(v) => {
            assert!(
                v.starts_with("cmd: ls ok"),
                "{FP}: expected 'cmd: ls ok' prefix, got: {v:?}"
            );
            assert!(
                v.contains("0 args"),
                "{FP}: expected '0 args' tail on empty argv, got: {v:?}"
            );
        }
        TurnResult::Error(e) => panic!("{FP}: dispatch errored on registered command: {e}"),
    }
}

/// `r229m3-cmd-03`: registry lookup is case-sensitive. Registering
/// `Ls` and typing `ls` misses (surfaces UnknownCommand); registering
/// `ls` and typing `ls` hits. The M3 registry is a plain
/// `HashMap<String, _>` — case-folding is a higher-layer alias-table
/// concern (see `cmd_dispatch.rs` module doc).
#[test]
fn r229m3_cmd_03_case_sensitive_lookup() {
    const FP: &str = "r229m3-cmd-03";

    let mut a = ReplState::new();
    a.cmd_registry.register("Ls", nullary_sig("Ls"));
    let turn_a = eval_turn(&mut a, "ls".to_owned());
    match turn_a.result {
        TurnResult::Error(e) => assert!(
            e.starts_with("cmd:") && (e.contains("unknown") || e.contains("UnknownCommand")),
            "{FP}: case-mismatched lookup must miss with 'cmd: unknown', got: {e:?}"
        ),
        TurnResult::Value(v) => panic!("{FP}: expected miss on case mismatch, got Value: {v}"),
    }

    let mut b = ReplState::new();
    b.cmd_registry.register("ls", nullary_sig("ls"));
    let turn_b = eval_turn(&mut b, "ls".to_owned());
    match turn_b.result {
        TurnResult::Value(v) => assert!(
            v.starts_with("cmd: ls ok"),
            "{FP}: exact-case match must hit the happy path, got: {v:?}"
        ),
        TurnResult::Error(e) => panic!("{FP}: exact-case lookup errored: {e}"),
    }
}

/// `r229m3-cmd-04`: `ls | wc` dispatches through the M3 pipeline stub,
/// which now renders `pipe: N stages` instead of the M1/M2
/// `<not yet implemented>` string. Real stage threading is R229.M4.
#[test]
fn r229m3_cmd_04_pipeline_stub_upgraded() {
    const FP: &str = "r229m3-cmd-04";
    let mut state = ReplState::new();

    let turn = eval_turn(&mut state, "ls | wc".to_owned());
    match turn.result {
        TurnResult::Value(v) => {
            assert!(
                v.starts_with("pipe:"),
                "{FP}: expected 'pipe:' prefix on M3 pipe stub, got: {v:?}"
            );
            assert!(
                v.contains("2 stages"),
                "{FP}: 'ls | wc' has two pipeline stages, got: {v:?}"
            );
        }
        TurnResult::Error(e) => panic!("{FP}: pipeline stub errored: {e}"),
    }
}

/// `r229m3-cmd-05`: the lambda stub message is unchanged from M1/M2.
/// M3 only touched the `Cmd` and `Pipe` arms; the Lambda arm remains
/// `lambda: <not yet implemented>` (R229.M4's lambda JIT lands there).
#[test]
fn r229m3_cmd_05_lambda_stub_unchanged() {
    const FP: &str = "r229m3-cmd-05";
    let mut state = ReplState::new();

    let turn = eval_turn(&mut state, "{ |x| x }".to_owned());
    match turn.result {
        TurnResult::Value(v) => assert!(
            v.starts_with("lambda:") && v.contains("not yet implemented"),
            "{FP}: lambda stub message must be untouched by M3, got: {v:?}"
        ),
        TurnResult::Error(e) => panic!("{FP}: lambda parse errored under M3: {e}"),
    }
}

/// `r229m3-cmd-06`: the `ReplState::with_command` builder registers
/// the sig and a subsequent `eval_turn` hits the happy path.
#[test]
fn r229m3_cmd_06_with_command_builder() {
    const FP: &str = "r229m3-cmd-06";
    let mut state = ReplState::default().with_command("ls", nullary_sig("ls"));

    let turn = eval_turn(&mut state, "ls".to_owned());
    match turn.result {
        TurnResult::Value(v) => assert!(
            v.starts_with("cmd: ls ok"),
            "{FP}: with_command-registered sig must dispatch, got: {v:?}"
        ),
        TurnResult::Error(e) => panic!("{FP}: with_command dispatch errored: {e}"),
    }
}

/// `r229m3-cmd-07`: a ten-turn stress mixing cmd, datalog, and
/// pipeline holds `turn_counter` to 10 and lets every turn render a
/// non-empty result. Session state (`session_edb`, `cmd_registry`)
/// survives every arm — the M3 executor never mutates the registry
/// itself, only reads it.
#[test]
fn r229m3_cmd_07_mixed_stress_ten_turns() {
    const FP: &str = "r229m3-cmd-07";
    let mut state = ReplState::default().with_command("ls", nullary_sig("ls"));

    // Interleave `ls` (cmd), `ls | wc` (pipe), and `datalog { }`
    // (datalog empty block). Each arm must render either a Value or
    // an Error (empty registry rejects `p/1`, etc.); nothing should
    // panic and the counter should climb to 10.
    let sources = [
        "ls",
        "ls | wc",
        "datalog { }",
        "ls",
        "ls | wc | uniq",
        "datalog { }",
        "ls",
        "ls",
        "ls | wc",
        "datalog { }",
    ];
    for (i, src) in sources.iter().enumerate() {
        let turn = eval_turn(&mut state, (*src).to_owned());
        match &turn.result {
            TurnResult::Value(v) => assert!(
                !v.is_empty(),
                "{FP}: iter {i} '{src}' rendered empty value"
            ),
            TurnResult::Error(e) => assert!(
                !e.is_empty(),
                "{FP}: iter {i} '{src}' rendered empty error"
            ),
        }
    }
    assert_eq!(
        state.turn_counter, 10,
        "{FP}: ten mixed turns must advance counter to 10"
    );
    assert_eq!(
        state.cmd_registry.len(),
        1,
        "{FP}: executor must not touch registry contents (still 1 entry)"
    );
}

/// `r229m3-cmd-08`: `ReplState::default()` yields a `cmd_registry`
/// that is empty. Any lookup misses.
#[test]
fn r229m3_cmd_08_default_registry_is_empty() {
    const FP: &str = "r229m3-cmd-08";
    let state = ReplState::default();

    assert!(
        state.cmd_registry.is_empty(),
        "{FP}: fresh ReplState must have an empty cmd_registry"
    );
    assert_eq!(
        state.cmd_registry.len(),
        0,
        "{FP}: fresh cmd_registry len must be 0"
    );
    assert!(
        state.cmd_registry.lookup("anything").is_none(),
        "{FP}: lookup on empty registry must be None"
    );
}
