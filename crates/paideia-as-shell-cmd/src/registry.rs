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

/// Str→functor lookup. `resolve` returns the functor; the caller
/// then invokes it against the session's [`crate::SchemasSig`] to
/// obtain the concrete [`crate::CommandSig`].
#[derive(Default)]
pub struct CommandRegistry {
    functors: HashMap<String, CommandFunctor>,
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
