//! R229.M4 pipeline value-threading corpus: eight fixtures pinning
//! [`paideia_as_shell_repl::pipeline::execute_pipeline`] and the
//! `SyntaxNode::Pipe` arm of [`paideia_as_shell_repl::eval_turn`].
//!
//! Each `assert!` names the fingerprint (`r229m4-pipe-NN`) in its
//! message so the R220.M10 correlator attributes a regression to a
//! fixture without re-parsing the test name.
//!
//! # AST-node fixtures vs. source-string fixtures
//!
//! Some fixtures (1, 5) construct `SyntaxNode::Pipe` / `SyntaxNode::Cmd`
//! literals directly and call `execute_pipeline` bypassing the parser,
//! because the shape they need (an empty pipeline; a mid-stage halt
//! against a specific arg-shape spec) is not naturally produced by
//! typing a string. Other fixtures (7, 8) go through `eval_turn` end-to-
//! end to verify the turn-arm render and the state-mutation invariant.
//! The mix is intentional — the pipeline runner has two callers (the
//! turn executor, and any future R229.M7 replay driver) and both paths
//! should be exercised at fixture altitude.
//!
//! # CommandSig fixtures
//!
//! Reused shape from `tests/turn_cmd.rs`:
//!
//! * [`nullary_sig`] — no positional arguments; typechecks against argv
//!   `[]` and fires `ExtraPositional` on argv `[_, ..]`.
//! * [`unary_str_sig`] — one required `String` positional; typechecks
//!   against argv `[_]` and fires `Missing` on argv `[]`. Used to
//!   verify that the pipeline runner *did* prepend a prior stage's
//!   rendered output as an implicit first arg — a stage whose spec
//!   requires one String will fail argparse if the prepend never
//!   happened.
//! * [`unary_int_sig`] — one required `Int` positional; used by the
//!   mid-stage halt fixture (5) to demonstrate that a prior stage's
//!   rendered `cmd: <name> ok (K args)` string is *not* an `Int` and
//!   thus surfaces a `TypeMismatch` at the correct stage index.

use paideia_as_shell_ast::{Context, NodeSpan, SyntaxNode};
use paideia_as_shell_cmd::{
    ArgSpec, CapSpec, CommandSig, CommandWeight, EffectRow, ExecuteResult, InvocationCtx,
};
use paideia_as_shell_repl::{
    eval_turn, execute_pipeline, PipelineResult, ReplState, TurnResult,
};

/// No-op `execute` fn — R229.M4's stage dispatcher does not call it
/// (see `cmd_dispatch::execute_cmd`'s doc: real invocation is R229.M5+).
fn noop_execute(ctx: &InvocationCtx) -> ExecuteResult {
    ExecuteResult::new(0, &ctx.fingerprint)
}

/// A `CommandSig` with no positional arguments — matches any argv of
/// length 0.
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

/// A `CommandSig` with one required `String` positional — matches any
/// argv of length 1 (parse_scalar's `_ => Value::Str` catch-all takes
/// every non-Int/Bool token).
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
            help: "implicit first arg threaded from the prior stage".to_owned(),
        }],
        flags: vec![],
        effects: EffectRow::pure(),
        required_capabilities: CapSpec::none(),
        execute: noop_execute,
        weight: CommandWeight::Light,
    }
}

/// A `CommandSig` with one required `Int` positional — a prior stage's
/// rendered `cmd: <name> ok (K args)` string cannot parse as an i64, so
/// the pipeline halts at this stage with a `TypeMismatch`.
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
            help: "mid-pipeline halt trigger for r229m4-pipe-05".to_owned(),
        }],
        flags: vec![],
        effects: EffectRow::pure(),
        required_capabilities: CapSpec::none(),
        execute: noop_execute,
        weight: CommandWeight::Light,
    }
}

/// Build a bare-name `Cmd` node with the given name and (optional)
/// pipeline-context argv.
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

