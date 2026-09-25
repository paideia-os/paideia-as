//! R229.M7 pipeline typed-`Value` threading corpus: eight fixtures
//! pinning [`paideia_as_shell_repl::pipeline::PipelineResult::stage_values`]
//! and the `SyntaxNode::Pipe` arm's render-through-[`Value`] path in
//! [`paideia_as_shell_repl::eval_turn`].
//!
//! Each `assert!` names the fingerprint (`r229m7-piped-NN`) in its
//! message so the R220.M10 correlator attributes a regression to a
//! fixture without re-parsing the test name.
//!
//! # Why a separate file from `turn_pipe_threading.rs`
//!
//! The M4 corpus pins the string-threading contract (`stage_outputs`,
//! `final_value`, `halted_at`, `halt_reason`) and must stay green
//! under M7 — M7 adds `stage_values` alongside without moving the
//! existing fields. Keeping the two corpora in separate files keeps
//! the fixture-tag namespace clean (`r229m4-*` vs. `r229m7-*`) and
//! lets a follow-on milestone that lifts the M7 baseline (commands
//! returning typed values directly) touch only this file's `Value::Str(_)`
//! invariant without disturbing the string-threading pins.

use paideia_as_shell_ast::{Context, NodeSpan, SyntaxNode};
use paideia_as_shell_cmd::{
    ArgSpec, CapSpec, CommandSig, CommandWeight, EffectRow, ExecuteResult, InvocationCtx,
};
use paideia_as_shell_repl::{
    eval_turn, execute_pipeline, ReplState, TurnResult, Value,
};

/// No-op `execute` fn — R229.M7's stage dispatcher does not call it
/// (real `sig.execute` invocation is a later milestone; M7 still lifts
/// `execute_cmd`'s rendered string into `Value::Str`).
fn noop_execute(ctx: &InvocationCtx) -> ExecuteResult {
    ExecuteResult::new(0, &ctx.fingerprint)
}

/// A `CommandSig` with no positional arguments — matches any argv of
/// length 0. Reused shape from the M4 corpus.
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

/// A `CommandSig` with one required `String` positional — the only way
/// a bare `wc` in `ls | wc` dispatches successfully is for the pipeline
/// runner to have projected the prior stage's `Value` back into an
/// argv token and prepended it.
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

/// Build a bare-name `Cmd` node with the given name and (optional)
/// pipeline-context argv. Same helper shape as the M4 corpus.
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

/// `r229m7-piped-01`: empty pipeline — `execute_pipeline(state, &[])`
/// returns an empty `stage_values` parallel to the empty
/// `stage_outputs`. Pins the "no stages ran → no typed values"
/// invariant so a caller inspecting the M7 field on an empty
/// pipeline does not see a spurious `Unit` sentinel.
#[test]
fn r229m7_piped_01_empty_pipeline_no_values() {
    const FP: &str = "r229m7-piped-01";
    let state = ReplState::new();

    let result = execute_pipeline(&state, &[])
        .expect("empty pipeline must not surface a structural halt");

    assert!(
        result.stage_values.is_empty(),
        "{FP}: no stages ran, so `stage_values` must be empty, got: {:?}",
        result.stage_values
    );
    assert!(
        result.stage_outputs.is_empty(),
        "{FP}: no stages ran, so `stage_outputs` must be empty, got: {:?}",
        result.stage_outputs
    );
    assert_eq!(
        result.stage_values.len(),
        result.stage_outputs.len(),
        "{FP}: lockstep invariant — `stage_values.len() == stage_outputs.len()`"
    );
    assert_eq!(
        result.final_value, "",
        "{FP}: empty pipeline `final_value` stays the empty string"
    );
    assert_eq!(
        result.halted_at, None,
        "{FP}: empty pipeline is not a halt"
    );
}

/// `r229m7-piped-02`: single-stage `ls` — the M7 lift wraps the M3
/// rendered string as `Value::Str(_)`; `stage_values.len() == 1`.
/// Baseline invariant for the M7 milestone (all `Value::Str`) pins
/// the exact shape (test 7 checks the invariant across every fixture,
/// this one checks the single-stage case in isolation so a
/// regression that fires only at length 1 is not masked).
#[test]
fn r229m7_piped_02_single_stage_wraps_value_str() {
    const FP: &str = "r229m7-piped-02";
    let mut state = ReplState::new();
    state.cmd_registry.register("ls", nullary_sig("ls"));

    let stage = cmd_node("ls", vec![]);
    let result =
        execute_pipeline(&state, &[&stage]).expect("registered command must dispatch");

    assert_eq!(
        result.stage_values.len(),
        1,
        "{FP}: one stage completed, so `stage_values.len()` must be 1"
    );
    match &result.stage_values[0] {
        Value::Str(s) => assert!(
            s.starts_with("cmd: ls ok"),
            "{FP}: `Value::Str` payload must carry the M3 render, got: {s:?}"
        ),
        other => panic!(
            "{FP}: M7 baseline wraps every stage as `Value::Str`, got: {other:?}"
        ),
    }
}

