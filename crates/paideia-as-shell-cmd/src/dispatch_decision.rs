//! R222.M6 — light vs heavy command dispatch decision.
//!
//! Given a concrete [`CommandSig`], [`decide_dispatch`] returns a
//! [`DispatchTarget`] naming *how* the shell should invoke the
//! command:
//!
//! * [`DispatchTarget::InProcess`] — call the functor's `execute` op
//!   in the shell process. Carries a `functor_id` (FNV-1a-64 of the
//!   command name) the dispatcher can key against its in-process
//!   functor table when the R222.M5 substrate port replaces the
//!   `HashMap<String, CommandFunctor>` with a
//!   `HashMap<Str, ClosureFatPtr>`. Today the id is informational —
//!   the host-side registry still keys on `&str` — but landing the id
//!   now means the caller of `decide_dispatch` can already write the
//!   R222.M5-shaped lookup, and the shape does not churn under the
//!   port.
//!
//! * [`DispatchTarget::Spawn`] — the supervisor spawns the command's
//!   substrate binary. Carries `binary_path` (a `/bin/<name>` stub
//!   for R222.M6 — real path lookup against a supervisor manifest is
//!   the R222.M6-followup milestone) and `argv[0]` (conventionally
//!   the command name).
//!
//! # Why the two shapes rather than a single struct + tag
//!
//! An in-process dispatch has no `binary_path` or `argv` — the shell
//! already holds the parsed `Invocation` and hands it to the
//! functor. A spawn dispatch has no `functor_id` — the child process
//! resolves its own name via its own registry, not the host's.
//! Modelling them as one struct with `Option<binary_path>` +
//! `Option<functor_id>` invites a call site to reach for the field
//! that isn't set. The two-variant enum makes the "which fields are
//! populated" question a `match` the compiler enforces.
//!
//! # Why fnv1a_64 rather than the substrate `Str::hash`
//!
//! The R222.M5 substrate stores functor closures in a
//! `HashMap<Str, ClosureFatPtr>` keyed by NFC-normalised strings
//! whose hash is the R220.M4 `Str::hash` intrinsic — BLAKE3 today,
//! with the same fingerprint semantics
//! `design/terminal/schema-registry.md` §3 documents. Host-side we
//! do not yet have BLAKE3, so `crate::fingerprint::fnv1a_64` stands
//! in — the *shape* the caller sees (a `u64` derived from the name
//! bytes) is identical, and swapping the algorithm under the R222.M5
//! landing changes the constants without changing any call site.

use crate::fingerprint::fnv1a_64;
use crate::sig::{CommandSig, CommandWeight};

/// How the shell invokes a resolved command.
///
/// Two disjoint variants — see the module docs for why this is not
/// one struct with option fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DispatchTarget {
    /// In-process functor invocation.
    ///
    /// `functor_id` is FNV-1a-64 of the command's name. The
    /// dispatcher keys against it when R222.M5 lands the substrate
    /// `HashMap<Str, ClosureFatPtr>`; today the host-side registry
    /// still uses `&str` keys, but the caller of `decide_dispatch`
    /// can already write the substrate-shape lookup against this
    /// field without a code change under the port.
    InProcess {
        /// FNV-1a-64 of the command name bytes.
        functor_id: u64,
    },
    /// Spawn a substrate process for this command.
    ///
    /// R222.M6 lands a stub `binary_path` of `/bin/<name>`; the real
    /// resolver — consulting the supervisor's process manifest per
    /// `design/terminal/supervisor.md` §3 — is the R222.M6-followup.
    /// `argv[0]` is set to the command name so the child sees the
    /// conventional `argv[0]` = program-name shape (POSIX has
    /// carried this convention forward and no substrate we target
    /// deviates).
    Spawn {
        /// Filesystem path to the binary the supervisor spawns.
        binary_path: String,
        /// Argument vector the supervisor hands to the child;
        /// `argv[0]` is the command name.
        argv: Vec<String>,
    },
}

/// Decide how to dispatch a resolved [`CommandSig`].
///
/// The full body is a two-arm match on `sig.weight`:
///
/// * [`CommandWeight::Light`] → [`DispatchTarget::InProcess`] with
///   `functor_id = fnv1a_64(sig.name.as_bytes())`.
/// * [`CommandWeight::Heavy`] → [`DispatchTarget::Spawn`] with
///   `binary_path = /bin/<name>` and `argv = [name]`.
///
/// Pure and total — no I/O, no allocation beyond the `String`s the
/// `Spawn` variant carries. The caller (a REPL or a supervisor RPC
/// stub) is free to invoke this in a tight loop against many
/// candidate sigs.
pub fn decide_dispatch(sig: &CommandSig) -> DispatchTarget {
    match sig.weight {
        CommandWeight::Light => DispatchTarget::InProcess {
            functor_id: fnv1a_64(sig.name.as_bytes()),
        },
        CommandWeight::Heavy => DispatchTarget::Spawn {
            binary_path: format!("/bin/{}", sig.name),
            argv: vec![sig.name.clone()],
        },
    }
}
