//! R229.M8 command typed-`Value` return corpus: eight fixtures pinning
//! [`paideia_as_shell_repl::cmd_dispatch::execute_cmd`]'s new
//! `Result<Value, CmdError>` return shape, the two M8 demonstrator
//! commands (`count` → `Value::Int`, `echo` → `Value::Str`), and the
//! pipeline runner's typed-value threading now that the M7
//! `Value::Str(rendered)` lift has moved into the dispatcher.
//!
//! Each `assert!` names the fixture (`r229m8-cmd-NN`) in its message so
//! the R220.M10 correlator attributes a regression without re-parsing
//! the test name.
//!
//! # Why a separate file from `turn_cmd.rs` and `turn_pipe_typed.rs`
//!
//! `turn_cmd.rs` (M3) pins the string-return dispatcher contract
//! against the pre-M8 shape and stays green under M8 because the
//! generic (non-`count`/non-`echo`) path still produces the same
//! `cmd: <name> ok (K args)` payload, wrapped as `Value::Str` and
//! rendered back by the same [`crate::turn::render_value`] the M8 Cmd
//! arm feeds through. `turn_pipe_typed.rs` (M7) pins the "every stage
//! value is Value::Str" baseline against fixtures registered under
//! non-demonstrator names (`a`, `b`, `c`, `d`); M8 keeps those tests
//! green by construction. Keeping the M8 fixtures in their own file
//! keeps the fingerprint namespace clean (`r229m8-cmd-*`) and lets a
//! follow-on milestone that grows `CommandSig::execute` to emit typed
//! values directly touch only this file's demonstrator special-case
//! pins without disturbing the M3/M7 corpora.
//!
//! # CommandSig fixture shapes
//!
//! Reused pattern from the M3 and M7 corpora (`nullary_sig`,
//! `unary_str_sig`). Two new helpers cover the M8 demonstrator space:
//!
//! * [`binary_str_sig`] — two required `String` positionals. Used to
//!   register `echo` with a shape that accepts exactly the argv the
//!   M8 corpus threads (two-arg `["hello", "world"]` at fixture 2, or
//!   two-arg `["3"]` after a 1→2 pipe boundary at fixture 7).
//! * [`ternary_str_sig`] — three required `String` positionals. Used
//!   to register `count` for fixture 1's three-arg direct call.
//!
//! Both helpers spell `String` for every positional; M8's dispatcher
//! special-cases on `name` (not on `sig`), so a sig with any accepted-
//! argv shape produces the correct `Value::Int` / `Value::Str` payload
//! — the argparse pre-check only gates *whether* the payload is
//! computed, not *what* the payload is.

use paideia_as_shell_ast::{Context, NodeSpan, SyntaxNode};
use paideia_as_shell_cmd::{
    ArgSpec, CapSpec, CommandSig, CommandWeight, EffectRow, ExecuteResult, InvocationCtx,
};
use paideia_as_shell_repl::{
    execute_cmd, execute_pipeline, eval_turn, CmdError, ReplState, TurnResult, Value,
};

/// No-op `execute` fn — R229.M8's dispatcher still does not call it
/// (the M8 demonstrator payload is chosen by the dispatcher itself,
/// not by the sig's `execute` op — a follow-on milestone that grows
/// `CommandSig::execute` to return a typed `Value` will start driving
/// it). Kept as a real `fn` to satisfy the `ExecuteFn = fn(...)` field
/// type.
fn noop_execute(ctx: &InvocationCtx) -> ExecuteResult {
    ExecuteResult::new(0, &ctx.fingerprint)
}

/// A `CommandSig` with no positional arguments — matches argv of
/// length 0 only (any non-empty argv fires `ExtraPositional`).
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

/// A `CommandSig` with one required `String` positional — matches an
/// argv of length exactly 1.
fn unary_str_sig(name: &str) -> CommandSig {
    CommandSig {
        name: name.to_owned(),
        input_schema: None,
        output_schema: None,
        arguments: vec![ArgSpec {
            name: "s".to_owned(),
            type_name: "String".to_owned(),
            required: true,
            default: None,
            help: "M8 corpus: single required String positional".to_owned(),
        }],
        flags: vec![],
        effects: EffectRow::pure(),
        required_capabilities: CapSpec::none(),
        execute: noop_execute,
        weight: CommandWeight::Light,
    }
}

