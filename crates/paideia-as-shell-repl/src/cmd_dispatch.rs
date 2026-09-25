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

/// Failure modes for [`execute_cmd`].
///
/// Kept as a three-variant enum rather than a boxed `dyn Error`: each
/// variant names a distinct architectural axis (unknown name, argv
/// type-check failure, dispatch not yet implemented) so a diagnostic
/// layer (R229.M5) can key directly off the discriminant without
/// string-parsing. `Display` reads as `unknown command: X` / `argparse:
/// ...` / `not yet implemented` — the leading tag lets a caller strip
/// or reformat without re-inspecting the enum.
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
}

impl fmt::Display for CmdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CmdError::UnknownCommand(name) => write!(f, "unknown command: {name}"),
            CmdError::ArgParseFailed(msg) => write!(f, "argparse: {msg}"),
            CmdError::NotImplemented => f.write_str("not yet implemented"),
        }
    }
}

impl Error for CmdError {}

/// Look up `name` in `reg`, typecheck `args` against the sig's
/// positional arg specs, and return a rendered success tag.
///
/// # Milestone stub
///
/// M3 returns `format!("cmd: {name} ok ({n} args)")` on the happy path.
/// Real invocation (assembling an [`InvocationCtx`](paideia_as_shell_cmd::InvocationCtx),
/// calling `sig.execute(&ctx)`, rendering the resulting scalar) is
/// R229.M4/M5's landing — see the module doc for why deferring is a
/// UX win, not just a scoping convenience.
///
/// # Argparse coupling
///
/// The M3 stub *does* run [`paideia_as_shell_cmd::parse_argv`] over
/// `args` so a user typing `head foo` sees the `Int`-expected error the
/// same way the R222.M4 tests do. If the sig has no positional
/// arguments (M3 test fixtures often use this shape), `parse_argv`
/// accepts an empty argv and immediately returns; the wrapper still
/// reports `ok (0 args)`.
///
/// # Flag handling
///
/// Flags are NOT parsed here. The `Cmd` node's `args` field is a flat
/// `Vec<SyntaxNode>` and the R229.M3 executor does not (yet)
/// distinguish `--flag=value` from a positional argument. Splitting
/// argv into positional-vs-flag lists is R229.M4's landing (it needs
/// the same split for `Pipe` stage input threading); M3 treats every
/// argv token as positional and lets `parse_argv` report the shape
/// mismatch when a `--flag`-shaped token lands on an `ArgSpec` slot.
pub fn execute_cmd(
    reg: &CmdDispatchRegistry,
    name: &str,
    args: &[String],
) -> Result<String, CmdError> {
    let sig = reg
        .lookup(name)
        .ok_or_else(|| CmdError::UnknownCommand(name.to_owned()))?;

    // Argv typecheck — R222.M4. `parse_argv` handles an empty argv
    // against an empty spec vector by returning `Ok(vec![])`, so the
    // common "sig has no args" case flows through unchanged.
    let owned: Vec<String> = args.to_vec();
    match parse_argv(&sig.arguments, &owned) {
        Ok(_values) => Ok(format!("cmd: {name} ok ({} args)", args.len())),
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
