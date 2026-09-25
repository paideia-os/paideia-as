//! R229.M3 — REPL command dispatch: name-keyed [`CommandSig`] registry
//! plus a stub `execute_cmd` that runs the R222.M4 argv typechecker and
//! returns a rendered scalar tag.
//!
//! # Why a separate module (not inlined into `turn.rs`)
//!
//! The turn executor's `SyntaxNode::Cmd` arm collects an argv from the
//! AST and then hands it to a *dispatcher*: name → `CommandSig` lookup,
//! argv → argparse type-check, and (M4+) a real `execute` op invocation.
//! Keeping the dispatcher in its own module means:
//!
//! * The executor branch in `turn.rs` stays a two-liner (`collect argv`
//!   + `execute_cmd`), matching the shape the R229.M4 pipeline stage
//!   integration will thread a `Stream<Record>` through — the arm gets
//!   longer by lines that talk about streams, not lines that talk
//!   about dispatch.
//! * The R222 command surface (`CommandSig`, `argparse::parse_argv`)
//!   is a substantial API; touching only its re-exports from a single
//!   module keeps `use` lines out of `turn.rs`.
//! * A future R229.M5 diagnostics layer that wants to render a
//!   dispatch error keyed on `CmdError` discriminant can `use crate::
//!   cmd_dispatch::CmdError` without pulling the turn pipeline into
//!   scope.
//!
//! # M3 execution model (stub)
//!
//! The R222.M2 `CommandSig::execute` op has the shape
//! `fn(&InvocationCtx) -> ExecuteResult`, and `ExecuteResult` carries
//! a `u64 scalar` + a fingerprint — a shape designed for a streams-in
//! world where `find` returns the number of records it emitted. That
//! world does not exist yet in the REPL: no `Stream<Record>` handle,
//! no capability environment, no fingerprint threading through the
//! turn pipeline. Invoking `execute` today would return a scalar
//! whose meaning is command-specific and whose downstream renderer
//! (the M4/M5 pipeline evaluator) is absent — a number the user
//! cannot interpret is a worse UX than an explicit "ok" tag.
//!
//! M3 therefore lands *dispatch shape only*: lookup the sig, run the
//! argv typechecker, and return `cmd: <name> ok (<n> args)` on
//! success. R229.M4 wires a real `InvocationCtx` (with a per-turn
//! fingerprint from the enclosing `ReplTurn`) and calls
//! `sig.execute(&ctx)`; R229.M5 lets a pipe stage read the previous
//! stage's `ExecuteResult` into the next stage's ctx.
//!
//! # Registry shape
//!
//! The interim registry is `HashMap<String, CommandSig>` — the same
//! shape [`paideia_as_shell_cmd::CommandRegistry`] uses today for
//! functor lookup (the field there is
//! `HashMap<String, CommandFunctor>` — a functor `fn(&SchemasSig) ->
//! CommandSig`; the REPL's registry stores the *already-instantiated*
//! sig because the REPL has a single session-wide `SchemasSig` and
//! reifying per turn buys nothing). The map is name-case-sensitive:
//! `Ls` and `ls` are distinct entries. Case-folding belongs at a
//! higher layer (a shell-user configuration alias table, R229.M6) and
//! encoding it here would silently swallow a user's typo instead of
//! surfacing it as a dispatch miss.

use std::collections::HashMap;
use std::error::Error;
use std::fmt;

use paideia_as_shell_cmd::{parse_argv, ArgParseError, CommandSig};

use crate::lambda_eval::Value;

/// Name-keyed registry of already-instantiated [`CommandSig`] handles.
///
/// A driver (interactive REPL, `.pds` runner, R229.M7 replay harness)
/// registers commands once at session start via [`Self::register`], and
/// the executor's `Cmd` branch consults it read-only via
/// [`Self::lookup`] on every turn.
///
/// # Why not `HashMap<String, CommandFunctor>`
///
/// The [`paideia_as_shell_cmd::CommandRegistry`] map values are
/// `fn(&SchemasSig) -> CommandSig` — a driver-side functor that binds a
/// per-session `SchemasSig` at lookup time. The REPL's session has a
/// single `SchemasSig` for its whole lifetime (per R222 §6.1 the
/// signature is stable across a session), so pre-instantiating and
/// storing the `CommandSig` directly:
///
/// * saves an allocation + functor-call on the hot dispatch path;
/// * removes the need for `ReplState` to carry a `SchemasSig` field
///   (that field will land in R229.M6 when session-scoped schema
///   overrides arrive; forcing it into the M3 shape prematurely
///   couples the M3 registry to a schema surface it does not yet use).
///
/// A driver that wants the functor shape (e.g. a session that swaps its
/// `SchemasSig` mid-run — not a use case R229 supports, but a shape a
/// future driver might want) can wrap the functor at registration:
/// `reg.register("find", find::functor(&schemas))`. The registry does
/// not need to change.
#[derive(Clone, Debug, Default)]
pub struct CmdDispatchRegistry {
    /// Name-to-sig table. Case-sensitive (see module doc).
    entries: HashMap<String, CommandSig>,
}

