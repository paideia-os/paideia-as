//! The [`eval_turn`] entry point and the [`ReplState`] / [`ReplTurn`]
//! / [`TurnResult`] shapes it operates over.
//!
//! # Pipeline
//!
//! ```text
//!   source ── parse ──▶ SyntaxNode
//!             │
//!             ▼
//!         typecheck (M1: identity Ok; M2 wires HM Algorithm W)
//!             │
//!             ▼
//!         elaborate (M1: identity; M2+ produces typed nodes)
//!             │
//!             ▼
//!         execute (dispatch on SyntaxNode variant)
//! ```
//!
//! # Executor branches (R229.M2)
//!
//! * [`SyntaxNode::DatalogBlock`] — lower the block to a
//!   [`paideia_as_shell_datalog::Program`] via [`crate::lower::lower_datalog`]
//!   (a direct AST → program walk, replacing the M1 re-tokenization
//!   bridge), then run [`Evaluator::run_query_typed`] with an empty
//!   [`Query`] and an empty [`SchemaRegistry`] so the R226.M9 schema
//!   check runs on every atom the block mentions. Renders
//!   `dlg: N facts` on typecheck success (N = program.facts.len()),
//!   `typecheck: K error(s)` on schema failure, or `run: <err>` on
//!   other evaluator failures. The M1 fixpoint + session-EDB overlay
//!   path is intentionally dropped for M2 — R226.M9's typed path is
//!   the only path a schema-driven caller needs; a follow-on
//!   milestone re-wires the session overlay through the typed path
//!   (see the R226.M9 doc for the intended shape).
//! * [`SyntaxNode::Cmd`] / [`SyntaxNode::Pipe`] — stub. R229.M3 wires
//!   the command dispatcher through here.
//! * [`SyntaxNode::Lambda`] — stub. R229.M4 wires the lambda JIT.
//! * Anything else (Seq, RecordExpr, literals, …) — stub with the
//!   variant name, so a user-visible message names what did not run.
//!
//! # Fingerprint
//!
//! `repl.turn.{id:016x}` — id is the *pre-increment* turn counter
//! (so the first turn is `repl.turn.0000000000000000`). See the crate
//! doc for the R229.M7 replay-alignment rationale.

use paideia_as_shell_ast::{parser as ast_parser, SyntaxNode};
use paideia_as_shell_cmd::CommandSig;
use paideia_as_shell_datalog::{
    EvalError, Evaluator, Query, SchemaRegistry, SessionEdb,
};
use paideia_as_shell_hm::{MonoType, TypeEnv, TypeScheme, TypeVar};

use crate::cmd_dispatch::{self, CmdDispatchRegistry};
use crate::lambda_eval::{self, LambdaError, Value};
use crate::lower;
use crate::pipeline;
use crate::type_stage::{self, TypeStageError};

/// Session-scoped mutable state a driver threads through every
/// [`eval_turn`] call.
///
/// # Fields
///
/// * `turn_counter` — monotone count of completed turns (post-
///   increment). Reset only by constructing a fresh `ReplState`.
/// * `session_edb` — Datalog session overlay. Persists across turns
///   per R226.M8 so `assert p(a,b).` in one turn is visible to a query
///   in the next.
///
/// Kept as a plain struct with public fields (rather than
/// getters/setters) because R229.M2's HM environment and M3's command
/// registry will land as sibling fields — an interface with three
/// public fields is trivially extended, one with three getter pairs
/// is not.
#[derive(Debug, Default)]
pub struct ReplState {
    /// Number of turns completed in this session (monotone, never
    /// decreases, never wraps at u64::MAX in any realistic session).
    pub turn_counter: u64,
    /// Datalog session-local EDB. See
    /// [`paideia_as_shell_datalog::SessionEdb`] for the contract.
    pub session_edb: SessionEdb,
    /// R229.M3 command-dispatch registry — name → `CommandSig`. The
    /// executor's `SyntaxNode::Cmd` arm consults it via
    /// [`crate::cmd_dispatch::execute_cmd`]. The field is `pub` so a
    /// driver can register commands directly on the struct literal
    /// (matching the `session_edb` shape above); the
    /// [`Self::with_command`] builder is the ergonomic path for
    /// method-chaining.
    pub cmd_registry: CmdDispatchRegistry,
    /// R225.M4 HM term environment threaded through stage 2 of every
    /// turn's pipeline. `TypeEnv` is persistent-by-construction (see
    /// [`paideia_as_shell_hm::TypeEnv`]), and R225.M4 never *mutates*
    /// it — a driver that wants to seed prelude bindings does so by
    /// replacing the field before the first turn. A future milestone
    /// (R225.M5+ let-persistence at the REPL surface) will grow the
    /// mutation path here.
    pub type_env: TypeEnv,
    /// R229.M5 term-value environment threaded through the lambda
    /// executor. Empty at session start; the M5 executor reads it (via
    /// [`lambda_eval::eval_lambda`]) and, since R229.M6, top-level
    /// `let` bindings mutate it in place — a `let x = 42` turn
    /// installs `x → Int(42)`, which a subsequent turn's lambda body
    /// sees when it references `x`. See [`execute_let`] and
    /// [`eval_let_binding`] for the mutation paths; those paths also
    /// keep [`Self::type_env`] in step (see `execute_let`'s doc). Kept
    /// as a public field so a driver can seed prelude values
    /// (mirroring how `type_env` is seeded) without going through a
    /// builder.
    pub value_env: std::collections::HashMap<String, Value>,
}

