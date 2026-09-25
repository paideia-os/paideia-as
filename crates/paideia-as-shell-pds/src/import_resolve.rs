//! R227.M3 `.pds` import resolution + module dependency graph.
//!
//! A `.pds` script's header may carry any number of
//! `#import "path" as name` pragmas (parsed by
//! [`crate::header::parse_header`] into `PdsHeader::imports`). Before
//! the runtime can hand the script's body to the shell-lex layer, each
//! quoted `path` must be resolved to a real filesystem entry — first
//! against the invoking project's root, then against an ordered list of
//! system module search paths. This module owns that resolution, plus
//! the DFS-driven module graph the loader walks to detect circular
//! imports before evaluation begins.
//!
//! # Position in the pipeline
//!
//! ```text
//!   .pds source
//!       │
//!       ▼
//!   parse_header  ── PdsHeader { imports: Vec<Import>, .. }
//!       │
//!       ▼
//!   resolve_imports(&header, &ctx)                     ← this module
//!       │
//!       ├── Ok(Vec<ResolvedImport>)                    → module graph
//!       └── Err(NotFound { path, tried })              → refuse to load; report
//!       │
//!       ▼
//!   ModuleGraph::build_from_root(root, &ctx, parser)   ← this module
//!       │
//!       ├── Ok(ModuleGraph { edges, resolved })        → schema-check, capabilities
//!       └── Err(Cycle { path } | ParseError | …)       → refuse to load; report
//! ```
//!
//! # Resolution semantics
//!
//! Given an [`ImportContext`] with `project_root = P` and
//! `system_paths = [S1, S2, …]`, resolution of an [`Import`] with
//! quoted path `q` walks the search list `[P, S1, S2, …]` in order.
//! The **first** entry whose `join(q)` exists on disk wins; no attempt
//! is made to detect ambiguity (two different roots offering the same
//! logical import). Project-local sources therefore shadow system
//! sources of the same relative path — the two-phase order is
//! deliberate: a script vendored inside a project can override the
//! shipped standard library without renaming.
//!
//! If none of the joined paths exists, resolution fails with
//! [`ImportError::NotFound`], carrying the full `tried` list so a
//! diagnostic can point at every location that was checked. This
//! echoes the R227.M2 checker's decision to accumulate misses rather
//! than short-circuit on the first: a user staring at a "module not
//! found" error should not have to bisect their search-path list one
//! entry at a time.
//!
//! # Module graph
//!
//! [`ModuleGraph::build_from_root`] runs a depth-first walk from a
//! `root` `.pds` file. At each node it invokes a caller-supplied
//! parser callback to obtain the node's [`crate::header::PdsHeader`]
//! (kept as a callback rather than a hard dependency on
//! [`crate::header::parse_header`] so tests can drive the graph with
//! mock parsers, and so a future loader that parses `.pds` files off a
//! different byte source than the filesystem — an in-memory cache, a
//! packed archive — can substitute without a second graph
//! implementation). The walk maintains a `visited` set (a canonical
//! path never gets re-parsed) and an active `stack` (the current DFS
//! recursion path) so a back-edge onto the stack is caught as
//! [`ImportError::Cycle`], with `path` echoing the offending stack for
//! diagnostic purposes.
//!
//! Edges are recorded as `(importer_path_string, imported_alias)`
//! pairs — the alias is the identifier the script uses at
//! call-sites, so tools that later render "who imports what?" as a
//! human-facing graph don't have to re-derive it from the resolved
//! `PathBuf`.
//!
//! # Fingerprints
//!
//! The R227.M3 test corpus tags each fixture with `r227m3-imp-NN` so
//! the R220.M10 `@fingerprint` correlator can attribute pass/fail to a
//! specific fixture without re-parsing its name.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};

use crate::header::{Import, PdsHeader};