impl CmdDispatchRegistry {
    /// Empty registry — the shape a driver constructs before wiring any
    /// commands, and the shape [`crate::ReplState::default`] leaves the
    /// `cmd_registry` field in.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a command sig under `name`. If a sig is already
    /// registered under `name`, it is replaced — last-registration-wins
    /// matches the [`CommandRegistry::register`](paideia_as_shell_cmd::CommandRegistry)
    /// shape and lets a driver override a built-in without a special
    /// call.
    pub fn register(&mut self, name: impl Into<String>, sig: CommandSig) {
        self.entries.insert(name.into(), sig);
    }

    /// Look up a sig by exact-byte name. Returns `None` on miss (see
    /// module doc for the case-sensitivity rationale).
    pub fn lookup(&self, name: &str) -> Option<&CommandSig> {
        self.entries.get(name)
    }

    /// Number of registered entries — kept `pub` because a driver
    /// building a `describe` prompt or the R229.M6 replay-alignment
    /// harness wants to sanity-check the registry size without
    /// iterating.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Failure modes for [`execute_cmd`] and [`crate::pipeline::execute_pipeline`].
///
/// Kept as a small enum rather than a boxed `dyn Error`: each variant
/// names a distinct architectural axis (unknown name, argv type-check
/// failure, dispatch not yet implemented, pipeline structural halt) so a
/// diagnostic layer (R229.M5) can key directly off the discriminant
/// without string-parsing. `Display` reads as `unknown command: X` /
/// `argparse: ...` / `not yet implemented` / `pipeline halted at stage
/// N (reason)` — the leading tag lets a caller strip or reformat
/// without re-inspecting the enum.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CmdError {
    /// The command name did not resolve to a registered [`CommandSig`].
    UnknownCommand(String),
    /// The argv did not typecheck against the sig's positional
    /// [`paideia_as_shell_cmd::ArgSpec`] list.
    ArgParseFailed(String),
    /// A dispatch path this milestone does not support was reached.
    /// R229.M3 does not surface this today — it stays in the enum so
    /// M4's `execute` call site (which returns
    /// `ExecuteResult`-transformed values that R229 has no consumer
    /// for yet) has a home to route "we recognized this but cannot yet
    /// run it".
    NotImplemented,
    /// A pipeline stage could not be dispatched *at the pipeline shape*
    /// — e.g. a non-`SyntaxNode::Cmd` stage, or a `Cmd` head that is not
    /// a bare name. Distinct from `UnknownCommand`/`ArgParseFailed`
    /// (which are Cmd-scope errors that
    /// [`crate::pipeline::execute_pipeline`] captures into
    /// [`crate::pipeline::PipelineResult`] so partial stage outputs
    /// survive); a `PipelineHalted` at this level means the pipeline
    /// could not run *at all* past index `stage_idx` and there is no
    /// partial state worth preserving.
    PipelineHalted {
        /// 0-based index of the stage at which the halt occurred.
        stage_idx: usize,
        /// Human-readable reason (the underlying non-Cmd node's
        /// `Debug`, or a "non-name head" tag).
        reason: String,
    },
}

impl fmt::Display for CmdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CmdError::UnknownCommand(name) => write!(f, "unknown command: {name}"),
            CmdError::ArgParseFailed(msg) => write!(f, "argparse: {msg}"),
            CmdError::NotImplemented => f.write_str("not yet implemented"),
            CmdError::PipelineHalted { stage_idx, reason } => {
                write!(f, "pipeline halted at stage {stage_idx} ({reason})")
            }
        }
    }
}

impl Error for CmdError {}