impl ReplState {
    /// Fresh session: turn counter at 0, empty session EDB, empty
    /// command registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Builder: register `sig` under `name` and return the state.
    ///
    /// Consumes `self` and returns it so a driver can chain multiple
    /// `with_command(...)` calls at construction:
    ///
    /// ```ignore
    /// let state = ReplState::default()
    ///     .with_command("head", head::functor(&schemas))
    ///     .with_command("count", count::functor(&schemas));
    /// ```
    ///
    /// The builder is a thin wrapper over
    /// [`CmdDispatchRegistry::register`]; a driver that already holds
    /// a `&mut ReplState` mid-session should call
    /// `state.cmd_registry.register(...)` directly (no need to move
    /// the whole state through the builder).
    pub fn with_command(mut self, name: impl Into<String>, sig: CommandSig) -> Self {
        self.cmd_registry.register(name, sig);
        self
    }
}

/// The user-visible outcome of a single turn.
///
/// Kept as a two-variant enum rather than `Result<String, String>` so
/// downstream renderers pattern-match on intent rather than on the
/// `Ok`/`Err` polarity — an error rendered as `Err(String)` invites
/// callers to `?`-propagate a turn failure up through the driver,
/// which is exactly the wrong shape for a REPL (the failure is
/// *material* — the user needs to see it, not have it swallowed by
/// the outer loop).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TurnResult {
    /// The turn completed and rendered `value` as its user-visible
    /// output. In R229.M1 most branches are stubs so `value` typically
    /// carries a "not yet implemented" marker.
    Value(String),
    /// The turn failed with `message`. The message is prefixed with
    /// the stage tag (`parse:`, `type:`, `elab:`, `exec:`) so the
    /// user (or the R229.M5 diagnostics layer) can attribute the
    /// failure without re-running the pipeline.
    Error(String),
}

/// One turn's record: input, outcome, fingerprint.
///
/// The R229.M7 replay harness materializes a sequence of `ReplTurn`
/// values; a fresh session run through the same source sequence must
/// produce the same fingerprints and, once M2+ wires real semantics,
/// the same results.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplTurn {
    /// The raw source the user typed for this turn.
    pub source: String,
    /// The turn's outcome (see [`TurnResult`]).
    pub result: TurnResult,
    /// Fingerprint of the form `repl.turn.{id:016x}` where `id` is
    /// the pre-increment turn counter.
    pub fingerprint: String,
    /// The HM-inferred [`MonoType`] of the turn's root expression, or
    /// `None` when stage 2 did not produce one (R225.M4 wiring).
    ///
    /// `None` is the *normal* outcome for the shapes the M4 subset does
    /// not cover — the pipeline sub-language (Cmd with args, Pipe,
    /// Seq, Redirect, Background), the DatalogBlock branch, the bare
    /// nullary command form (`ls`), and the lambda-side shapes not yet
    /// modelled (Match, BinOp, UnaryOp, LitBool). See
    /// [`crate::type_stage`] for the enumerated support list.
    /// `None` also appears when the turn short-circuits before stage 2
    /// (a parse failure) or aborts *at* stage 2 with a real HM error
    /// (`type: unbound …` / `type: unify failed: …`) — in either case
    /// the `result` field carries the diagnostic.
    pub inferred_type: Option<MonoType>,
}