/// `r229m4-pipe-01`: empty pipeline — `execute_pipeline(state, &[])`
/// returns the default `PipelineResult`. The parser will not produce a
/// zero-stage `Pipe` (its rhs is always at least one stage), so this
/// fixture calls `execute_pipeline` directly to pin the *contract* of
/// the empty case for any driver that constructs a stage list by
/// hand.
#[test]
fn r229m4_pipe_01_empty_pipeline() {
    const FP: &str = "r229m4-pipe-01";
    let state = ReplState::new();

    let result = execute_pipeline(&state, &[])
        .expect("empty pipeline must not surface a structural halt");
    assert_eq!(
        result,
        PipelineResult::default(),
        "{FP}: empty pipeline must return the default `PipelineResult`"
    );
    assert_eq!(
        result.stage_outputs,
        Vec::<String>::new(),
        "{FP}: no stages ran, so `stage_outputs` must be empty"
    );
    assert_eq!(
        result.final_value, "",
        "{FP}: no stages ran, so `final_value` must be the empty string"
    );
    assert_eq!(
        result.halted_at, None,
        "{FP}: empty pipeline is *not* a halt (there was nothing to halt)"
    );
    assert_eq!(
        result.halt_reason, None,
        "{FP}: `halt_reason` must be None whenever `halted_at` is None"
    );
}

/// `r229m4-pipe-02`: single-stage `ls` — dispatches through the M3
/// happy path and lands `stage_outputs.len() == 1`, `final_value ==
/// stage_outputs[0]`, `halted_at == None`.
#[test]
fn r229m4_pipe_02_single_stage_ok() {
    const FP: &str = "r229m4-pipe-02";
    let mut state = ReplState::new();
    state.cmd_registry.register("ls", nullary_sig("ls"));

    let stage = cmd_node("ls", vec![]);
    let result =
        execute_pipeline(&state, &[&stage]).expect("registered command must dispatch");

    assert_eq!(
        result.stage_outputs.len(),
        1,
        "{FP}: one stage completed, so `stage_outputs.len()` must be 1"
    );
    assert!(
        result.stage_outputs[0].starts_with("cmd: ls ok"),
        "{FP}: stage 0 output must be the M3 render, got: {:?}",
        result.stage_outputs[0]
    );
    assert_eq!(
        result.final_value, result.stage_outputs[0],
        "{FP}: `final_value` mirrors the last successful stage's output"
    );
    assert_eq!(
        result.halted_at, None,
        "{FP}: full-success pipeline must have `halted_at == None`"
    );
    assert_eq!(
        result.halt_reason, None,
        "{FP}: `halt_reason` must be None on full success"
    );
}

/// `r229m4-pipe-03`: two-stage `ls | wc`. `wc` is registered with a
/// single required `String` positional — the *only* way it can
/// dispatch successfully against a source that types no explicit arg
/// for it (`wc` alone at pipeline position) is for the runner to
/// prepend ls's rendered output as the implicit first arg. If the
/// prepend did not happen, wc would see argv `[]` and argparse would
/// fire `Missing`, halting the pipeline at stage 1. A successful
/// dispatch here therefore proves the value-threading contract.
#[test]
fn r229m4_pipe_03_two_stage_threading() {
    const FP: &str = "r229m4-pipe-03";
    let mut state = ReplState::new();
    state.cmd_registry.register("ls", nullary_sig("ls"));
    state.cmd_registry.register("wc", unary_str_sig("wc"));

    let ls_node = cmd_node("ls", vec![]);
    let wc_node = cmd_node("wc", vec![]);
    let result = execute_pipeline(&state, &[&ls_node, &wc_node])
        .expect("both stages must dispatch");

    assert_eq!(
        result.halted_at, None,
        "{FP}: value threading must let a unary-String wc dispatch, got halt: {:?}",
        result.halt_reason
    );
    assert_eq!(
        result.stage_outputs.len(),
        2,
        "{FP}: both stages completed, so `stage_outputs.len()` must be 2"
    );
    assert!(
        result.stage_outputs[0].starts_with("cmd: ls ok"),
        "{FP}: stage 0 (ls) output must be the M3 render, got: {:?}",
        result.stage_outputs[0]
    );
    assert!(
        result.stage_outputs[1].contains("wc ok (1 args)"),
        "{FP}: wc must have received exactly 1 arg (the prepended ls output), got: {:?}",
        result.stage_outputs[1]
    );
    assert_eq!(
        result.final_value, result.stage_outputs[1],
        "{FP}: `final_value` mirrors the last stage's output"
    );
}

