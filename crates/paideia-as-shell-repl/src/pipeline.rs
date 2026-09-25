//! R229.M4 — REPL pipeline value threading.
//!
//! Executes a `Pipe`'s linear stage list (produced by
//! [`crate::turn::flatten_pipe_stages`]) left-to-right, threading each
//! stage's rendered output as the next stage's *implicit first
//! positional argument*. The result is a [`PipelineResult`] recording
//! per-stage outputs, the last successful stage's value, and — on an
//! `execute_cmd` failure at some stage — the halt index and reason.
//!
//! # Why "prepend the rendered string as an argv token"
//!
//! The R222.M2 `CommandSig::execute` op consumes a `Stream<Record>` and
//! produces one — the "real" value-threading shape a future milestone
//! will land. R229.M4 does *not* land that shape: there is no
//! `InvocationCtx`, no `Stream<Record>` handle, no capability
//! environment, no fingerprint threading through the turn pipeline. All
//! that exists today at the pipeline stage is
//! [`crate::cmd_dispatch::execute_cmd`], which:
//!
//! * looks up the sig by name,
//! * runs the R222.M4 argparse over the positional argv,
//! * returns a rendered `cmd: <name> ok (K args)` string.
//!
//! To exercise value-threading end-to-end under that constraint, M4
//! makes each stage's *rendered string* the next stage's implicit
//! first positional argument. That lets a test register `wc` with one
//! required `String` positional and verify — via argv shape — that the
//! previous stage's output landed in wc's argument list; it also lets
//! the R229.M7 replay harness align pipe transcripts by observing
//! `stage_outputs`. M5+ replaces the string-threading with a real
//! `Stream<Record>`/`ExecuteResult`-scalar path; the shape of this
//! module (`execute_pipeline` returning `PipelineResult`) is preserved
//! across that migration because the outer turn code already discards
//! everything but `final_value` on the happy path and `halted_at` on
//! the sad path.
//!
//! # Halt taxonomy
//!
//! Two axes:
//!
//! * **Structural halt** — a stage is not a `SyntaxNode::Cmd`, or its
//!   `Cmd::name` head is not a bare `Ident`/`Var` (i.e. the dispatcher
//!   has no name key to look up). Surfaces as `Err(CmdError::
//!   PipelineHalted { stage_idx, reason })` with no partial state — the
//!   pipeline could not begin executing at that index or beyond, and
//!   any prior stage outputs are discarded because there is no
//!   `PipelineResult` shape to return them in. This is intentionally
//!   *strict*: a structural malformation is a parser/elaborator bug
//!   that should surface unambiguously rather than being silently
//!   swallowed as "partial success up to stage k".
//!
//! * **Execution halt** — a stage is a valid `Cmd` but `execute_cmd`
//!   returns an error (`UnknownCommand`, `ArgParseFailed`, …).
//!   Surfaces as `Ok(PipelineResult { halted_at: Some(i), halt_reason:
//!   Some(reason), stage_outputs: [..i], final_value: <last success> })`
//!   — the *partial* success (stages 0..i completed) is preserved for
//!   a caller (test harness, R229.M5 diagnostic renderer) that wants to
//!   inspect what did run before the halt.
//!
//! # `halt_reason` field
//!
//! Not in the R229.M4 spec's `PipelineResult` sketch, but required by
//! the fixture corpus (`r229m4-pipe-06` renders `pipe: pipeline halted
//! at stage 2 (unknown command: xyz)` — the underlying reason must
//! survive from `CmdError` through the pipeline runner into the
//! `TurnResult::Error` render). Adding it as a sibling field to
//! `halted_at` keeps the two "there was a halt" pieces of information
//! adjacent — a `Some/Some` invariant that a future refactor can
//! collapse into `halted_at: Option<HaltInfo>` if that shape becomes
//! more ergonomic.

use paideia_as_shell_ast::SyntaxNode;

use crate::cmd_dispatch::{self, CmdError};
use crate::lambda_eval::Value;
use crate::turn::{arg_to_string, cmd_head_name, ReplState};