/// Run one REPL turn end-to-end: parse → typecheck → elaborate →
/// execute → render.
///
/// Mutates `state.turn_counter` (always incremented, even on error —
/// so a fingerprint sequence has no gaps and R229.M7's replay can
/// align by index). Mutates `state.session_edb` only if the executor
/// asserts into it (R229.M1 has no such branch — session mutation
/// stays programmatic per R226.M8).
pub fn eval_turn(state: &mut ReplState, source: String) -> ReplTurn {
    // Pre-increment id: the first turn's fingerprint is
    // `repl.turn.0000000000000000`.
    let id = state.turn_counter;
    state.turn_counter += 1;
    let fingerprint = format!("repl.turn.{:016x}", id);

    // Stage 1: parse.
    let node = match ast_parser::parse(&source) {
        Ok(n) => n,
        Err(err) => {
            return ReplTurn {
                source,
                result: TurnResult::Error(format!("parse: {:?}", err)),
                fingerprint,
                inferred_type: None,
            };
        }
    };

    // Stage 2: typecheck (R225.M4 wiring).
    //
    // The DatalogBlock branch has its own R226.M9 typed query pass
    // downstream in stage 4 (`execute_datalog`); running the lambda-
    // shaped HM checker over the block would be a category error, so
    // it is skipped up front. Every other root variant is offered to
    // [`type_stage::type_check`]; the three-way error triage matches
    // [`TypeStageError`]'s per-variant contract:
    //
    // * `Ok(mono)` — capture as `inferred_type: Some(mono)` and
    //   proceed to stage 4.
    // * `Err(UnsupportedNode(_))` — no HM opinion, but not a failure.
    //   Record `inferred_type: None` and proceed. This preserves the
    //   R229.M2 (datalog) / R229.M3 (Cmd + Pipe) executor contract
    //   for the pipeline sub-language, whose own checkers run in
    //   stage 4.
    // * `Err(UnboundVar | UnifyFailed)` — a genuine HM failure over a
    //   shape M4 does model. Abort the turn with a `type:`-prefixed
    //   Error, matching the `<stage>: <message>` convention.
    let inferred_type = if matches!(node, SyntaxNode::DatalogBlock { .. }) {
        None
    } else {
        match type_stage::type_check(&node, &state.type_env) {
            Ok(mono) => Some(mono),
            Err(TypeStageError::UnsupportedNode(_)) => None,
            Err(err) => {
                return ReplTurn {
                    source,
                    result: TurnResult::Error(format!("type: {err}")),
                    fingerprint,
                    inferred_type: None,
                };
            }
        }
    };

    // Stage 3: elaborate. R229.M1 identity — the AST-as-is is passed
    // to the executor. R229.M2+ lowers to a typed IR here.
    let elaborated = &node;

    // Stage 4: execute — dispatch by variant.
    let result = execute(state, elaborated);

    ReplTurn { source, result, fingerprint, inferred_type }
}