/// A resolved [`Import`]: the on-disk path the loader will read, plus
/// the alias the importing script uses to refer to the module.
///
/// Kept separate from [`Import`] (which is the *lexical* form parsed
/// out of the header — the quoted `path` string, verbatim) so that a
/// consumer can hand a `Vec<ResolvedImport>` on to the body loader
/// without also having to carry a search-context around: the
/// `PathBuf` is the only thing the loader needs by the time it opens
/// the file.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ResolvedImport {
    /// The absolute-ish (`ctx.project_root` or `ctx.system_paths[i]`
    /// joined with the quoted path) path the loader will read. Not
    /// canonicalised — R227.M3 does not touch symlinks; the arbiter
    /// that owns the search list is responsible for feeding in
    /// already-canonical roots when that matters.
    pub path: PathBuf,
    /// The identifier the importing script uses to refer to the
    /// module. Copied verbatim from [`Import::alias`].
    pub alias: String,
}

/// The search context for [`resolve_imports`] and
/// [`ModuleGraph::build_from_root`].
///
/// Holds one **project root** (the directory the invoking script
/// lives in — checked first) plus an ordered list of **system
/// paths** (the ambient search list — walked after the project
/// root). The two are kept structurally distinct so that a future
/// arbiter that logs "resolved via project vs. system" has the split
/// available without re-deriving it from list position.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImportContext {
    /// The project-local search root — checked first for every
    /// import. Shadows any system-path entry with the same relative
    /// name.
    pub project_root: PathBuf,
    /// Ambient system search paths, walked in order after
    /// `project_root`. Order matters: the first entry whose
    /// `join(path)` exists wins.
    pub system_paths: Vec<PathBuf>,
}

impl ImportContext {
    /// Construct a context with `project_root` and an empty system
    /// search list. Chain [`Self::with_system_paths`] to attach an
    /// ambient search list.
    #[must_use]
    pub fn new(project_root: PathBuf) -> Self {
        Self {
            project_root,
            system_paths: Vec::new(),
        }
    }

    /// Attach an ordered list of ambient system search paths. Walked
    /// after `project_root` when resolving each import. Consumes and
    /// returns `self` so the builder call reads as
    /// `ImportContext::new(root).with_system_paths(vec![…])`.
    #[must_use]
    pub fn with_system_paths(mut self, paths: Vec<PathBuf>) -> Self {
        self.system_paths = paths;
        self
    }
}

/// Discriminated failure modes for [`resolve_imports`] and
/// [`ModuleGraph::build_from_root`].
///
/// The enum shape lets both surfaces share a single error channel:
/// the graph builder can uniformly propagate a `NotFound` from the
/// resolver, a `Cycle` from its own DFS, or a `ParseError` from the
/// caller-supplied parser callback, without a per-layer `From` shim.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImportError {
    /// None of the search roots contained the requested import. `path`
    /// echoes the quoted string as written in the `#import` pragma;
    /// `tried` lists every root+path join that was tested, in the
    /// order they were tested (project root first, then each system
    /// path in list order), so a diagnostic can point at every miss
    /// rather than at only the first.
    NotFound {
        /// The quoted path as written in the `#import` pragma.
        path: String,
        /// Every filesystem location that was tested and did not
        /// exist, in the order tested.
        tried: Vec<PathBuf>,
    },
    /// A back-edge onto the active DFS stack: importing this module
    /// would close a cycle. `path` echoes the offending stack from
    /// the earliest node up to (and including) the repeat, rendered
    /// as strings so the diagnostic doesn't lose the character set to
    /// a lossy `Path`→`str` conversion at report time.
    Cycle {
        /// The DFS stack at the point the back-edge was seen, from
        /// the earliest active node up to (and including) the node
        /// whose second entry closed the cycle.
        path: Vec<String>,
    },
    /// The parser callback handed to [`ModuleGraph::build_from_root`]
    /// returned an error for one of the graph's nodes. `path` names
    /// the file the parser was invoked on; `reason` is the parser's
    /// own message (copied verbatim so a downstream reader can pattern
    /// on it if the parser is a known one).
    ParseError {
        /// The file the parser was invoked on (as a lossy string —
        /// diagnostics only, never re-fed to the filesystem).
        path: String,
        /// The parser's own diagnostic message.
        reason: String,
    },
    /// A generic I/O failure surfaced by a resolver-side operation
    /// other than "not found" (permission denied, invalid path, …).
    /// Currently unused by the M3 surface — reserved so a future
    /// milestone that actually opens files during resolution can
    /// extend without a breaking signature change.
    IoError {
        /// The path whose I/O failed.
        path: String,
    },
}

impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound { path, tried } => {
                write!(
                    f,
                    "import `{path}` not found; tried {} location(s):",
                    tried.len()
                )?;
                for (i, p) in tried.iter().enumerate() {
                    write!(f, "{}{}", if i == 0 { " " } else { ", " }, p.display())?;
                }
                Ok(())
            }
            Self::Cycle { path } => {
                write!(f, "import cycle detected: ")?;
                for (i, node) in path.iter().enumerate() {
                    if i > 0 {
                        write!(f, " -> ")?;
                    }
                    write!(f, "{node}")?;
                }
                Ok(())
            }
            Self::ParseError { path, reason } => {
                write!(f, "parse error in `{path}`: {reason}")
            }
            Self::IoError { path } => {
                write!(f, "I/O error accessing `{path}`")
            }
        }
    }
}

impl std::error::Error for ImportError {}

/// Resolve every `#import` in `header` against the search roots in
/// `ctx`.
///
/// For each [`Import`] entry in header-declaration order, walks the
/// search list `[ctx.project_root, ctx.system_paths[0], …]` and picks
/// the first entry whose `join(entry.path)` reports
/// [`std::path::Path::exists`]. Returns a [`ResolvedImport`] per
/// input, in the same order.
///
/// The scan does **not** short-circuit on the first miss: an
/// [`ImportError::NotFound`] is raised the moment any single import
/// fails to resolve, but the successfully-resolved prefix is
/// discarded (the resolver's contract is all-or-nothing — a
/// half-resolved script is not something the loader can meaningfully
/// hand on).
///
/// # Errors
///
/// Returns [`ImportError::NotFound`] the first time an import fails
/// to resolve against any search root. `tried` lists every candidate
/// path that was tested for that import, in the order tested.
pub fn resolve_imports(
    header: &PdsHeader,
    ctx: &ImportContext,
) -> Result<Vec<ResolvedImport>, ImportError> {
    let mut resolved = Vec::with_capacity(header.imports.len());
    for entry in &header.imports {
        resolved.push(resolve_one(entry, ctx)?);
    }
    Ok(resolved)
}

/// Resolve a single [`Import`] against the search list in `ctx`.
///
/// Factored out so [`ModuleGraph::build_from_root`] can call it
/// per-edge without materialising a synthetic [`PdsHeader`] just to
/// call [`resolve_imports`].
fn resolve_one(entry: &Import, ctx: &ImportContext) -> Result<ResolvedImport, ImportError> {
    let mut tried: Vec<PathBuf> = Vec::with_capacity(1 + ctx.system_paths.len());

    let project_candidate = ctx.project_root.join(&entry.path);
    if project_candidate.exists() {
        return Ok(ResolvedImport {
            path: project_candidate,
            alias: entry.alias.clone(),
        });
    }
    tried.push(project_candidate);

    for sys in &ctx.system_paths {
        let candidate = sys.join(&entry.path);
        if candidate.exists() {
            return Ok(ResolvedImport {
                path: candidate,
                alias: entry.alias.clone(),
            });
        }
        tried.push(candidate);
    }

    Err(ImportError::NotFound {
        path: entry.path.clone(),
        tried,
    })
}

/// The DFS-materialised module dependency graph rooted at some
/// entry-point `.pds` file.
///
/// `edges` records every `(importer_path_string, imported_alias)`
/// pair encountered, in the order the DFS produced them. `resolved`
/// maps each importer's lossy path string to the [`PathBuf`] the
/// resolver settled on — a small convenience for tools that render
/// the graph and want the on-disk location without re-running
/// [`resolve_imports`].
///
/// The graph is a *materialised* record of one walk, not a live
/// index: mutating the filesystem after building it does not
/// invalidate the recorded edges. Rebuild the graph if the loader
/// needs to reflect a filesystem change.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModuleGraph {
    /// Every `(importer_path_string, imported_alias)` pair the DFS
    /// visited, in DFS-emit order.
    pub edges: Vec<(String, String)>,
    /// Map from importer path (lossy string) to the resolved
    /// [`PathBuf`] of that importer.
    pub resolved: HashMap<String, PathBuf>,
}