/// Outcome of one pipeline execution.
///
/// # Invariants
///
/// * `stage_outputs.len()` is the number of stages that ran to
///   completion (i.e. `execute_cmd` returned `Ok`). On a full-success
///   pipeline of N stages this is N; on a mid-pipeline halt at stage
///   `i` this is `i` (stages 0..i-1 succeeded, stage i failed).
/// * `final_value` mirrors `stage_outputs.last().cloned().unwrap_or_default()`.
///   Kept as a distinct field (rather than computed at read time) so a
///   caller rendering the turn result does not need to import the
///   pipeline module's invariants to read the last stage's output.
/// * `halted_at` is `None` on full success, `Some(i)` when
///   `execute_cmd` failed at stage `i` (and thus stages `i..N` did not
///   run and are absent from `stage_outputs`).
/// * `halt_reason` is `Some(_)` iff `halted_at.is_some()`; the string
///   is the `Display` of the `CmdError` `execute_cmd` returned at the
///   halted stage.
///
/// The `PartialEq` derive lets a test compare a synthesised expected
/// result against the actual — the field set is small (five scalars)
/// so a naive equality is the right shape. `Eq` is intentionally *not*
/// derived: R229.M7 grew `stage_values: Vec<Value>` and [`Value`]'s
/// `Fn(Closure)` carrier holds a `HashMap<String, Value>` in its
/// captured env — an unordered container that cannot claim `Eq`.
/// `PartialEq` is enough for every test comparison the corpus needs.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct PipelineResult {
    /// Rendered output of each stage that ran to completion, in stage
    /// order. See the type doc for the length/halt invariant.
    pub stage_outputs: Vec<String>,
    /// Typed [`Value`] each stage produced, in stage order, parallel to
    /// [`Self::stage_outputs`]. Under R229.M7 every entry is a
    /// [`Value::Str`] wrapping the stage's rendered string — commands
    /// still return a rendered `String` from `execute_cmd` and the
    /// pipeline runner lifts each into `Value::Str` before threading it
    /// onward. A follow-on milestone that grows `CommandSig::execute` to
    /// return a typed [`Value`] directly will fill in `Value::Int`,
    /// `Value::Bool`, `Value::Fn`, and `Value::Unit` entries without
    /// touching the runner shape — the caller in `turn.rs` already
    /// renders through [`crate::lambda_eval::Value`]'s renderer.
    ///
    /// Invariant: `stage_values.len() == stage_outputs.len()` — both
    /// grow lockstep, and on a mid-pipeline halt at stage `i` both are
    /// truncated to `i`.
    pub stage_values: Vec<Value>,
    /// Convenience mirror of `stage_outputs.last()`. Empty string on
    /// an empty pipeline; also empty on a stage-0 halt (no prior
    /// success).
    pub final_value: String,
    /// `Some(i)` iff `execute_cmd` failed at stage `i`; `None` on
    /// success or on an empty pipeline.
    pub halted_at: Option<usize>,
    /// The `Display` of the `CmdError` that halted the pipeline;
    /// `None` iff `halted_at.is_none()`.
    pub halt_reason: Option<String>,
}

