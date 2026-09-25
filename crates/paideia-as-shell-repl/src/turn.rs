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
use paideia_as_shell_datalog::{
    EvalError, Evaluator, Query, SchemaRegistry, SessionEdb,
};

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
}

impl ReplState {
    /// Fresh session: turn counter at 0, empty session EDB.
    pub fn new() -> Self {
        Self::default()
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
        SyntaxNode::Cmd { .. } | SyntaxNode::Pipe { .. } => {
            TurnResult::Value("cmd: <not yet implemented in R229.M1>".into())
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
