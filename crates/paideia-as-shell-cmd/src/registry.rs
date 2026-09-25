//! R222.M3 supporting piece — the command registry.
//!
//! The registry is the `HashMap<Str, ClosureFatPtr>` from R220.M7,
//! lowered to `HashMap<String, CommandFunctor>` on the host side. When
//! R222.M5 lands the on-disk manifest loader (per
//! `design/terminal/command-registry.md`), the same registry seeds
//! from `/system/shell/commands.toml` and the per-user override at
//! `/users/<u>/shell/commands.toml`; the in-code seeding here becomes
//! the fallback the shell uses when the manifest is not yet mounted
//! (host-side bring-up per the materialization plan).
//!
//! # Why `String` keys rather than `&'static str`
//!
//! R222.M5 loads commands whose names arrive as owned bytes from the
//! TOML parser; going to `String` now (rather than `&'static str`)
//! avoids a keys-lifetime split when the loader lands. Interim
//! `String` allocation is one per registration — negligible at
//! shell-startup latency.
//!
//! # Why FNV-1a-64 via `std::collections::HashMap`'s default RandomState
//!
//! The R220.M6/M7 substrate uses FNV-1a-64 (schema-registry.md §3) —
//! the on-disk substrate compatibility is a M5 concern (the manifest
//! loader hashes with FNV to compute the same fingerprint the daemon
//! sees). For the R222.M3 in-process registry, `std::HashMap`'s
//! default `RandomState` (SipHash) is cryptographically stronger and
//! removes any FNV-tuned adversarial input risk while the registry
//! lives host-side. When R222.M5 wires the substrate path, that path
//! uses FNV to match the wire; this in-process fallback keeps SipHash
//! because the two never share a table.

use std::collections::HashMap;

use crate::commands::{count, find, head, sort, where_, CommandFunctor};
use crate::schema::SchemasSig;
use crate::sig::CommandSig;
use crate::wire;

/// Str→functor lookup. `resolve` returns the functor; the caller
/// then invokes it against the session's [`crate::SchemasSig`] to
/// obtain the concrete [`crate::CommandSig`].
///
/// # Two population paths, one lookup surface
///
/// The registry is populated from either of two sources:
///
/// * **In-code functors** — [`Self::register`] plants a
///   [`CommandFunctor`] fn-ptr under a shell name. This is the
///   pre-manifest bring-up path
///   ([`Self::with_light_commands`]): the functor takes the session's
///   [`SchemasSig`] and returns a fully-executable [`CommandSig`].
///
/// * **Loaded sigs** — [`Self::register_sig`] plants a [`CommandSig`]
///   parsed from `/system/shell/commands.toml` (per
///   [`crate::registry_client`]). These sigs carry
///   [`crate::wire::placeholder_execute`] on the `execute` field
///   because the on-disk manifest describes commands, it does not
///   ship their bodies — the substrate-side supervisor spawns the
///   real body via the R222.M6 heavy-dispatch path once the RPC lands.
///
/// The two tables are kept side-by-side (rather than unified through a
/// synthesised functor that closes over the loaded sig) because a
/// [`CommandFunctor`] is a bare `fn` pointer with no capture ability:
/// there is no runtime-safe way to synthesise one that returns a
/// heap-loaded value. Callers pick the right lookup for what they
/// need — [`Self::resolve`] for a functor to invoke locally,
/// [`Self::resolve_sig`] for a signature to describe or dispatch
/// through the supervisor.
#[derive(Default)]
pub struct CommandRegistry {
    functors: HashMap<String, CommandFunctor>,
    sigs: HashMap<String, CommandSig>,
}

impl CommandRegistry {
    /// Empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register (or overwrite) a functor under a shell name.
    ///
    /// Overwrite semantics: R222.M5's per-user override loads AFTER
    /// the system manifest, so a duplicate name silently replaces the
    /// system entry with the user entry. Matching that here (rather
    /// than refusing) keeps the R222.M5 loader's `.insert` shape
    /// pointer-compatible.
    pub fn register(&mut self, name: impl Into<String>, functor: CommandFunctor) {
        self.functors.insert(name.into(), functor);
    }