/// `r229m4-pipe-04`: three-stage chain `a | b | c` — every stage after
/// the first receives the prior stage's rendered output as its
/// implicit first arg. `a` is nullary; `b` and `c` each require one
/// String positional, so the run only completes if the prepend
/// happens at both the 0→1 and 1→2 boundaries.
#[test]
fn r229m4_pipe_04_three_stage_chain() {
    const FP: &str = "r229m4-pipe-04";
    let mut state = ReplState::new();
    state.cmd_registry.register("a", nullary_sig("a"));
    state.cmd_registry.register("b", unary_str_sig("b"));
    state.cmd_registry.register("c", unary_str_sig("c"));

    let stages = [cmd_node("a", vec![]), cmd_node("b", vec![]), cmd_node("c", vec![])];
    let refs: Vec<&SyntaxNode> = stages.iter().collect();
    let result =
        execute_pipeline(&state, &refs).expect("three-stage chain must dispatch");

    assert_eq!(
        result.halted_at, None,
        "{FP}: threading must succeed at every boundary, got halt: {:?}",
        result.halt_reason
    );
    assert_eq!(
        result.stage_outputs.len(),
        3,
        "{FP}: all three stages completed, so `stage_outputs.len()` must be 3"
    );
    assert!(
        result.stage_outputs[1].contains("b ok (1 args)"),
        "{FP}: stage 1 (b) must have seen 1 threaded arg, got: {:?}",
        result.stage_outputs[1]
    );
    assert!(
        result.stage_outputs[2].contains("c ok (1 args)"),
        "{FP}: stage 2 (c) must have seen 1 threaded arg, got: {:?}",
        result.stage_outputs[2]
    );
    assert_eq!(
        result.final_value, result.stage_outputs[2],
        "{FP}: `final_value` mirrors the last stage's output"
    );
}

/// `r229m4-pipe-05`: middle-stage error halts the pipeline. `a` is
/// nullary and succeeds at stage 0; `b` requires an `Int` positional
/// but the runner prepends a's rendered String (`cmd: a ok (0 args)`)
/// which is not an Int — argparse fires `TypeMismatch`. The pipeline
/// halts with `halted_at: Some(1)`; `stage_outputs` retains stage 0's
/// output; stage 2 (`c`) never runs.
#[test]
fn r229m4_pipe_05_mid_stage_halt() {
    const FP: &str = "r229m4-pipe-05";
    let mut state = ReplState::new();
    state.cmd_registry.register("a", nullary_sig("a"));
    state.cmd_registry.register("b", unary_int_sig("b"));
    state.cmd_registry.register("c", unary_str_sig("c"));

    let stages = [cmd_node("a", vec![]), cmd_node("b", vec![]), cmd_node("c", vec![])];
    let refs: Vec<&SyntaxNode> = stages.iter().collect();
    let result = execute_pipeline(&state, &refs).expect(
        "structural check passes — halt should surface as Ok(halted_at: Some(_))",
    );

    assert_eq!(
        result.halted_at,
        Some(1),
        "{FP}: pipeline must halt at stage 1 (the Int-required stage)"
    );
    assert_eq!(
        result.stage_outputs.len(),
        1,
        "{FP}: only stage 0 succeeded, so `stage_outputs.len()` must be 1"
    );
    assert!(
        result.stage_outputs[0].starts_with("cmd: a ok"),
        "{FP}: stage 0 output must be preserved, got: {:?}",
        result.stage_outputs[0]
    );
    let reason = result
        .halt_reason
        .as_deref()
        .expect(&format!("{FP}: halt_reason must be Some when halted_at is Some"));
    assert!(
        reason.contains("argparse") || reason.contains("Int"),
        "{FP}: halt reason must mention argparse / Int mismatch, got: {reason:?}"
    );
    assert_eq!(
        result.final_value, result.stage_outputs[0],
        "{FP}: `final_value` mirrors the last *successful* stage's output"
    );
}