/// Dispatch the executor by AST root variant. R229.M2's four real
/// arms (plus the fallback) map one-to-one to the four sub-language
/// surfaces the shell composes.
fn execute(state: &mut ReplState, node: &SyntaxNode) -> TurnResult {
    match node {
        SyntaxNode::DatalogBlock { .. } => execute_datalog(state, node),
        SyntaxNode::Cmd { name, args, .. } => execute_cmd_node(state, name, args),
        SyntaxNode::Pipe { .. } => {
            // R229.M4: real stage-to-stage value threading. Flatten the
            // right-associated `Pipe(a, Pipe(b, c))` spine into a linear
            // `Vec<&SyntaxNode>` of stages and hand it to
            // [`pipeline::execute_pipeline`]. The pipeline runner
            // returns:
            //
            // * `Ok(res)` with `halted_at: None` on full success — the
            //   turn renders `pipe[N]: <final_value>` where N is the
            //   stage count and final_value is the last stage's rendered
            //   output (from the M3 execute_cmd stub, so today: `cmd:
            //   <name> ok (K args)`).
            // * `Ok(res)` with `halted_at: Some(i)` on an execute_cmd
            //   failure at stage i — the turn renders `pipe: pipeline
            //   halted at stage i (<underlying reason>)` as an Error.
            //   `res.stage_outputs[..i]` remain observable to a caller
            //   that reaches for the PipelineResult directly (test 5 in
            //   `tests/turn_pipe_threading.rs`).
            // * `Err(CmdError::PipelineHalted { .. })` on a *structural*
            //   halt — a non-Cmd stage or a Cmd head that is not a bare
            //   name — where no partial state survives.
            //
            // Real invocation of `sig.execute` (returning an
            // `ExecuteResult` scalar + a fingerprint) is still M5+ — M4
            // threads the *rendered string* of each stage as the next
            // stage's implicit first positional argument, which is
            // enough to prove the value-threading path end-to-end
            // without waiting on the `Stream<Record>` shape.
            let stages = flatten_pipe_stages(node);
            // `state` is `&mut ReplState` here but `execute_pipeline`
            // only reads it; explicit reborrow to shared avoids the
            // coercion rules relying on nightly-only behaviours.
            match pipeline::execute_pipeline(&*state, &stages) {
                Ok(res) => match res.halted_at {
                    None => {
                        // R229.M7: the rendered "final" comes from the
                        // typed [`Value`] the last stage produced,
                        // routed through the shared [`render_value`]
                        // renderer — the same surface the Lambda arm
                        // and the Let arm use. Under M7 every
                        // `stage_values` entry is a `Value::Str`, so
                        // the render still lands the same M3 string the
                        // M4 `final_value` mirror carried; a follow-on
                        // milestone that grows commands to return typed
                        // values immediately gets typed `pipe[N]:`
                        // renders without a second executor touch.
                        //
                        // Empty pipeline (no stages, `stage_values`
                        // empty) is *not* something the parser
                        // produces — `flatten_pipe_stages` always
                        // returns at least one stage — but a direct
                        // caller of `execute_pipeline` can construct
                        // one, so the render defaults to the empty
                        // string in that case to keep the outer shape
                        // `pipe[0]: `.
                        let final_rendered = res
                            .stage_values
                            .last()
                            .map(render_value)
                            .unwrap_or_default();
                        TurnResult::Value(format!(
                            "pipe[{}]: {}",
                            stages.len(),
                            final_rendered
                        ))
                    }
                    Some(i) => TurnResult::Error(format!(
                        "pipe: pipeline halted at stage {} ({})",
                        i,
                        res.halt_reason.as_deref().unwrap_or("<unknown>")
                    )),
                },
                Err(err) => TurnResult::Error(format!("pipe: {err}")),
            }
        }
        SyntaxNode::Lambda { .. } => {
            // R229.M5: real lambda execution via
            // [`lambda_eval::eval_lambda`]. The M1..M4 stub returned
            // the literal `lambda: <not yet implemented>` string; M5
            // walks the AST under `state.value_env` and renders the
            // resulting [`Value`] on the same `TurnResult::Value`
            // surface. Each variant renders in the same shape the
            // user's own program would produce on the RHS of a binding
            // (`42`, `"hello"`, `true`, `()`, `<closure>`) — no `lambda:`
            // prefix on the happy path, matching the general "the
            // executor renders one line of user output" convention.
            // Errors keep the `lambda:` stage tag so a driver can
            // attribute a runtime lambda failure without inspecting
            // the AST.
            match lambda_eval::eval_lambda(node, &state.value_env) {
                Ok(v) => TurnResult::Value(render_value(&v)),
                Err(e) => TurnResult::Error(format!("lambda: {e}")),
            }
        }
        // R229.M6: intercept top-level `Let` and mutate `state.value_env`
        // so subsequent turns see the binding. The R221.M5 pipeline
        // parser does not (yet) emit a top-level `SyntaxNode::Let` — the
        // canonical driver path is [`eval_let_binding`], which builds the
        // node and threads it through this arm — but leaving the branch
        // wired here means a follow-on parser milestone that produces
        // `let x = 42` at the pipeline top-level slots in without a
        // second executor touch. A `Let` reached inside a Lambda body is
        // handled by `eval_lambda`'s own local-binding path (see the
        // module doc's "top-level vs. nested" distinction).
        SyntaxNode::Let { name, value, .. } => match execute_let(state, name, value) {
            Ok(v) => TurnResult::Value(format!("{name} = {}", render_value(&v))),
            Err(e) => TurnResult::Error(format!("lambda: {e}")),
        },
        // Everything else — Seq, Group, Redirect, RecordExpr, literals,
        // Atom/Rule outside a block, App/Var/Match, BinOp/UnaryOp,
        // FieldAccess, QVar/InterpVar/NotAtom, Ident — falls into the
        // generic "not yet implemented" bucket. Naming the variant keeps
        // the user's error message specific without hard-coding twenty
        // arms of essentially the same string. (`Let` moved up to its
        // own arm in R229.M6.)
        other => TurnResult::Value(format!(
            "{} — <not yet implemented>",
            variant_name(other)
        )),
    }
}

