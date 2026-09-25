//! R228.M6 -- filesystem-path completion catalogue.
//!
//! [`PathProvider`] is the shape the R229 REPL hands the completion
//! engine on start-up (and refreshes on chdir): a flat map from
//! **directory path** to the list of names that directory contains.
//! The engine consults it whenever the token neighbourhood shapes as
//! a path-in-progress -- see [`crate::complete`]'s `try_path_completion`
//! branch for the trigger conditions and the M6 CHANGELOG for the
//! exact detection algorithm.
//!
//! # Design decisions
//!
//! * **Pre-materialized entries, not a live walker.** The M6 shape
//!   stores `HashMap<String, Vec<String>>` rather than borrowing a
//!   `std::fs::read_dir` at consult time so this crate stays no_std-
//!   ready and unit-testable without a temp-directory scaffold. The
//!   REPL owns the caching / invalidation policy at the R229 wire-up
//!   (a chdir or an mtime bump on the directory triggers a re-seed);
//!   pushing that policy into this crate would couple the pure
//!   completion function to a syscall surface the LSP consumer does
//!   not want.
//!
//! * **Directory keys stored bare (`"/usr"`, not `"/usr/"`).** The
//!   normalization convention: the root is `"/"`, every other
//!   directory drops its trailing slash. The trigger's split routine
//!   normalizes the caller's cursor path the same way before doing
//!   the lookup, so seed shape and query shape agree. Storing bare
//!   keys is also what a `std::fs::canonicalize` result yields, so a
//!   future live-walker migration seeds directly.
//!
//! * **Values are child *names*, not full paths.** The completion
//!   candidate text is the name the user types after the trailing
//!   `/`; joining back to a full path is the REPL's job at accept
//!   time. Keeping names bare matches [`crate::CommandFlags`]'s bare-
//!   name convention -- the emitter reintroduces the dash(es) or the
//!   path separator, not the catalogue.
//!
//! * **Missing dir = zero candidates, not an error.** A path prefix
//!   whose directory is absent from the provider yields an empty
//!   result rather than an `Err`. A REPL user typing `/nonexistent/`
//!   should see the popup close, not a diagnostic bar -- and the
//!   completion function's `Request -> Response` contract has no
//!   error channel by design (per the crate-level rationale about
//!   graceful degradation on adversarial input).

use std::collections::HashMap;

/// Directory -> child-name catalogue consulted by
/// [`crate::complete`]'s path-completion branch.
///
/// Directory keys follow the convention documented at the module
/// header: the root is stored as `"/"`, every other directory drops
/// its trailing slash (`"/usr"`, `"/usr/local"`). Values are the
/// *bare* names inside that directory, not full paths -- the emitter
/// keeps them bare and the caller joins them onto the leading dir at
/// accept time.
#[derive(Clone, Debug, Default)]
pub struct PathProvider {
    /// Directory path -> child names. See the module-header
    /// normalization convention for the key shape; missing entries
    /// yield zero candidates rather than a lookup error.
    pub entries: HashMap<String, Vec<String>>,
}

impl PathProvider {
    /// Construct an empty provider. Convenience alias for
    /// [`Default::default`] so the R229 REPL boot path reads
    /// `PathProvider::new().insert(...)` without needing the
    /// `Default` trait in scope.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert one directory's children. Overwrites any prior list at
    /// the same key -- the M6 caller re-seeds the whole entry on
    /// chdir rather than delta-appending, matching the wholesale-
    /// replacement discipline of [`crate::CompletionEngine::with_records`]
    /// and [`crate::CompletionEngine::with_command_types`].
    ///
    /// The `dir` argument accepts any `Into<String>` so seed sites can
    /// pass `&str` literals or owned `String`s without an explicit
    /// conversion at every call site.
    pub fn insert(&mut self, dir: impl Into<String>, children: Vec<String>) {
        self.entries.insert(dir.into(), children);
    }

    /// List the bare child names of `dir`. Returns an empty vector
    /// when `dir` is not seeded -- the caller then emits zero path
    /// candidates.  Cloned rather than borrowed so the caller may
    /// mutate the provider between consult sites without lifetime
    /// churn; the M6 corpus dirs are small (tens of entries), so the
    /// clone cost is negligible next to the score-match pass that
    /// follows.
    pub fn list(&self, dir: &str) -> Vec<String> {
        self.entries.get(dir).cloned().unwrap_or_default()
    }
}