/// A `CommandSig` with two required `String` positionals — matches an
/// argv of length exactly 2. Used to register `echo` in the fixtures
/// where the argv shape is `["a", "b"]` (direct call at fixture 2)
/// and `["<threaded>", "<explicit>"]` (pipeline boundary at fixture 7).
fn binary_str_sig(name: &str) -> CommandSig {
    CommandSig {
        name: name.to_owned(),
        input_schema: None,
        output_schema: None,
        arguments: vec![
            ArgSpec {
                name: "a".to_owned(),
                type_name: "String".to_owned(),
                required: true,
                default: None,
                help: "M8 corpus: first required String positional".to_owned(),
            },
            ArgSpec {
                name: "b".to_owned(),
                type_name: "String".to_owned(),
                required: true,
                default: None,
                help: "M8 corpus: second required String positional".to_owned(),
            },
        ],
        flags: vec![],
        effects: EffectRow::pure(),
        required_capabilities: CapSpec::none(),
        execute: noop_execute,
        weight: CommandWeight::Light,
    }
}

/// A `CommandSig` with three required `String` positionals — matches
/// an argv of length exactly 3. Used to register `count` at fixture 1
/// where the direct call carries argv `["a", "b", "c"]`.
fn ternary_str_sig(name: &str) -> CommandSig {
    CommandSig {
        name: name.to_owned(),
        input_schema: None,
        output_schema: None,
        arguments: vec![
            ArgSpec {
                name: "x".to_owned(),
                type_name: "String".to_owned(),
                required: true,
                default: None,
                help: "M8 corpus: first required String positional".to_owned(),
            },
            ArgSpec {
                name: "y".to_owned(),
                type_name: "String".to_owned(),
                required: true,
                default: None,
                help: "M8 corpus: second required String positional".to_owned(),
            },
            ArgSpec {
                name: "z".to_owned(),
                type_name: "String".to_owned(),
                required: true,
                default: None,
                help: "M8 corpus: third required String positional".to_owned(),
            },
        ],
        flags: vec![],
        effects: EffectRow::pure(),
        required_capabilities: CapSpec::none(),
        execute: noop_execute,
        weight: CommandWeight::Light,
    }
}

/// Build a bare-name `Cmd` node with the given name and (optional)
/// pipeline-context argv. Same helper shape as the M4/M7 corpora.
fn cmd_node(name: &str, args: Vec<SyntaxNode>) -> SyntaxNode {
    let span = NodeSpan::synthetic(Context::Pipeline);
    SyntaxNode::Cmd {
        name: Box::new(SyntaxNode::Ident {
            name: name.to_owned(),
            span,
        }),
        args,
        span,
    }
}

/// Build an `Ident` node (a pipeline-context bare word) for use as a
/// `Cmd`'s argv token.
fn ident(name: &str) -> SyntaxNode {
    let span = NodeSpan::synthetic(Context::Pipeline);
    SyntaxNode::Ident {
        name: name.to_owned(),
        span,
    }
}

/// `r229m8-cmd-01`: `execute_cmd(reg, "count", &["a", "b", "c"])`
/// returns `Ok(Value::Int(3))` — the M8 dispatcher's `count` demo
/// short-circuits the generic tag and emits the argv length as a
/// typed integer. The sig registered under `count` accepts three
/// `String` positionals so the argparse pre-check does not veto the
/// payload; the demonstrator overrides the generic tag but the sig
/// contract still runs first.
#[test]
fn r229m8_cmd_01_count_returns_value_int() {
    const FP: &str = "r229m8-cmd-01";
    let mut state = ReplState::new();
    state.cmd_registry.register("count", ternary_str_sig("count"));

    let argv = [
        "a".to_owned(),
        "b".to_owned(),
        "c".to_owned(),
    ];
    let result = execute_cmd(&state.cmd_registry, "count", &argv);
    match result {
        Ok(Value::Int(n)) => assert_eq!(
            n, 3,
            "{FP}: `count` with 3 args must return `Value::Int(3)`, got Int({n})"
        ),
        Ok(other) => panic!(
            "{FP}: `count` demonstrator must return `Value::Int`, got: {other:?}"
        ),
        Err(e) => panic!("{FP}: registered `count` must dispatch, got error: {e}"),
    }
}