/// `r229m4-pipe-06`: unknown command at stage 2 surfaces the underlying
/// `unknown command: xyz` inside the pipe's halt reason, both in the
/// `PipelineResult` and in the `TurnResult::Error` render when a
/// caller goes through `eval_turn` (the render is
/// `pipe: pipeline halted at stage 2 (unknown command: xyz)`).
#[test]
fn r229m4_pipe_06_unknown_command_at_stage_2() {
    const FP: &str = "r229m4-pipe-06";
    let mut state = ReplState::new();
    state.cmd_registry.register("a", nullary_sig("a"));
    state.cmd_registry.register("b", unary_str_sig("b"));
    // Note: `xyz` intentionally NOT registered.

    // Direct execute_pipeline path — inspect PipelineResult.
    let stages = [
        cmd_node("a", vec![]),
        cmd_node("b", vec![]),
        cmd_node("xyz", vec![]),
    ];
    let refs: Vec<&SyntaxNode> = stages.iter().collect();
    let result = execute_pipeline(&state, &refs).expect(
        "structural check passes — halt should surface as Ok(halted_at: Some(_))",
    );

    assert_eq!(
        result.halted_at,
        Some(2),
        "{FP}: pipeline must halt at stage 2 (the unregistered command)"
    );
    assert_eq!(
        result.stage_outputs.len(),
        2,
        "{FP}: stages 0 and 1 succeeded, so `stage_outputs.len()` must be 2"
    );
    let reason = result
        .halt_reason
        .as_deref()
        .expect(&format!("{FP}: halt_reason must be Some when halted_at is Some"));
    assert!(
        reason.contains("unknown command: xyz"),
        "{FP}: halt reason must name the missing command verbatim, got: {reason:?}"
    );

    // Turn-level render path — go through eval_turn and check the
    // rendered Error carries the same information under the `pipe:`
    // tag. `a | b | xyz` parses as a right-associated Pipe; the M4
    // turn arm flattens it before dispatch.
    let mut state2 = ReplState::new();
    state2.cmd_registry.register("a", nullary_sig("a"));
    state2.cmd_registry.register("b", unary_str_sig("b"));
    let turn = eval_turn(&mut state2, "a | b | xyz".to_owned());
    match turn.result {
        TurnResult::Error(e) => {
            assert!(
                e.starts_with("pipe:"),
                "{FP}: pipe halt must carry the `pipe:` stage tag, got: {e:?}"
            );
            assert!(
                e.contains("halted at stage 2"),
                "{FP}: turn render must name the halt index, got: {e:?}"
            );
            assert!(
                e.contains("unknown command: xyz"),
                "{FP}: turn render must include the underlying halt reason, got: {e:?}"
            );
        }
        TurnResult::Value(v) => panic!(
            "{FP}: pipeline with unregistered stage must render as Error, got Value: {v}"
        ),
    }
}

/// `r229m4-pipe-07`: end-to-end via `eval_turn` — a three-stage source
/// `a | b | c` (all registered) dispatches through the Pipe arm and
/// renders `pipe[3]: <final_value>` as `TurnResult::Value`.
#[test]
fn r229m4_pipe_07_eval_turn_render_prefix() {
    const FP: &str = "r229m4-pipe-07";
    let mut state = ReplState::default()
        .with_command("a", nullary_sig("a"))
        .with_command("b", unary_str_sig("b"))
        .with_command("c", unary_str_sig("c"));

    let turn = eval_turn(&mut state, "a | b | c".to_owned());
    match turn.result {
        TurnResult::Value(v) => {
            assert!(
                v.starts_with("pipe[3]:"),
                "{FP}: three-stage happy path must render `pipe[3]:` prefix, got: {v:?}"
            );
            assert!(
                v.contains("c ok (1 args)"),
                "{FP}: rendered final_value must be stage 2 (c)'s M3 output, got: {v:?}"
            );
        }
        TurnResult::Error(e) => panic!("{FP}: happy path pipeline errored: {e}"),
    }
}

/// `r229m4-pipe-08`: the turn counter advances by exactly one for a
/// whole pipeline (not once per stage). A pipeline is *one* REPL turn
/// — the R229.M7 replay harness aligns transcripts by turn index, and
/// a per-stage increment would desynchronize it.
#[test]
fn r229m4_pipe_08_turn_counter_advances_by_one() {
    const FP: &str = "r229m4-pipe-08";
    let mut state = ReplState::default()
        .with_command("a", nullary_sig("a"))
        .with_command("b", unary_str_sig("b"))
        .with_command("c", unary_str_sig("c"));

    assert_eq!(
        state.turn_counter, 0,
        "{FP}: fresh state must have turn_counter == 0"
    );
    let turn = eval_turn(&mut state, "a | b | c".to_owned());
    assert_eq!(
        state.turn_counter, 1,
        "{FP}: a three-stage pipeline is one turn — counter must advance by 1, not 3"
    );
    assert_eq!(
        turn.fingerprint, "repl.turn.0000000000000000",
        "{FP}: fingerprint uses the pre-increment counter (see crate doc)"
    );

    // A second pipeline turn advances by exactly one more, again
    // regardless of stage count.
    let _ = eval_turn(&mut state, "a | b".to_owned());
    assert_eq!(
        state.turn_counter, 2,
        "{FP}: second pipeline turn must land counter at 2"
    );
}
