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
//! # Executor branches (R229.M1)
//!
//! * [`SyntaxNode::DatalogBlock`] — re-tokenize the source, slice out
//!   the `Context::Datalog` tokens (excluding the wrapping braces),
//!   call [`paideia_as_shell_datalog::parser::parse_block`], then run
//!   [`Evaluator::run_stratified_with_session`] with the session EDB
//!   as the overlay. Renders "dlg: N facts loaded" where N is the
//!   fixpoint's total tuple count. The re-tokenization exists because
//!   the shell-ast parser already produced a `SyntaxNode::DatalogBlock`
//!   over the same source, but its item shape (`SyntaxNode::Atom` /
//!   `SyntaxNode::Rule`) is not what R226's evaluator consumes
//!   (`Program { rules, facts }`). R229.M2's typed elaborator replaces
//!   this bridge with a proper AST → Program lowering; M1 keeps the
//!   double-parse to avoid landing an adapter that will be discarded.
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
use paideia_as_shell_datalog::{parser as dl_parser, Evaluator, SessionEdb};
use paideia_as_shell_lex::{Context, Token, TokenKind};

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
    let result = execute(state, &source, elaborated);

    ReplTurn { source, result, fingerprint }
}

/// Dispatch the executor by AST root variant. R229.M1's four real
/// arms (plus the fallback) map one-to-one to the four sub-language
/// surfaces the shell composes.
fn execute(state: &mut ReplState, source: &str, node: &SyntaxNode) -> TurnResult {
    match node {
        SyntaxNode::DatalogBlock { .. } => execute_datalog(state, source),
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

/// Datalog branch of the executor. See the module doc for why we
/// re-tokenize rather than lower the shell-ast items in-place.
fn execute_datalog(state: &mut ReplState, source: &str) -> TurnResult {
    // Re-normalize the source with the same NFC pass the shell-ast
    // parser used, then re-tokenize. `tokenize_ok` panics on lex
    // errors; the shell-ast parser succeeded on the same input, so
    // any lex error here would be a bug (we would rather surface it
    // as a crash in tests than silently mask it).
    let nfc = paideia_as_unicode::nfc_normalize(source);
    let tokens = paideia_as_shell_lex::tokenize_ok(&nfc);

    // Slice out the Datalog-context tokens, dropping the wrapping
    // `{ … }` (both stamped `Datalog` per the R221.M4 delimiter
    // arithmetic). `parse_block` refuses those braces at its first
    // token (it wants a predicate name or clause opener).
    let dl_tokens: Vec<Token> = tokens
        .into_iter()
        .filter(|t| {
            t.context == Context::Datalog
                && !matches!(t.kind, TokenKind::LBrace | TokenKind::RBrace)
        })
        .collect();

    let program = match dl_parser::parse_block(&dl_tokens) {
        Ok(p) => p,
        Err(err) => return TurnResult::Error(format!("parse: dlg: {:?}", err)),
    };

    // R226.M8: run the fixpoint with the session EDB as overlay. The
    // Database result carries `.total_tuple_count()` — every ground
    // fact loaded (program facts + session overlay + any IDB tuples
    // that fired) in one number.
    let evaluator = Evaluator::new();
    match evaluator.run_stratified_with_session(&program, &state.session_edb) {
        Ok(db) => TurnResult::Value(format!("dlg: {} facts loaded", db.total_tuple_count())),
        Err(err) => TurnResult::Error(format!("exec: dlg: {:?}", err)),
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