/// `r229m8-cmd-02`: `execute_cmd(reg, "echo", &["hello", "world"])`
/// returns `Ok(Value::Str("hello world"))` — the M8 dispatcher's
/// `echo` demo joins the argv tokens with a single ASCII space and
/// emits the joined payload as a typed string.
#[test]
fn r229m8_cmd_02_echo_returns_value_str_joined() {
    const FP: &str = "r229m8-cmd-02";
    let mut state = ReplState::new();
    state.cmd_registry.register("echo", binary_str_sig("echo"));

    let argv = ["hello".to_owned(), "world".to_owned()];
    let result = execute_cmd(&state.cmd_registry, "echo", &argv);
    match result {
        Ok(Value::Str(s)) => assert_eq!(
            s, "hello world",
            "{FP}: `echo` must join argv with single space, got: {s:?}"
        ),
        Ok(other) => panic!(
            "{FP}: `echo` demonstrator must return `Value::Str`, got: {other:?}"
        ),
        Err(e) => panic!("{FP}: registered `echo` must dispatch, got error: {e}"),
    }
}

/// `r229m8-cmd-03`: unregistered command still surfaces the M3-shape
/// `CmdError::UnknownCommand` error — the M8 typed-return migration
/// does not change the error path, only the success payload. Pins
/// the invariant that a `Result<Value, CmdError>` return does not
/// swallow the discriminant a diagnostic layer needs (see
/// `cmd_dispatch::CmdError` doc for the discriminant contract).
#[test]
fn r229m8_cmd_03_unknown_still_errors() {
    const FP: &str = "r229m8-cmd-03";
    let state = ReplState::new();

    let result = execute_cmd(&state.cmd_registry, "nosuchcmd", &[]);
    match result {
        Err(CmdError::UnknownCommand(name)) => assert_eq!(
            name, "nosuchcmd",
            "{FP}: UnknownCommand payload must carry the offending name, got: {name:?}"
        ),
        Err(other) => panic!(
            "{FP}: expected UnknownCommand, got other error: {other:?}"
        ),
        Ok(v) => panic!(
            "{FP}: unregistered command must not dispatch, got Value: {v:?}"
        ),
    }
}

/// `r229m8-cmd-04`: end-to-end pipeline `count | echo` (via
/// `execute_pipeline` on hand-built AST) — stage 0 (`count`,
/// registered nullary, 0 args) emits `Value::Int(0)`; stage 1 (`echo`,
/// registered with one String positional to accept the threaded arg)
/// receives argv `["0"]` and emits `Value::Str("0")`. Pins the typed-
/// through-typed threading path: the runner threads a `Value::Int`
/// across the 0→1 boundary (not a `Value::Str` wrap of the render
/// tag) and the next stage's payload reflects the projected string.
#[test]
fn r229m8_cmd_04_pipeline_int_then_str() {
    const FP: &str = "r229m8-cmd-04";
    let mut state = ReplState::new();
    state.cmd_registry.register("count", nullary_sig("count"));
    state.cmd_registry.register("echo", unary_str_sig("echo"));

    let stages = [cmd_node("count", vec![]), cmd_node("echo", vec![])];
    let refs: Vec<&SyntaxNode> = stages.iter().collect();
    let result =
        execute_pipeline(&state, &refs).expect("both stages must dispatch");

    assert_eq!(
        result.halted_at, None,
        "{FP}: full-success pipeline must have `halted_at == None`, got halt: {:?}",
        result.halt_reason
    );
    assert_eq!(
        result.stage_values.len(),
        2,
        "{FP}: two stages completed, `stage_values.len()` must be 2, got: {}",
        result.stage_values.len()
    );
    match &result.stage_values[0] {
        Value::Int(n) => assert_eq!(
            *n, 0,
            "{FP}: stage 0 (`count` with 0 args) must be `Value::Int(0)`, got Int({n})"
        ),
        other => panic!(
            "{FP}: stage 0 must be `Value::Int` (M8 demonstrator return), got: {other:?}"
        ),
    }
    match &result.stage_values[1] {
        Value::Str(s) => assert_eq!(
            s, "0",
            "{FP}: stage 1 (`echo` receiving the projected \"0\") must be \
             `Value::Str(\"0\")`, got Str({s:?})"
        ),
        other => panic!(
            "{FP}: stage 1 must be `Value::Str` (echo joins its argv), got: {other:?}"
        ),
    }
}

