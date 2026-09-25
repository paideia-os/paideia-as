//! R222.M3 — functor instantiation at the REPL prompt.
//!
//! Given a raw shell line (`"find ."`) and the session's
//! [`SchemasSig`], this module resolves the head against the
//! [`CommandRegistry`], instantiates the functor, elaborates argv
//! into an [`InvocationCtx`], and returns an [`Invocation`] the
//! caller invokes `execute` on.
//!
//! # Line elaboration — R222.M3 interim
//!
//! Whitespace split only — no quote handling, no `|` awareness. Those
//! land at R221.M5 (unified AST parser) and R223.M1 (pipeline stage
//! parser); this module gains a `from_tokens(&[Token]) ->
//! Result<Invocation, _>` alongside `from_line` when those milestones
//! close, at which point this crate takes on
//! `paideia-as-shell-lex` as a dep. Doing the coupling then, not
//! now, keeps R222 reviewable in isolation.
//!
//! # Fingerprint tagging (R220.M10 shape)
//!
//! Every dispatch stamps a per-turn fingerprint through
//! [`InvocationCtx::fingerprint`] and the returned [`Invocation`].
//! Callers pass the tag they want on the wire (typically the R222.M3
//! canary shape `"r222-m3-cmd-NN"`). The R229 REPL will generate
//! these from a per-session monotonic turn counter when it lands.

use crate::commands::CommandFunctor;
use crate::registry::CommandRegistry;
use crate::schema::SchemasSig;
use crate::sig::{CommandSig, ExecuteResult, InvocationCtx};

/// A resolved-but-not-yet-executed command invocation.
///
/// Holds the concrete [`CommandSig`] returned by functor
/// instantiation, the elaborated argv, and the fingerprint tag the
/// caller supplied. [`Invocation::execute`] runs the command's op
/// against the ctx and returns an [`ExecuteResult`].
#[derive(Clone, Debug)]
pub struct Invocation {
    /// Concrete signature returned by the functor.
    pub sig: CommandSig,
    /// Elaborated argv (does NOT include the command name).
    pub argv: Vec<String>,
    /// Per-turn fingerprint (R220.M10 shape).
    pub fingerprint: String,
}

impl Invocation {
    /// Invoke the command's `execute` op against `argv` + `fingerprint`.
    pub fn execute(&self) -> ExecuteResult {
        let ctx = InvocationCtx {
            argv: self.argv.clone(),
            fingerprint: self.fingerprint.clone(),
        };
        (self.sig.execute)(&ctx)
    }
}

/// What can go wrong at dispatch time.
///
/// Kept as a plain enum rather than a `miette::Diagnostic` because
/// R222.M3 is not yet on the diagnostics rail — that arrives at
/// R222.M4 (ArgSpec/FlagSpec typing) when the elaborator wants
/// diagnostics pointed at the ArgSpec's source span. When it does,
/// the shape of these variants stays the same; the trait derive gets
/// added.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DispatchError {
    /// The line was empty (or all-whitespace).
    EmptyLine,
    /// The command name was not in the registry.
    UnknownCommand(String),
    /// A required positional argument was missing (R222.M3 only checks
    /// count; type-check against `ArgSpec::type_name` is R222.M4).
    MissingRequiredArg {
        /// Command that was resolved.
        command: String,
        /// Name of the argument the command declared as required.
        arg_name: String,
    },
}

impl std::fmt::Display for DispatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyLine => f.write_str("empty shell line"),
            Self::UnknownCommand(name) => write!(f, "unknown command `{name}`"),
            Self::MissingRequiredArg { command, arg_name } => {
                write!(f, "command `{command}` missing required argument `{arg_name}`")
            }
        }
    }
}

impl std::error::Error for DispatchError {}

/// The R222.M3 REPL entry point.
///
/// Splits `line` on whitespace (interim — R221.M5 tokens replace this
/// at parser integration), resolves the head against `registry`,
/// instantiates the functor against `schemas`, checks required-arg
/// count, and returns the [`Invocation`].
pub fn dispatch(
    registry: &CommandRegistry,
    schemas: &SchemasSig,
    line: &str,
    fingerprint_tag: &str,
) -> Result<Invocation, DispatchError> {
    let mut parts = line.split_whitespace();
    let head = parts.next().ok_or(DispatchError::EmptyLine)?;
    let functor: CommandFunctor = registry
        .resolve(head)
        .ok_or_else(|| DispatchError::UnknownCommand(head.to_owned()))?;
    let sig = functor(schemas);
    let argv: Vec<String> = parts.map(String::from).collect();

    // Interim required-arg check: count positional required args and
    // refuse if argv is shorter. The type-check against
    // `ArgSpec::type_name` waits for R222.M4.
    let required_count = sig.arguments.iter().filter(|a| a.required).count();
    if argv.len() < required_count {
        // Pick the first missing required arg by scanning in order.
        let missing = sig
            .arguments
            .iter()
            .filter(|a| a.required)
            .nth(argv.len())
            .expect("required_count > argv.len() implies a missing slot");
        return Err(DispatchError::MissingRequiredArg {
            command: sig.name.clone(),
            arg_name: missing.name.clone(),
        });
    }

    Ok(Invocation {
        sig,
        argv,
        fingerprint: fingerprint_tag.to_owned(),
    })
}