/// `r229m7-piped-03`: two-stage `a | b` — both `stage_values` entries
/// are `Value::Str` and `stage_values.len() == 2`. Rules out the
/// regression "the runner forgot to push a `Value` when it pushed a
/// String" or vice versa.
#[test]
fn r229m7_piped_03_two_stage_both_str_values() {
    const FP: &str = "r229m7-piped-03";
    let mut state = ReplState::new();
    state.cmd_registry.register("a", nullary_sig("a"));
    state.cmd_registry.register("b", unary_str_sig("b"));

    let stages = [cmd_node("a", vec![]), cmd_node("b", vec![])];
    let refs: Vec<&SyntaxNode> = stages.iter().collect();
    let result =
        execute_pipeline(&state, &refs).expect("both stages must dispatch");

    assert_eq!(
        result.stage_values.len(),
        2,
        "{FP}: two stages completed, so `stage_values.len()` must be 2"
    );
    for (i, v) in result.stage_values.iter().enumerate() {
        assert!(
            matches!(v, Value::Str(_)),
            "{FP}: M7 baseline — stage {i} value must be Value::Str, got: {v:?}"
        );
    }
}

/// `r229m7-piped-04`: end-to-end via `eval_turn` — a two-stage source
/// `a | b` (both registered) renders through the Pipe arm's typed
/// path (`render_value(&last_stage_value)` inside the `pipe[N]:`
/// prefix). The prefix `pipe[2]:` must appear because M7 keeps the
/// M4-visible outer shape.
#[test]
fn r229m7_piped_04_eval_turn_render_prefix_typed() {
    const FP: &str = "r229m7-piped-04";
    let mut state = ReplState::default()
        .with_command("a", nullary_sig("a"))
        .with_command("b", unary_str_sig("b"));

    let turn = eval_turn(&mut state, "a | b".to_owned());
    match turn.result {
        TurnResult::Value(v) => {
            assert!(
                v.starts_with("pipe[2]:"),
                "{FP}: two-stage happy path must render `pipe[2]:` prefix, got: {v:?}"
            );
            // Under M7 the "final" is `render_value(&Value::Str(...))`
            // which reduces to the wrapped string — the same M3 render
            // stage 1 (b) produced. The M4 substring pin (`b ok (1
            // args)`) therefore still holds under M7 by construction.
            assert!(
                v.contains("b ok (1 args)"),
                "{FP}: rendered final must be stage 1 (b)'s M3 output, got: {v:?}"
            );
        }
        TurnResult::Error(e) => panic!("{FP}: happy path pipeline errored: {e}"),
    }
}

/// `r229m7-piped-05`: `stage_values.len() == stage_outputs.len()` for
/// every pipeline that runs at least one stage. Direct
/// `execute_pipeline` call so the lockstep invariant is pinned at the
/// runner boundary (not just through the turn arm's render path).
#[test]
fn r229m7_piped_05_lockstep_len_invariant() {
    const FP: &str = "r229m7-piped-05";
    let mut state = ReplState::new();
    state.cmd_registry.register("a", nullary_sig("a"));
    state.cmd_registry.register("b", unary_str_sig("b"));
    state.cmd_registry.register("c", unary_str_sig("c"));

    let stages = [
        cmd_node("a", vec![]),
        cmd_node("b", vec![]),
        cmd_node("c", vec![]),
    ];
    let refs: Vec<&SyntaxNode> = stages.iter().collect();
    let result =
        execute_pipeline(&state, &refs).expect("three-stage chain must dispatch");

    assert_eq!(
        result.stage_values.len(),
        result.stage_outputs.len(),
        "{FP}: `stage_values` and `stage_outputs` must grow lockstep — got \
         values.len()={}, outputs.len()={}",
        result.stage_values.len(),
        result.stage_outputs.len()
    );
    assert_eq!(
        result.stage_values.len(),
        3,
        "{FP}: three stages completed, so both parallel vectors must have length 3"
    );
}