impl ModuleGraph {
    /// Build a module dependency graph rooted at `root`.
    ///
    /// Runs a depth-first walk from `root`. At each unvisited node,
    /// the caller-supplied `parser` callback is invoked with the
    /// node's path and expected to return the node's parsed
    /// [`PdsHeader`]. Each `#import` in that header is resolved via
    /// [`resolve_imports`] against `ctx`, an edge
    /// `(current_path, alias)` is recorded, and the DFS recurses
    /// into each resolved [`PathBuf`].
    ///
    /// Cycle detection uses an active-stack membership check: the
    /// second time a path appears on the recursion stack, the walk
    /// halts with [`ImportError::Cycle`] whose `path` echoes the
    /// stack from the earliest active node up to (and including) the
    /// repeat.
    ///
    /// # Errors
    ///
    /// * [`ImportError::NotFound`] when `resolve_imports` can't
    ///   place one of the header's imports.
    /// * [`ImportError::Cycle`] when the DFS closes a back-edge onto
    ///   the active recursion stack.
    /// * [`ImportError::ParseError`] when `parser` returns `Err` for
    ///   any node.
    pub fn build_from_root<F>(
        root: &Path,
        ctx: &ImportContext,
        parser: F,
    ) -> Result<Self, ImportError>
    where
        F: Fn(&Path) -> Result<PdsHeader, ImportError>,
    {
        let mut graph = ModuleGraph::default();
        let mut visited: HashSet<PathBuf> = HashSet::new();
        let mut stack: Vec<PathBuf> = Vec::new();
        dfs(root, ctx, &parser, &mut graph, &mut visited, &mut stack)?;
        Ok(graph)
    }
}

/// Recursive DFS worker for [`ModuleGraph::build_from_root`].
///
/// Held out-of-line so the public entry point stays declarative and
/// the recursion's state (visited set, active stack) has one
/// definitive owner. `parser` is generic over `F: Fn(&Path) -> …`
/// rather than `dyn Fn(&Path) -> …` so a monomorphised callback path
/// (the common case: a real filesystem-backed parser) has no dynamic
/// dispatch overhead per node.
fn dfs<F>(
    current: &Path,
    ctx: &ImportContext,
    parser: &F,
    graph: &mut ModuleGraph,
    visited: &mut HashSet<PathBuf>,
    stack: &mut Vec<PathBuf>,
) -> Result<(), ImportError>
where
    F: Fn(&Path) -> Result<PdsHeader, ImportError>,
{
    let current_owned = current.to_path_buf();

    // Back-edge check: if `current` is already on the active DFS
    // stack, we've closed a cycle. Emit the stack + the repeat as
    // strings so the diagnostic doesn't lose the character set to a
    // lossy `Path`→`str` conversion at report time.
    if stack.iter().any(|p| p == &current_owned) {
        let mut path: Vec<String> =
            stack.iter().map(|p| p.display().to_string()).collect();
        path.push(current_owned.display().to_string());
        return Err(ImportError::Cycle { path });
    }

    // Already-visited (finished, off-stack) nodes are DAG shortcuts —
    // a module can be imported by two different importers without
    // being re-parsed. The edge from `current`'s parent to `current`
    // is recorded by the caller before recursion; we just skip the
    // re-walk.
    if visited.contains(&current_owned) {
        return Ok(());
    }

    stack.push(current_owned.clone());

    // Parse the current node. Any error the callback returns —
    // typically `ParseError`, but a callback that fetches from a
    // more elaborate byte source is free to raise the other variants
    // — is surfaced verbatim; the DFS's job is to propagate, not to
    // reinterpret.
    let header = match parser(current) {
        Ok(h) => h,
        Err(e) => {
            stack.pop();
            return Err(e);
        }
    };

    // Record the resolved path of `current` in the map first so a
    // tool rendering the graph can look up "who is this importer?"
    // even if the sub-walk fails partway.
    let current_key = current_owned.display().to_string();
    graph
        .resolved
        .insert(current_key.clone(), current_owned.clone());

    // For each import: resolve it (may raise NotFound), record the
    // edge (importer_string → alias), and recurse into the resolved
    // path.
    for import in &header.imports {
        let resolved = resolve_one(import, ctx)?;
        graph
            .edges
            .push((current_key.clone(), resolved.alias.clone()));
        dfs(&resolved.path, ctx, parser, graph, visited, stack)?;
    }

    stack.pop();
    visited.insert(current_owned);
    Ok(())
}