    /// Look up a functor by shell name.
    ///
    /// R222.M3 uses raw `&str` byte-equality on the map lookup. R222.M5
    /// moves to R220.M4 `Str::eq` (NFC-normalised at the boundary
    /// per SH-D9); no shape change here — the map key type stays
    /// `String`, only the eq/hash callbacks on the map differ.
    pub fn resolve(&self, name: &str) -> Option<CommandFunctor> {
        self.functors.get(name).copied()
    }

    /// Number of registered functors.
    pub fn len(&self) -> usize {
        self.functors.len()
    }

    /// Whether any functor is registered.
    pub fn is_empty(&self) -> bool {
        self.functors.is_empty()
    }

    /// Iterator over registered names (unordered).
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.functors.keys().map(String::as_str)
    }

    /// R222.M8 — return the `CommandSig` wire-format bytes for the
    /// functor registered under `name`, or `None` if no such command
    /// exists.
    ///
    /// The functor is instantiated against the R220 seed
    /// [`SchemasSig`] — the two-schema starter session the R222.M3
    /// canary uses — so `describe find` at bring-up time returns a
    /// signature bound against `FileSchema@0.1` and `RawByteChunk@0.1`
    /// rather than an unbound placeholder. When R222.M5 wires the
    /// session-real `SchemasSig` off the on-disk manifest, this
    /// method grows a `describe_with(&self, name, &SchemasSig)`
    /// variant; keeping the shorthand here means the REPL bring-up
    /// path (`describe <cmd>` on a fresh session) needs no extra
    /// plumbing.
    ///
    /// The returned bytes are `wire::to_wire(functor(&SchemasSig::r220_seed()))`
    /// — see [`crate::wire`] for the layout and the deliberate
    /// exclusion of the `execute` fn-ptr.
    pub fn describe(&self, name: &str) -> Option<Vec<u8>> {
        self.functors
            .get(name)
            .map(|functor| wire::to_wire(&functor(&SchemasSig::r220_seed())))
    }

    /// R222.M5 — register (or overwrite) a fully-formed [`CommandSig`]
    /// loaded from an on-disk manifest under its `name` field.
    ///
    /// This is the sig-side counterpart of [`Self::register`]: the
    /// loader in [`crate::registry_client`] parses each TOML entry into
    /// a `CommandSig` with [`crate::wire::placeholder_execute`] as the
    /// `execute` fn-ptr and hands the batch to [`crate::registry_client::seed_registry`],
    /// which in turn calls this method once per entry.
    ///
    /// Overwrite semantics match [`Self::register`]: the per-user
    /// manifest is loaded AFTER the system manifest, so a duplicate name
    /// silently shadows.
    pub fn register_sig(&mut self, sig: CommandSig) {
        self.sigs.insert(sig.name.clone(), sig);
    }

    /// R222.M5 — look up a loaded [`CommandSig`] by shell name.
    ///
    /// The counterpart to [`Self::resolve`]: this returns the descriptive
    /// half of the functor (name, schemas, arg/flag specs, effects, caps,
    /// weight, plus the placeholder `execute`) — the shape the R222.M6
    /// heavy-dispatch path hands to the supervisor when the command's
    /// body lives in a spawned process.
    pub fn resolve_sig(&self, name: &str) -> Option<&CommandSig> {
        self.sigs.get(name)
    }

    /// R222.M5 — number of loaded sigs in the registry.
    ///
    /// Deliberately separate from [`Self::len`] (which counts in-code
    /// functors): the two populations are independent (see the struct
    /// docs), and a single unified count would mislead a caller into
    /// thinking a functor-only registry can dispatch a loaded sig or
    /// vice-versa.
    pub fn sig_len(&self) -> usize {
        self.sigs.len()
    }

    /// Seed the registry with the R222.M3 five reference light
    /// commands: `find`, `where`, `sort`, `head`, `count`.
    ///
    /// This is the pre-manifest fallback the shell uses during
    /// bring-up. The R222.M5 loader replaces this with a real load
    /// from `/system/shell/commands.toml`; the shape of the returned
    /// registry is identical.
    pub fn with_light_commands() -> Self {
        let mut r = Self::new();
        r.register("find", find::functor);
        r.register("where", where_::functor);
        r.register("sort", sort::functor);
        r.register("head", head::functor);
        r.register("count", count::functor);
        r
    }
}
