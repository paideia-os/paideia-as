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

use crate::cmd_dispatch::{self, CmdDispatchRegistry};
use crate::lower;

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
            };
        }
    };

    // Stage 2: typecheck. R229.M1 stub — always Ok. R229.M2 wires
    // `paideia-as-shell-hm`'s Algorithm W over the AST.
    // (No code: identity.)

    // Stage 3: elaborate. R229.M1 identity — the AST-as-is is passed
    // to the executor. R229.M2+ lowers to a typed IR here.
    let elaborated = &node;

    // Stage 4: execute — dispatch by variant.
    let result = execute(state, elaborated);

    ReplTurn { source, result, fingerprint }
}

/// Dispatch the executor by AST root variant. R229.M2's four real
/// arms (plus the fallback) map one-to-one to the four sub-language
/// surfaces the shell composes.
fn execute(state: &mut ReplState, node: &SyntaxNode) -> TurnResult {
    match node {
        SyntaxNode::DatalogBlock { .. } => execute_datalog(state, node),
        SyntaxNode::Cmd { name, args, .. } => execute_cmd_node(state, name, args),
        SyntaxNode::Pipe { .. } => {
            // R229.M3 pipeline stub — the M2 render was "not yet
            // implemented". M3 upgrades the tag to `pipe:` so the user
            // sees dispatch attribution (their `a | b | c` did reach
            // the pipe arm, not fall through as a Cmd). Real
            // stage-to-stage value threading — assembling each stage's
            // `InvocationCtx`, running the R222.M4 argparse, calling
            // sig.execute, threading the previous stage's `ExecuteResult`
            // scalar into the next stage's ctx — is R229.M4.
            let stages = count_pipe_stages(node);
            TurnResult::Value(format!("pipe: {stages} stages"))
        }
        SyntaxNode::Lambda { .. } => {
            TurnResult::Value("lambda: <not yet implemented>".into())
        }
        // Everything else — Seq, Group, Redirect, RecordExpr, literals,
        // Atom/Rule outside a block, App/Var/Let/Match, BinOp/UnaryOp,
        // FieldAccess, QVar/InterpVar/NotAtom, Ident — falls into the
        // generic "not yet implemented" bucket. Naming the variant keeps
        // the user's error message specific without hard-coding twenty
        // arms of essentially the same string.
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
    let cmd_name = match head {
        SyntaxNode::Ident { name, .. } | SyntaxNode::Var { name, .. } => name.clone(),
        _ => return TurnResult::Error("cmd: non-name head".into()),
    };
    let argv: Vec<String> = args.iter().map(arg_to_string).collect();
    match cmd_dispatch::execute_cmd(&state.cmd_registry, &cmd_name, &argv) {
        Ok(rendered) => TurnResult::Value(rendered),
        Err(err) => TurnResult::Error(format!("cmd: {err}")),
    }
}

/// Project a `Cmd` argument node into its argv-string shape.
///
/// See [`execute_cmd_node`]'s doc for why bare-name variants are
/// unwrapped (Ident + Var) and everything else falls through to
/// `Debug` — the fallback exists to keep the M3 stub compiling
/// against every future AST addition; M4's real elaborator replaces
/// it with a proper `SyntaxNode` → `paideia_as_cmd::Value` walk.
fn arg_to_string(node: &SyntaxNode) -> String {
    match node {
        SyntaxNode::Ident { name, .. } | SyntaxNode::Var { name, .. } => name.clone(),
        SyntaxNode::LitStr { value, .. } => value.clone(),
        SyntaxNode::LitInt { value, .. } => value.to_string(),
        SyntaxNode::LitBool { value, .. } => value.to_string(),
        other => format!("{other:?}"),
    }
}

/// Count the linear stages of a right-associated `Pipe` spine.
///
/// The R221.M5 parser right-associates `a | b | c` to
/// `Pipe(a, Pipe(b, c))`; walking the right spine gives 3 for that
/// input. A non-`Pipe` root counts as one stage — the caller guards
/// this so it never happens at the outer entry, but the walk stays
/// defensive so a future caller can hand any node in.
fn count_pipe_stages(node: &SyntaxNode) -> usize {
    let mut n = 1usize;
    let mut cursor = node;
    while let SyntaxNode::Pipe { rhs, .. } = cursor {
        n += 1;
        cursor = rhs;
    }
    n
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