/// `r229m7-piped-06`: halted pipeline preserves partial `stage_values`
/// parallel to `stage_outputs`. Stage 0 (`a`) succeeds; stage 1
/// (`xyz`) is not registered — `execute_cmd` returns
/// `UnknownCommand`, the runner records `halted_at: Some(1)` and
/// truncates *both* parallel vectors at length 1.
#[test]
fn r229m7_piped_06_halted_preserves_partial_values() {
    const FP: &str = "r229m7-piped-06";
    let mut state = ReplState::new();
    state.cmd_registry.register("a", nullary_sig("a"));
    // Note: `xyz` intentionally NOT registered.

    let stages = [cmd_node("a", vec![]), cmd_node("xyz", vec![])];
    let refs: Vec<&SyntaxNode> = stages.iter().collect();
    let result = execute_pipeline(&state, &refs).expect(
        "structural check passes — halt should surface as Ok(halted_at: Some(_))",
    );

    assert_eq!(
        result.halted_at,
        Some(1),
        "{FP}: pipeline must halt at stage 1 (the unregistered command)"
    );
    assert_eq!(
        result.stage_values.len(),
        1,
        "{FP}: only stage 0 succeeded, so `stage_values.len()` must be 1"
    );
    assert_eq!(
        result.stage_values.len(),
        result.stage_outputs.len(),
        "{FP}: parallel vectors stay lockstep even on halt"
    );
    match &result.stage_values[0] {
        Value::Str(s) => assert!(
            s.starts_with("cmd: a ok"),
            "{FP}: preserved stage-0 value must wrap the M3 render, got: {s:?}"
        ),
        other => panic!(
            "{FP}: preserved stage-0 value must be `Value::Str`, got: {other:?}"
        ),
    }
    assert!(
        result.halt_reason.is_some(),
        "{FP}: `halt_reason` must be Some when `halted_at` is Some"
    );
}

/// `r229m7-piped-07`: M7 baseline invariant — every entry in
/// `stage_values` is a `Value::Str`. A follow-on milestone that
/// grows commands to return typed values directly will loosen this
/// invariant; this fixture stays as a canary that flips the day the
/// baseline lifts (a caller reading it will know it needs updating).
#[test]
fn r229m7_piped_07_all_values_are_str_baseline() {
    const FP: &str = "r229m7-piped-07";
    let mut state = ReplState::new();
    state.cmd_registry.register("a", nullary_sig("a"));
    state.cmd_registry.register("b", unary_str_sig("b"));
    state.cmd_registry.register("c", unary_str_sig("c"));
    state.cmd_registry.register("d", unary_str_sig("d"));

    let stages = [
        cmd_node("a", vec![]),
        cmd_node("b", vec![]),
        cmd_node("c", vec![]),
        cmd_node("d", vec![]),
    ];
    let refs: Vec<&SyntaxNode> = stages.iter().collect();
    let result =
        execute_pipeline(&state, &refs).expect("four-stage chain must dispatch");

    assert_eq!(
        result.stage_values.len(),
        4,
        "{FP}: four stages completed, so `stage_values.len()` must be 4"
    );
    for (i, v) in result.stage_values.iter().enumerate() {
        assert!(
            matches!(v, Value::Str(_)),
            "{FP}: M7 baseline — every stage value must be `Value::Str`, \
             stage {i} was: {v:?}"
        );
    }
}

/// `r229m7-piped-08`: the turn counter advances by exactly one per
/// pipeline `eval_turn`, regardless of the pipeline's stage count.
/// Same invariant as `r229m4-pipe-08` but re-pinned under the M7
/// render path — a regression that per-stage-increments the counter
/// under the typed render would desynchronise R229 replay by exactly
/// the stage count.
#[test]
fn r229m7_piped_08_turn_counter_advances_by_one() {
    const FP: &str = "r229m7-piped-08";
    let mut state = ReplState::default()
        .with_command("a", nullary_sig("a"))
        .with_command("b", unary_str_sig("b"))
        .with_command("c", unary_str_sig("c"));

    assert_eq!(
        state.turn_counter, 0,
        "{FP}: fresh state must have turn_counter == 0"
    );

    let _ = eval_turn(&mut state, "a | b | c".to_owned());
    assert_eq!(
        state.turn_counter, 1,
        "{FP}: a three-stage pipeline is one turn — counter must advance by 1, not 3"
    );

    // A second pipeline turn (different stage count) still advances
    // by exactly one — the increment is per turn, not per stage.
    let _ = eval_turn(&mut state, "a | b".to_owned());
    assert_eq!(
        state.turn_counter, 2,
        "{FP}: second pipeline turn (2 stages) must land counter at 2, not 4"
    );

    // A single-stage `Cmd` (parses as `Cmd`, not `Pipe`, so it does
    // not go through the Pipe arm) still advances by one — pinned
    // here as the boundary condition that a per-stage regression
    // would violate.
    let _ = eval_turn(&mut state, "a".to_owned());
    assert_eq!(
        state.turn_counter, 3,
        "{FP}: single-stage (Cmd, not Pipe) turn must still bump by exactly 1"
    );
}