/// Datalog branch of the executor (R229.M2).
///
/// Lowers the AST block to a [`paideia_as_shell_datalog::Program`] via
/// [`crate::lower::lower_datalog`], then runs
/// [`Evaluator::run_query_typed`] with an empty query and an empty
/// [`SchemaRegistry`] so R226.M9's schema check runs on every atom the
/// program mentions. The three render arms cover:
///
/// * lowering failure — `lower: <LowerError>`.
/// * schema failure — `typecheck: K error(s)`. The R226.M9 pass batches
///   every diagnostic into one [`EvalError::TypeCheckErrors`], so a
///   single Error render is enough regardless of how many atoms fail.
/// * fixpoint success — `dlg: N facts` where N is the lowered
///   program's fact count. The empty-query path returns the empty
///   binding set on success, so `N` intentionally names the input's
///   fact count rather than the fixpoint's tuple count (the former is
///   what a caller who just typed a block wants to see confirmed).
/// * other evaluator failure (`UnstratifiedNegation`, aggregation,
///   pipeline-value refusal, …) — `run: <err>`.
///
/// `state.session_edb` is *not* consulted on this path: R226.M9's
/// typed query surface does not (yet) accept a session overlay; a
/// follow-on milestone re-wires the overlay through the typed pass.
fn execute_datalog(_state: &mut ReplState, node: &SyntaxNode) -> TurnResult {
    let program = match lower::lower_datalog(node) {
        Ok(p) => p,
        Err(e) => return TurnResult::Error(format!("lower: {e}")),
    };

    let evaluator = Evaluator::new();
    let registry = SchemaRegistry::new();
    let empty_query = Query { goals: Vec::new() };

    match evaluator.run_query_typed(&program, &empty_query, &registry) {
        Ok(_) => TurnResult::Value(format!("dlg: {} facts", program.facts.len())),
        Err(EvalError::TypeCheckErrors(errs)) => {
            TurnResult::Error(format!("typecheck: {} error(s)", errs.len()))
        }
        Err(other) => TurnResult::Error(format!("run: {other:?}")),
    }
}