/// Look up `name` in `reg`, typecheck `args` against the sig's
/// positional arg specs, and return the typed [`Value`] the command
/// produced.
///
/// # Return shape (R229.M8)
///
/// M3–M7 returned `Result<String, CmdError>` — a rendered success tag
/// like `cmd: <name> ok (K args)`, which the pipeline runner lifted
/// into a `Value::Str` at the stage boundary. R229.M8 lifts that lift
/// *into* the dispatcher: `execute_cmd` now returns a typed
/// [`Value`] directly, and the pipeline runner threads the value
/// through unchanged. The stage-boundary `Value::Str(rendered)` wrap
/// disappears — a command that wants to emit an `Int` (or a `Bool`,
/// or a closure) no longer has to round-trip through a rendered
/// string.
///
/// # Milestone-stub demonstrator commands
///
/// M8 does not yet grow [`CommandSig::execute`] itself to return a
/// [`Value`] (that is a follow-on that also has to teach the
/// `InvocationCtx` / `ExecuteResult` shape about `Value`). Instead,
/// M8 special-cases *two* command names inside this dispatcher — a
/// deliberately small demonstrator surface that proves the typed
/// return path end-to-end without touching `paideia-as-shell-cmd`:
///
/// * `count` — returns `Value::Int(args.len() as i64)`. Pinned by
///   the `r229m8-cmd-*` corpus as the "int-typed return" fixture and
///   used downstream in pipeline tests to prove a typed `Value::Int`
///   projects back into an argv token on the next stage's input.
/// * `echo` — returns `Value::Str(args.join(" "))`. The "user
///   content" fixture: proves the dispatcher can carry an argv-
///   derived payload without smuggling it through the M3–M7 render
///   tag.
///
/// Every other registered command falls through to the M3 render
/// (`format!("cmd: {name} ok ({K} args)")`) wrapped as `Value::Str`
/// — the M4/M7 corpora that use `a`, `b`, `c`, `d`, `ls`, `wc` see
/// the same string payload they saw pre-M8. The special-case is
/// keyed on `name`, not on `CommandSig` fields, so a fixture can
/// register `count` under any valid sig — the demonstrator overrides
/// the generic tag but the argparse pre-check (below) still runs.
///
/// # Argparse coupling
///
/// The dispatcher runs [`paideia_as_shell_cmd::parse_argv`] over
/// `args` before choosing a return shape, so a user typing `head foo`
/// still sees the `Int`-expected error the same way the R222.M4
/// tests do. If argparse fails, the special-case for `count` / `echo`
/// is *not* consulted — the `ArgParseFailed` error surfaces first,
/// matching the M3 contract.
///
/// # Flag handling
///
/// Flags are NOT parsed here. The `Cmd` node's `args` field is a flat
/// `Vec<SyntaxNode>` and the executor does not (yet) distinguish
/// `--flag=value` from a positional argument. Splitting argv into
/// positional-vs-flag lists is a follow-on milestone; today every
/// argv token is treated as positional and `parse_argv` reports the
/// shape mismatch when a `--flag`-shaped token lands on an `ArgSpec`
/// slot.
pub fn execute_cmd(
    reg: &CmdDispatchRegistry,
    name: &str,
    args: &[String],
) -> Result<Value, CmdError> {
    let sig = reg
        .lookup(name)
        .ok_or_else(|| CmdError::UnknownCommand(name.to_owned()))?;

    // Argv typecheck — R222.M4. `parse_argv` handles an empty argv
    // against an empty spec vector by returning `Ok(vec![])`, so the
    // common "sig has no args" case flows through unchanged. The
    // typecheck runs *before* the M8 demonstrator special-case so a
    // sig-argv shape mismatch surfaces with its own error even when
    // the name is `count` / `echo` — the special-case is a payload
    // choice, not a bypass of the sig contract.
    let owned: Vec<String> = args.to_vec();
    match parse_argv(&sig.arguments, &owned) {
        Ok(_values) => Ok(match name {
            // M8 demonstrator: int-typed return payload. Keyed on
            // name so a fixture registered under `nullary_sig("count")`
            // still gets `Value::Int(args.len())` — the argparse
            // above already accepted the argv shape against whatever
            // sig the driver chose.
            "count" => Value::Int(args.len() as i64),
            // M8 demonstrator: user-content string payload. Argv
            // tokens joined by a single ASCII space; empty argv
            // produces an empty string.
            "echo" => Value::Str(args.join(" ")),
            // Generic path — same rendered tag M3 introduced, now
            // wrapped as a `Value::Str` so the runner can treat every
            // return uniformly.
            _ => Value::Str(format!("cmd: {name} ok ({} args)", args.len())),
        }),
        Err(e) => Err(CmdError::ArgParseFailed(argparse_display(&e))),
    }
}

/// Render an [`ArgParseError`] as a one-line string. Kept private
/// because the R229.M5 diagnostics layer will format these against a
/// source-span-carrying `miette::Diagnostic` shape once
/// `paideia-as-shell-cmd` derives it (per its own R222.M8 note);
/// exposing this shape publicly today would freeze a rendering the
/// diagnostics layer will replace.
fn argparse_display(e: &ArgParseError) -> String {
    // `ArgParseError` implements `Display` — delegate. The extra
    // indirection lets us later prepend a per-error tag ("missing",
    // "type-mismatch") from the discriminant without touching every
    // call site.
    format!("{e}")
}