/// `r229m8-cmd-05`: `eval_turn("count", state)` with `count` registered
/// nullary renders through the shared [`crate::turn::render_value`]
/// surface and produces `TurnResult::Value("0")` — the M8 Cmd arm now
/// projects the typed dispatcher return through `render_value`, and
/// `Value::Int(0)` renders as `"0"`. Pins the "typed return reaches
/// the user through the same rendering surface as Lambda / Let" cross-
/// module invariant.
#[test]
fn r229m8_cmd_05_eval_turn_renders_int_as_digits() {
    const FP: &str = "r229m8-cmd-05";
    let mut state = ReplState::new();
    state.cmd_registry.register("count", nullary_sig("count"));

    let turn = eval_turn(&mut state, "count".to_owned());
    match turn.result {
        TurnResult::Value(v) => assert_eq!(
            v, "0",
            "{FP}: `eval_turn(\"count\")` must render `Value::Int(0)` as \"0\", got: {v:?}"
        ),
        TurnResult::Error(e) => panic!(
            "{FP}: registered `count` must dispatch through eval_turn, got error: {e}"
        ),
    }
}

/// `r229m8-cmd-06`: a generic (non-`count`, non-`echo`) command
/// dispatched through `execute_cmd` still returns
/// `Value::Str("cmd: <name> ok (K args)")` — the M8 demonstrator
/// special-case is name-keyed and does not perturb the generic path.
/// Pins the M3/M4/M7 corpora's substring invariant (`starts_with("cmd:
/// foo ok")`) at the typed-return layer.
#[test]
fn r229m8_cmd_06_generic_still_value_str() {
    const FP: &str = "r229m8-cmd-06";
    let mut state = ReplState::new();
    state.cmd_registry.register("foo", nullary_sig("foo"));

    let result = execute_cmd(&state.cmd_registry, "foo", &[]);
    match result {
        Ok(Value::Str(s)) => assert!(
            s.starts_with("cmd: foo ok") && s.contains("0 args"),
            "{FP}: generic dispatcher must return the M3 render as `Value::Str`, got: {s:?}"
        ),
        Ok(other) => panic!(
            "{FP}: non-demonstrator command must return `Value::Str`, got: {other:?}"
        ),
        Err(e) => panic!("{FP}: registered `foo` must dispatch, got error: {e}"),
    }
}