/// R229.M3 — the `Cmd` branch of the executor.
///
/// Walks the AST `Cmd { name, args }` into a `(name_str, argv_vec)`
/// pair and hands them to [`cmd_dispatch::execute_cmd`]. Renders
/// `Ok(rendered)` as [`TurnResult::Value`] and `Err(cmd_err)` as
/// [`TurnResult::Error`] with a `cmd:` prefix — the executor's
/// convention is `<stage>: <message>` per the M1 doc.
///
/// # Head extraction
///
/// The `Cmd` node's `name` field is a `Box<SyntaxNode>`; a normal
/// pipeline parse (`ls foo bar`) makes it a
/// [`SyntaxNode::Ident`], and a lambda-context parse could in
/// principle make it a [`SyntaxNode::Var`] (see `ast::SyntaxNode::Var`
/// doc: "In Pipeline context this would appear as an `Ident` inside
/// `Cmd::name`"). M3 accepts either — a pipeline user should not care
/// whether the parser labelled the head Ident-in-Pipeline or Var-in-
/// Lambda for a bare name like `ls`. Anything else (a `FieldAccess`
/// chain — `bin/ls` — a `LitStr`, an inner `Cmd`) surfaces as
/// `cmd: non-name head` — the dispatch table is keyed by String and
/// there is no natural rendering of a nested node into a lookup key
/// that would not silently drop information.
///
/// # Argv collection
///
/// Each arg node contributes one string to the argv vector. `Ident`
/// and `Var` contribute their `name` (again accepting both since a
/// bare word can parse either way depending on context); `LitStr`
/// contributes its value verbatim (no shell-style quote stripping —
/// the parser already stripped the surrounding `"..."`). Any other
/// variant (a nested `Cmd`, a `Lambda`, a `RecordExpr`, …) contributes
/// its `Debug` form — the M3 shape is a "just so the argparse
/// typechecker sees SOMETHING" stub; M4 replaces the fallback with a
/// proper elaboration to `paideia_as_types::Value`.
fn execute_cmd_node(
    state: &mut ReplState,
    head: &SyntaxNode,
    args: &[SyntaxNode],
) -> TurnResult {
    let cmd_name = match cmd_head_name(head) {
        Some(name) => name,
        None => return TurnResult::Error("cmd: non-name head".into()),
    };
    let argv: Vec<String> = args.iter().map(arg_to_string).collect();
    match cmd_dispatch::execute_cmd(&state.cmd_registry, &cmd_name, &argv) {
        Ok(rendered) => TurnResult::Value(rendered),
        Err(err) => TurnResult::Error(format!("cmd: {err}")),
    }
}

/// Extract a `Cmd`'s head name as a plain string, accepting the two
/// AST variants a bare word can parse as (`Ident` in Pipeline context,
/// `Var` in a Lambda context). Returns `None` for a `FieldAccess` chain
/// (`bin/ls`), a `LitStr`, a nested `Cmd`, and anything else — the
/// dispatch registry is keyed by String and no natural rendering of a
/// nested node into a lookup key exists that would not silently drop
/// information.
///
/// Exposed `pub(crate)` so [`crate::pipeline::execute_pipeline`] can
/// reuse the same head-extraction rule as the top-level `Cmd` executor;
/// keeping both paths in sync avoids a class of bug where a bare `Cmd`
/// dispatches under a rule the pipeline stage rejects (or vice versa).
pub(crate) fn cmd_head_name(head: &SyntaxNode) -> Option<String> {
    match head {
        SyntaxNode::Ident { name, .. } | SyntaxNode::Var { name, .. } => Some(name.clone()),
        _ => None,
    }
}

/// Project a `Cmd` argument node into its argv-string shape.
///
/// See [`execute_cmd_node`]'s doc for why bare-name variants are
/// unwrapped (Ident + Var) and everything else falls through to
/// `Debug` — the fallback exists to keep the M3 stub compiling
/// against every future AST addition; M4's real elaborator replaces
/// it with a proper `SyntaxNode` → `paideia_as_cmd::Value` walk.
pub(crate) fn arg_to_string(node: &SyntaxNode) -> String {
    match node {
        SyntaxNode::Ident { name, .. } | SyntaxNode::Var { name, .. } => name.clone(),
        SyntaxNode::LitStr { value, .. } => value.clone(),
        SyntaxNode::LitInt { value, .. } => value.to_string(),
        SyntaxNode::LitBool { value, .. } => value.to_string(),
        other => format!("{other:?}"),
    }
}

/// Flatten a right-associated `Pipe` spine into a linear vector of
/// stage references.
///
/// The R221.M5 parser right-associates `a | b | c` to
/// `Pipe(a, Pipe(b, c))`; this walk yields `[a, b, c]` in source order.
/// A non-`Pipe` root produces a single-element vector — the caller
/// guards against this at the outer entry so it should not happen in
/// practice, but the walk stays defensive so a future caller (an
/// R229.M5 diagnostic layer replaying an arbitrary sub-node) can hand
/// any node in.
///
/// The returned `Vec<&SyntaxNode>` borrows from `node`; the pipeline
/// runner never needs owned nodes because each stage is dispatched by
/// walking its `name`/`args` fields into the M3 argv shape and never
/// mutates the AST.
fn flatten_pipe_stages(node: &SyntaxNode) -> Vec<&SyntaxNode> {
    let mut out = Vec::new();
    let mut cursor = node;
    loop {
        match cursor {
            SyntaxNode::Pipe { lhs, rhs, .. } => {
                out.push(lhs.as_ref());
                cursor = rhs.as_ref();
            }
            _ => {
                out.push(cursor);
                break;
            }
        }
    }
    out
}