/// Run a pipeline: dispatch each stage in order, threading the prior
/// stage's rendered output as an implicit first positional argument to
/// the next stage.
///
/// # Argument
///
/// `stages` — the linear stage list (produced by
/// `crate::turn::flatten_pipe_stages` on a `SyntaxNode::Pipe`
/// spine, or a bare single-element slice for a non-pipe caller).
/// An empty slice is a well-formed empty pipeline and returns the
/// default `PipelineResult`.
///
/// # Return shape
///
/// See the module doc's "Halt taxonomy" section for when this returns
/// `Err(CmdError::PipelineHalted)` (structural halt, no partial state)
/// vs. `Ok(PipelineResult { halted_at: Some(_), .. })` (execution
/// halt, partial state preserved).
///
/// # Value threading
///
/// For stage `i >= 1`, the previous stage's rendered output is
/// prepended to that stage's original argv, so a `wc` stage in `ls |
/// wc` sees argv `["<ls's output>"]` even though its source-level argv
/// was empty. Stage 0 receives its original argv unchanged; the
/// pre-stage-0 "input" of an empty string is *not* prepended — a
/// nullary-sig stage at index 0 receives a zero-length argv and
/// dispatches through the M3 happy path unchanged.
///
/// R229.M7 upgrades the *threaded quantity* from a bare `String` to a
/// typed [`Value`] (see [`PipelineResult::stage_values`]). Under M7
/// every `execute_cmd` return is lifted into `Value::Str(rendered)`
/// before it is threaded onward — the on-wire argv shape stays a
/// `Vec<String>` (the M3 dispatcher key), so a non-`Str` `Value` is
/// projected back into a string via [`value_to_arg_string`] at the
/// stage boundary. A follow-on milestone that grows commands to
/// return typed values directly replaces the `Value::Str(rendered)`
/// lift and gets typed cross-stage threading for free.
pub fn execute_pipeline(
    state: &ReplState,
    stages: &[&SyntaxNode],
) -> Result<PipelineResult, CmdError> {
    let mut result = PipelineResult::default();
    // `prior_value` carries the *typed* handoff between stages. Under
    // M7 every `execute_cmd` return lifts to `Value::Str(rendered)`, so
    // this is effectively a `Value::Str` at every stage boundary; the
    // typed carrier is what lets a future milestone thread `Value::Int`
    // / `Value::Bool` / `Value::Fn` without touching the loop shape.
    // `None` at stage 0 means "no prior stage exists"; the argv
    // prepend skip is keyed on `i >= 1`, not on `prior_value.is_some()`.
    let mut prior_value: Option<Value> = None;
    for (i, stage) in stages.iter().enumerate() {
        // 1) Structural check: every stage must be a `Cmd` with a bare-
        // name head. A `Redirect`, `Group`, nested `Pipe` (from a
        // parenthesised sub-pipeline — a shape R222 permits at grammar
        // but M4 does not yet flatten across), `DatalogBlock`, or
        // `Lambda` at pipeline position is a structural halt (see the
        // module doc's "Halt taxonomy"). Preserving prior stage outputs
        // is intentionally *not* done here: a structural halt indicates
        // a parser/elaborator bug the caller should not paper over by
        // rendering "partial success".
        let (head, args) = match stage {
            SyntaxNode::Cmd { name, args, .. } => (name.as_ref(), args.as_slice()),
            other => {
                return Err(CmdError::PipelineHalted {
                    stage_idx: i,
                    reason: format!("non-Cmd stage: {other:?}"),
                });
            }
        };
        let cmd_name = match cmd_head_name(head) {
            Some(n) => n,
            None => {
                return Err(CmdError::PipelineHalted {
                    stage_idx: i,
                    reason: format!("non-name head: {head:?}"),
                });
            }
        };

        // 2) Build the threaded argv. Stage 0 uses its source argv
        // as-is; stages 1..N prepend the prior stage's typed value
        // (projected into an argv token via `value_to_arg_string`) as
        // the implicit first positional argument. The projection is
        // where the M7 typed handoff meets the M3 string-keyed argv
        // shape — a future milestone that grows `CommandSig::execute`
        // to consume typed values replaces this projection with a
        // direct value pass-through and gets end-to-end typed
        // threading.
        let mut argv: Vec<String> = args.iter().map(arg_to_string).collect();
        if i >= 1 {
            let prior = prior_value.as_ref().expect(
                "invariant: stage i >= 1 implies stage 0 completed and set prior_value",
            );
            argv.insert(0, value_to_arg_string(prior));
        }

        // 3) Dispatch. On execute_cmd failure, capture the halt
        // metadata into the PipelineResult and return `Ok` with the
        // partial state — see the module doc's "Execution halt" for
        // rationale. On success, lift the rendered string into
        // `Value::Str` and thread it forward — M7's baseline is
        // "every stage produces a `Value::Str`"; a follow-on
        // milestone that has commands returning typed values swaps
        // this lift for the real return value.
        match cmd_dispatch::execute_cmd(&state.cmd_registry, &cmd_name, &argv) {
            Ok(rendered) => {
                let v = Value::Str(rendered.clone());
                result.stage_outputs.push(rendered.clone());
                result.stage_values.push(v.clone());
                result.final_value = rendered;
                prior_value = Some(v);
            }
            Err(err) => {
                result.halted_at = Some(i);
                result.halt_reason = Some(err.to_string());
                return Ok(result);
            }
        }
    }
    Ok(result)
}

/// Project a typed [`Value`] back into an argv-token string for the
/// M3 string-keyed `execute_cmd` dispatcher.
///
/// Under R229.M7 every `stage_values` entry is a `Value::Str` (the
/// lift in [`execute_pipeline`]), so the `Str` arm carries every real
/// dispatch today; the other arms exist so a follow-on milestone
/// that returns typed values from `CommandSig::execute` can drop the
/// lift without also having to teach this projection about `Int` /
/// `Bool` / `Fn` / `Unit`. The renders match [`crate::turn`]'s user-
/// visible `render_value` for `Int`, `Bool`, and `Unit`; a closure
/// stringifies as the placeholder `<closure>` because argv is not the
/// place to smuggle a captured environment.
fn value_to_arg_string(v: &Value) -> String {
    match v {
        Value::Str(s) => s.clone(),
        Value::Int(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Unit => "()".to_owned(),
        Value::Fn(_) => "<closure>".to_owned(),
    }
}