/// `r229m8-cmd-07`: pipeline `count(3 args) | echo(1 arg)` — direct
/// AST build with `count` carrying three argv tokens at stage 0.
/// Stage 0 emits `Value::Int(3)`; the runner projects it back into an
/// argv token (`"3"`) via [`crate::pipeline`]'s `value_to_arg_string`
/// and prepends it to stage 1's argv. Stage 1 (`echo` registered
/// unary-String) then receives argv `["3"]` and emits `Value::Str("3")`.
/// Pins the typed→string projection at the stage boundary: a typed
/// `Value::Int` on the wire lands as its decimal on the next stage's
/// argv, which the next stage then observes verbatim.
#[test]
fn r229m8_cmd_07_pipeline_int_projects_to_argv() {
    const FP: &str = "r229m8-cmd-07";
    let mut state = ReplState::new();
    state.cmd_registry.register("count", ternary_str_sig("count"));
    state.cmd_registry.register("echo", unary_str_sig("echo"));

    let stages = [
        cmd_node("count", vec![ident("a"), ident("b"), ident("c")]),
        cmd_node("echo", vec![]),
    ];
    let refs: Vec<&SyntaxNode> = stages.iter().collect();
    let result =
        execute_pipeline(&state, &refs).expect("both stages must dispatch");

    assert_eq!(
        result.halted_at, None,
        "{FP}: full-success pipeline must have `halted_at == None`, got halt: {:?}",
        result.halt_reason
    );
    match &result.stage_values[0] {
        Value::Int(n) => assert_eq!(
            *n, 3,
            "{FP}: stage 0 (`count` with 3 args) must be `Value::Int(3)`, got Int({n})"
        ),
        other => panic!(
            "{FP}: stage 0 must be `Value::Int` (count demonstrator), got: {other:?}"
        ),
    }
    // The projection through value_to_arg_string is not directly
    // observable from PipelineResult (it lives inside the runner's
    // loop), but the observable *consequence* is that stage 1's echo
    // — which joins its argv with " " — carries the projected "3".
    match &result.stage_values[1] {
        Value::Str(s) => assert_eq!(
            s, "3",
            "{FP}: stage 1 (echo of the projected \"3\") must be `Value::Str(\"3\")`, \
             got Str({s:?})"
        ),
        other => panic!(
            "{FP}: stage 1 must be `Value::Str` (echo joins its argv), got: {other:?}"
        ),
    }
    // Sanity: `stage_outputs` mirrors the render_value projection so a
    // caller inspecting the parallel `String` vector sees the same "3"
    // that the typed `Value::Str` carries — pins the M8 shift from
    // "runner supplies stage_outputs from execute_cmd's String return"
    // to "runner derives stage_outputs from stage_values via
    // render_value".
    assert_eq!(
        result.stage_outputs[0], "3",
        "{FP}: `stage_outputs[0]` must be `render_value(&Value::Int(3))` = \"3\", got: {:?}",
        result.stage_outputs[0]
    );
    assert_eq!(
        result.stage_outputs[1], "3",
        "{FP}: `stage_outputs[1]` must mirror the Value::Str payload, got: {:?}",
        result.stage_outputs[1]
    );
}

/// `r229m8-cmd-08`: `eval_turn` still bumps `state.turn_counter` by
/// exactly one per call, regardless of whether the dispatched command
/// produced a typed `Value::Int` / `Value::Str` / generic tag or
/// errored. The M8 typed-return migration must not perturb the R229
/// counter contract that the replay harness depends on.
#[test]
fn r229m8_cmd_08_turn_counter_advances_by_one_per_turn() {
    const FP: &str = "r229m8-cmd-08";
    let mut state = ReplState::new();
    state.cmd_registry.register("count", nullary_sig("count"));
    state.cmd_registry.register("echo", unary_str_sig("echo"));
    state.cmd_registry.register("foo", nullary_sig("foo"));

    assert_eq!(
        state.turn_counter, 0,
        "{FP}: fresh state must have turn_counter == 0"
    );

    // Turn 1: typed Int-return (count → Value::Int(0)).
    let _ = eval_turn(&mut state, "count".to_owned());
    assert_eq!(
        state.turn_counter, 1,
        "{FP}: Value::Int return must bump counter by 1 (got {})",
        state.turn_counter
    );

    // Turn 2: generic tag (foo → Value::Str("cmd: foo ok (0 args)")).
    let _ = eval_turn(&mut state, "foo".to_owned());
    assert_eq!(
        state.turn_counter, 2,
        "{FP}: generic Value::Str return must bump counter by 1 (got {})",
        state.turn_counter
    );

    // Turn 3: pipeline `count | echo` (Int → Str across a boundary).
    let _ = eval_turn(&mut state, "count | echo".to_owned());
    assert_eq!(
        state.turn_counter, 3,
        "{FP}: pipeline is one turn — must bump counter by 1, not per stage (got {})",
        state.turn_counter
    );

    // Turn 4: an unregistered command (drives the CmdError path).
    let _ = eval_turn(&mut state, "nosuchcmd".to_owned());
    assert_eq!(
        state.turn_counter, 4,
        "{FP}: error turns must still bump counter — no fingerprint gaps (got {})",
        state.turn_counter
    );
}