/// Short human name for a `SyntaxNode` variant. Used only by the
/// generic executor fallback so the user's stub message names what
/// did not run. A `Debug`-derived match arm on the whole node would
/// dump the entire subtree — this keeps the message one word wide.
fn variant_name(n: &SyntaxNode) -> &'static str {
    match n {
        SyntaxNode::Cmd { .. } => "cmd",
        SyntaxNode::Pipe { .. } => "pipe",
        SyntaxNode::Seq { .. } => "seq",
        SyntaxNode::Redirect { .. } => "redirect",
        SyntaxNode::Background { .. } => "background",
        SyntaxNode::Group { .. } => "group",
        SyntaxNode::DatalogBlock { .. } => "datalog-block",
        SyntaxNode::Atom { .. } => "atom",
        SyntaxNode::Rule { .. } => "rule",
        SyntaxNode::QVar { .. } => "qvar",
        SyntaxNode::InterpVar { .. } => "interp-var",
        SyntaxNode::NotAtom { .. } => "not-atom",
        SyntaxNode::Lambda { .. } => "lambda",
        SyntaxNode::App { .. } => "app",
        SyntaxNode::Var { .. } => "var",
        SyntaxNode::Let { .. } => "let",
        SyntaxNode::Match { .. } => "match",
        SyntaxNode::BinOp { .. } => "binop",
        SyntaxNode::UnaryOp { .. } => "unaryop",
        SyntaxNode::FieldAccess { .. } => "field-access",
        SyntaxNode::RecordExpr { .. } => "record",
        SyntaxNode::LitStr { .. } => "lit-str",
        SyntaxNode::LitInt { .. } => "lit-int",
        SyntaxNode::LitBool { .. } => "lit-bool",
        SyntaxNode::Ident { .. } => "ident",
    }
}

/// R229.M6 — evaluate a `let` RHS under the session's `value_env` and,
/// on success, insert the resulting binding so subsequent turns see it.
///
/// # Contract
///
/// * Reads `state.value_env` as the environment for `eval_lambda` so
///   the RHS can reference previously-bound names — chained lets like
///   `let x = 1; let y = x + 1` work because turn 1 has already
///   installed `x` before turn 2 evaluates `x + 1`.
/// * On `Ok(v)`, inserts `(name, v.clone())` into `state.value_env`
///   (mutating the session) *and* extends `state.type_env` with a
///   maximally-permissive `∀α. α` scheme for `name` (see the inline
///   comment in the body — HM would otherwise trip `UnboundVar` on
///   the next turn's reference to `name`), then returns the value.
/// * On `Err(e)`, leaves *both* `state.value_env` and `state.type_env`
///   untouched (the `?` early-returns before either mutation) — a
///   failed binding must not leak a partial mutation into the session.
///
/// # Shadowing
///
/// `HashMap::insert` overwrites any prior entry for `name`, which is
/// the shadowing semantic the R229 milestone doc calls out:
/// `let x = 1; let x = 99` leaves `x → 99` in the environment. The
/// M6 fixture `r229m6-let-04` pins this.
///
/// # Not a full turn
///
/// This helper does not bump `state.turn_counter` — the caller
/// ([`eval_let_binding`] or the [`execute`] dispatch) owns that. Kept
/// as a pure `(state, name, value_node) → Result<Value, _>` so a
/// future replay harness can call it deterministically without
/// re-driving the whole `eval_turn` pipeline.
pub fn execute_let(
    state: &mut ReplState,
    name: &str,
    value_node: &SyntaxNode,
) -> Result<Value, LambdaError> {
    let v = lambda_eval::eval_lambda(value_node, &state.value_env)?;
    state.value_env.insert(name.to_owned(), v.clone());
    // Keep `type_env` in step with `value_env` so a subsequent turn
    // that references `name` under HM (stage 2 of `eval_turn`) does not
    // abort with `type: unbound variable …`. R225.M4's HM checker
    // covers only a lambda subset (see `crate::type_stage` module doc)
    // and cannot always infer a precise scheme for the R229.M5 lambda
    // walker's runtime values — a `BinOp`-shaped RHS surfaces as
    // `UnsupportedNode` before HM ever sees the whole term. So M6
    // installs a maximally-permissive scheme `∀α. α` per binding:
    // instantiation at each use site mints a fresh var that HM unifies
    // against whatever the context needs, which is the correct
    // "type-check does not obstruct execution" story until a follow-on
    // milestone lifts BinOp / UnaryOp / LitBool into the HM subset and
    // execute_let can install the real inferred scheme.
    let wildcard = wildcard_scheme();
    state.type_env = state.type_env.extend(name.to_owned(), wildcard);
    Ok(v)
}

/// A polymorphic "any type" scheme `∀α. α`, used by [`execute_let`] to
/// keep `state.type_env` from tripping HM's UnboundVar on names the R229
/// walker installs into `value_env`. Instantiation at each use site
/// mints a fresh type variable that HM unifies freely — the scheme
/// commits to nothing about the value's runtime shape.
///
/// The `TypeVar(0)` id is arbitrary because the scheme quantifies it —
/// after `generalize`, the body's fresh copy carries a different id at
/// every use site (see [`paideia_as_shell_hm::infer::instantiate`]),
/// so no cross-scheme aliasing can occur.
fn wildcard_scheme() -> TypeScheme {
    let alpha = TypeVar(0);
    TypeScheme {
        quantified: vec![alpha],
        body: MonoType::Var(alpha),
    }
}

/// R229.M6 — public entry point for injecting a top-level `let`
/// binding into a session without going through the source parser.
///
/// # Why a helper (and not `eval_turn` with source)
///
/// The R221.M5 pipeline parser does not (yet) produce a top-level
/// `SyntaxNode::Let` — a bare `let x = 42` fails at the lexer (`=` is
/// not a recognised operator glyph). M6 lands the state-mutation
/// substrate that a follow-on parser milestone will wire into
/// `eval_turn`; until then, drivers (and the M6 test corpus) construct
/// a `SyntaxNode::Let`-shaped RHS by hand and hand it in through this
/// helper. When the parser catches up, this helper stays as the
/// programmatic path (an LSP action, a session-replay harness, a
/// driver that seeds prelude bindings) and `eval_turn`'s dispatch
/// begins reaching the [`SyntaxNode::Let`] arm on real source.
///
/// # Turn accounting
///
/// Bumps `state.turn_counter` and returns a [`TurnResult`] on the same
/// shape `eval_turn` uses — a driver that mixes `eval_turn` calls with
/// `eval_let_binding` calls sees a single monotone counter across the
/// session (M6 fixture `r229m6-let-08` pins the counter reaching 10
/// across a 10-turn mix).
///
/// # Rendering
///
/// On success, returns `TurnResult::Value("{name} = {rendered}")`
/// where `rendered` follows the same M5 rules as the [`SyntaxNode::Lambda`]
/// arm (`Int` → digits, `Str` → verbatim, `Bool` → "true"/"false",
/// `Unit` → "()", `Fn` → "\<closure\>"). On failure, returns
/// `TurnResult::Error("lambda: {err}")` and leaves the session
/// unchanged — consistent with the [`execute_let`] contract.
pub fn eval_let_binding(
    state: &mut ReplState,
    name: &str,
    value: SyntaxNode,
) -> TurnResult {
    // Match `eval_turn`'s counter contract: always bump, even on
    // error, so a fingerprint sequence has no gaps.
    state.turn_counter += 1;
    match execute_let(state, name, &value) {
        Ok(v) => TurnResult::Value(format!("{name} = {}", render_value(&v))),
        Err(e) => TurnResult::Error(format!("lambda: {e}")),
    }
}

/// Render a [`Value`] for user-visible output.
///
/// Centralised so the [`SyntaxNode::Lambda`] executor arm, the
/// [`SyntaxNode::Let`] executor arm, and [`eval_let_binding`] all
/// project a value onto the same string surface — a future addition
/// of, say, a hex-int renderer or a truncated-closure form has one
/// place to touch.
fn render_value(v: &Value) -> String {
    match v {
        Value::Int(n) => format!("{n}"),
        Value::Str(s) => s.clone(),
        Value::Bool(b) => format!("{b}"),
        Value::Unit => "()".into(),
        Value::Fn(_) => "<closure>".into(),
    }
}
