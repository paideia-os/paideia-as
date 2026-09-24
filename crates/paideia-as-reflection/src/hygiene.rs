//! Hygienic identifier substrate for the R220 reflection surface.
//!
//! `HygienicId` is the lightweight newtype the `Syntax` constructors
//! attach to every identifier they introduce.  Two identifiers with the
//! same spelling compare equal in name-resolution **only** when their
//! `HygienicId`s compare equal, which is what makes the Ullrich 2020
//! alpha-rename pass sound.
//!
//! # R220 layered evolution
//!
//! * **R220.M1** landed the [`HygienicId`] newtype (a bare `NonZeroU32`)
//!   plus the [`HYGIENIC_ID_UNTAGGED`] sentinel and [`fresh_hygienic_id`]
//!   minter.  Every constructor accepted a `HygienicId` argument but no
//!   pass actually attached anything but the sentinel.
//! * **R220.M2** (this round) grows the substrate with the concept of a
//!   **macro-invocation scope** ([`MacroScopeId`]).  Each macro
//!   invocation mints one fresh `MacroScopeId`; every identifier
//!   introduced by that invocation gets a `HygienicId` derived from that
//!   scope via [`HygienicId::for_macro_scope`].  The [`hygienic_rename`]
//!   pass walks a `Syntax` tree and re-tags DSL-introduced identifiers
//!   with the invocation's scope, leaving use-site identifiers untouched
//!   — the alpha-rename step from Ullrich 2020 §3 wired through the
//!   R220.M1 reflection API.
//!
//! # Encoding of `HygienicId`
//!
//! The 32 bits of the underlying `NonZeroU32` are partitioned so a
//! consumer can classify any id in `O(1)`:
//!
//! | Range                          | Meaning                              |
//! |--------------------------------|--------------------------------------|
//! | `0x0000_0001`                  | [`HYGIENIC_ID_UNTAGGED`] sentinel    |
//! | `0x0000_0002 ..= 0x7FFF_FFFF`  | R220.M1 unscoped fresh id            |
//! | `0x8000_0000 ..= 0xFFFF_FFFF`  | R220.M2 macro-scope-derived id       |
//!
//! For macro-scope-derived ids, the low 31 bits (with the top bit
//! cleared) name the [`MacroScopeId`].  This preserves the R220.M1
//! signature `HygienicId(NonZeroU32)` and keeps the [`HYGIENIC_ID_UNTAGGED`]
//! sentinel intact, so downstream crates that only used the M1 surface
//! keep compiling unchanged.
//!
//! # Why keep this crate low in the dependency graph
//!
//! `paideia-as-elaborator` carries its own `MacroId` / `HygienicName` /
//! `HygieneCache` triple for the phase-1/phase-2 macro pipeline; the
//! reflection crate deliberately does **not** depend on the elaborator
//! (R220.M3's `@dsl_parser` needs to sit below both).  The elaborator
//! bridges its own `MacroId` to the reflection [`MacroScopeId`] via a
//! `From` conversion — see `paideia_as_elaborator::hygiene::MacroId`.

use core::num::NonZeroU32;
use core::sync::atomic::{AtomicU32, Ordering};

use paideia_as_ast::AstArena;

use crate::syntax::Syntax;
use crate::walker::{SyntaxWalker, WalkAction, walk_syntax};

/// Bit mask marking a `HygienicId` as macro-scope-derived.  Set on all
/// ids returned by [`HygienicId::for_macro_scope`]; clear on the R220.M1
/// unscoped-fresh ids and on [`HYGIENIC_ID_UNTAGGED`].
pub const MACRO_SCOPE_BIT: u32 = 0x8000_0000;

/// Largest [`MacroScopeId`] the packed encoding can carry (31 bits, since
/// the top bit of the underlying `NonZeroU32` is reserved for the
/// [`MACRO_SCOPE_BIT`] tag).
pub const MAX_MACRO_SCOPE_ID: u32 = 0x7FFF_FFFF;

/// Sentinel `HygienicId` for identifiers copied verbatim from the caller
/// AST (i.e., not introduced by the reflected DSL).
///
/// R220.M2 reads this as "unmarked at the DSL level; retain whatever
/// hygiene tags the use-site AST already carried."
pub const HYGIENIC_ID_UNTAGGED: HygienicId = HygienicId(NonZeroU32::MIN);

/// A hygienic identifier tag attached to a `Syntax`-introduced name.
///
/// See the module-level docs for the u32 encoding.  Two `HygienicId`s
/// compare equal iff their underlying tags are equal.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd, Debug)]
pub struct HygienicId(NonZeroU32);

impl HygienicId {
    /// The raw integer value of this tag.  Never zero.
    #[must_use]
    pub fn get(self) -> u32 {
        self.0.get()
    }

    /// Construct a `HygienicId` from a positive integer.  Returns `None`
    /// if the integer is zero.  Prefer [`fresh_hygienic_id`] for new
    /// unscoped ids and [`HygienicId::for_macro_scope`] for scope-tagged
    /// ids; use this only when round-tripping through a persisted form.
    #[must_use]
    pub fn from_raw(n: u32) -> Option<Self> {
        NonZeroU32::new(n).map(Self)
    }

    /// True if this ID is the [`HYGIENIC_ID_UNTAGGED`] sentinel — the
    /// "pass this identifier through unchanged" marker.
    #[must_use]
    pub fn is_untagged(self) -> bool {
        self == HYGIENIC_ID_UNTAGGED
    }

    /// Construct a `HygienicId` that identifies a specific macro
    /// invocation's scope.  Every identifier the reflection layer
    /// attaches to that macro invocation's DSL-introduced names shares
    /// this id, and it is guaranteed alpha-distinct from every
    /// [`HYGIENIC_ID_UNTAGGED`] identifier and from every id derived from
    /// a different [`MacroScopeId`].
    #[must_use]
    pub fn for_macro_scope(scope: MacroScopeId) -> Self {
        let raw = MACRO_SCOPE_BIT | scope.0.get();
        HygienicId(
            NonZeroU32::new(raw)
                .expect("MACRO_SCOPE_BIT is nonzero so packed id is always nonzero"),
        )
    }

    /// True if this `HygienicId` was minted via
    /// [`HygienicId::for_macro_scope`] (i.e., it names a macro-invocation
    /// scope).  False for [`HYGIENIC_ID_UNTAGGED`] and for the R220.M1
    /// unscoped-fresh ids returned by [`fresh_hygienic_id`].
    #[must_use]
    pub fn is_macro_scope(&self) -> bool {
        self.0.get() & MACRO_SCOPE_BIT != 0
    }

    /// The [`MacroScopeId`] this `HygienicId` names, if any.  Returns
    /// `None` for [`HYGIENIC_ID_UNTAGGED`] and for R220.M1 unscoped-fresh
    /// ids.
    #[must_use]
    pub fn macro_scope(&self) -> Option<MacroScopeId> {
        if self.is_macro_scope() {
            let low = self.0.get() & MAX_MACRO_SCOPE_ID;
            NonZeroU32::new(low).map(MacroScopeId)
        } else {
            None
        }
    }
}

impl core::fmt::Display for HygienicId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.is_untagged() {
            write!(f, "h#untagged")
        } else if let Some(scope) = self.macro_scope() {
            write!(f, "h#scope={}", scope.get())
        } else {
            write!(f, "h#{}", self.0.get())
        }
    }
}

/// Allocate a fresh, globally-unique **unscoped** `HygienicId`.
///
/// Each returned tag is monotonically increasing across the process and
/// lives in the reserved unscoped range `2 ..= 0x7FFF_FFFF` (top bit
/// clear, so [`HygienicId::is_macro_scope`] is `false`).  The counter
/// never wraps in practice (≥ 2^31 DSL-introduced identifiers is not a
/// realistic budget for one compilation).
///
/// Prefer [`fresh_macro_scope_id`] + [`HygienicId::for_macro_scope`] for
/// identifiers whose scope-membership matters for hygiene comparison
/// (i.e., R220.M2+ hosted DSLs).  [`fresh_hygienic_id`] remains the
/// right entry point for R220.M1-style one-shot fresh ids that don't
/// need to compare equal across a macro's body.
#[must_use]
pub fn fresh_hygienic_id() -> HygienicId {
    static NEXT: AtomicU32 = AtomicU32::new(2);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    assert!(
        n <= MAX_MACRO_SCOPE_ID,
        "fresh_hygienic_id counter exhausted the unscoped range \
         2..=0x7FFF_FFFF; process must be restarted",
    );
    HygienicId(
        NonZeroU32::new(n).expect("fresh_hygienic_id counter starts at 2 and only grows"),
    )
}

/// A macro-invocation scope identifier.
///
/// **Fresh at every macro invocation** — call [`fresh_macro_scope_id`]
/// once per hosted-DSL invocation and reuse the resulting id to tag
/// every identifier that the invocation introduces.  Two invocations
/// (even of the same macro) receive distinct ids, so their DSL-scoped
/// names never alias.
///
/// A `MacroScopeId` packs into a [`HygienicId`] via
/// [`HygienicId::for_macro_scope`]; the reverse extraction is
/// [`HygienicId::macro_scope`].
#[derive(Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd, Debug)]
pub struct MacroScopeId(NonZeroU32);

impl MacroScopeId {
    /// The raw integer value of this scope id.  Never zero and always
    /// in the range `1 ..= MAX_MACRO_SCOPE_ID` (31-bit space).
    #[must_use]
    pub fn get(self) -> u32 {
        self.0.get()
    }

    /// Construct a `MacroScopeId` from a positive integer in the valid
    /// range.  Returns `None` for zero or for values above
    /// [`MAX_MACRO_SCOPE_ID`].  Prefer [`fresh_macro_scope_id`] for
    /// live minting; use this only when round-tripping.
    #[must_use]
    pub fn from_raw(n: u32) -> Option<Self> {
        if n == 0 || n > MAX_MACRO_SCOPE_ID {
            None
        } else {
            NonZeroU32::new(n).map(Self)
        }
    }
}

impl core::fmt::Display for MacroScopeId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "scope#{}", self.0.get())
    }
}

/// Allocate a fresh, globally-unique [`MacroScopeId`].
///
/// The counter is process-monotonic and lives in the 31-bit space
/// `1 ..= MAX_MACRO_SCOPE_ID`.  ≥ 2^31 macro invocations is not a
/// realistic budget for one compilation; the panic on exhaustion is
/// there so a runaway loop is caught early rather than silently
/// aliasing scope 1.
#[must_use]
pub fn fresh_macro_scope_id() -> MacroScopeId {
    static NEXT: AtomicU32 = AtomicU32::new(1);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    assert!(
        n <= MAX_MACRO_SCOPE_ID,
        "fresh_macro_scope_id counter exhausted the 31-bit scope space; \
         process must be restarted",
    );
    MacroScopeId(
        NonZeroU32::new(n).expect("fresh_macro_scope_id counter starts at 1 and only grows"),
    )
}

/// Alpha-rename a `Syntax` tree by attaching `macro_scope`'s
/// [`HygienicId`] to every DSL-introduced identifier reachable from
/// `syntax`.
///
/// This is the Ullrich 2020 §3 alpha-rename pass, exposed through the
/// R220.M1 reflection API.  After the pass:
///
/// * Every identifier that the macro **introduced** carries a
///   `HygienicId` whose scope is `macro_scope`.
/// * Identifiers **passed in from the use site** (via antiquotes or
///   macro arguments) retain their use-site [`HYGIENIC_ID_UNTAGGED`]
///   tag, so the use-site's own name-resolution context still applies.
///
/// The result is a `Syntax` value whose top-level hygiene tag is the
/// macro scope; child traversal via [`Syntax::children`] flows the same
/// tag down the subtree.  Antiquote sub-trees are visible under
/// [`Syntax::head_kind`] as [`crate::syntax::SyntaxKind::Other`] with an
/// underlying [`paideia_as_ast::reflect::TermHead::Antiquote`]; the
/// consumer resets those to [`HYGIENIC_ID_UNTAGGED`] via
/// [`Syntax::from_node_with_hygiene`].  A per-node map suitable for
/// name-resolution is produced by [`hygienic_rename_map`] — that is the
/// entry point the elaborator's `HygieneCache` bridge consumes.
///
/// The R220.M2 milestone requires the pass to be structurally correct
/// (macro-introduced names remain alpha-distinct from use-site names of
/// the same spelling); the acceptance test corpus lives in
/// `tests/hygiene_capture_corpus.rs` (20 hand-authored capture forms)
/// and `tests/hygiene_property.rs` (10 000 random forms).
#[must_use]
pub fn hygienic_rename<'a>(syntax: &Syntax<'a>, macro_scope: MacroScopeId) -> Syntax<'a> {
    // Structural rename: attach the scope tag at the root; child
    // traversal propagates it via `Syntax::children`.  Antiquote
    // handling is a per-node concern that consumers dispatch on
    // `head_kind()` — see the doc comment and `hygienic_rename_map`.
    let scope_tag = HygienicId::for_macro_scope(macro_scope);
    Syntax::from_node_with_hygiene(syntax.arena(), syntax.node_id(), scope_tag)
}

/// Per-node hygiene map produced by [`hygienic_rename_map`].
///
/// Maps each `NodeId` reachable from the renamed root to the
/// `HygienicId` that name resolution should see for that node:
///
/// * Nodes introduced by the macro → `HygienicId::for_macro_scope(scope)`.
/// * Nodes descending from an antiquote → [`HYGIENIC_ID_UNTAGGED`].
///
/// The map is the granular counterpart to [`hygienic_rename`]'s
/// coarse root-tag result.  Consumers that need per-identifier
/// hygiene (i.e., the elaborator's name resolver) use this;
/// consumers that only need to know the macro's scope tag use the
/// simpler [`hygienic_rename`].
#[derive(Clone, Debug, Default)]
pub struct HygienicRenameMap {
    entries: std::collections::HashMap<paideia_as_ast::NodeId, HygienicId>,
}

impl HygienicRenameMap {
    /// A fresh, empty map.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: std::collections::HashMap::new(),
        }
    }

    /// Look up the hygiene tag for `node_id`, or `None` if the walk
    /// didn't visit it.
    #[must_use]
    pub fn get(&self, node_id: paideia_as_ast::NodeId) -> Option<HygienicId> {
        self.entries.get(&node_id).copied()
    }

    /// Number of entries in the map.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True if the map has no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterate the entries.
    pub fn iter(&self) -> impl Iterator<Item = (paideia_as_ast::NodeId, HygienicId)> + '_ {
        self.entries.iter().map(|(k, v)| (*k, *v))
    }

    /// Insert an entry.  Overwrites any prior value for `node_id`.
    pub fn insert(&mut self, node_id: paideia_as_ast::NodeId, tag: HygienicId) {
        self.entries.insert(node_id, tag);
    }
}

/// Structural walker used by [`hygienic_rename_map`].  Descends
/// everywhere and records each visited node's hygiene tag; when it
/// enters an `Antiquote` node it flips a scope counter so descendants
/// are recorded as [`HYGIENIC_ID_UNTAGGED`] instead of the macro's tag.
struct HygieneAssigner {
    scope_tag: HygienicId,
    antiquote_depth: u32,
    map: HygienicRenameMap,
}

impl SyntaxWalker for HygieneAssigner {
    fn visit_node(&mut self, s: &Syntax<'_>) -> WalkAction {
        use paideia_as_ast::reflect::TermHead;
        let head = s.head_kind();
        let is_antiquote_root = head.term_head == TermHead::Antiquote;
        if is_antiquote_root {
            self.antiquote_depth += 1;
        }
        let tag = if self.antiquote_depth > 0 {
            HYGIENIC_ID_UNTAGGED
        } else {
            self.scope_tag
        };
        self.map.insert(s.node_id(), tag);
        WalkAction::Descend
    }

    fn exit_node(&mut self, s: &Syntax<'_>) {
        use paideia_as_ast::reflect::TermHead;
        if s.head_kind().term_head == TermHead::Antiquote {
            self.antiquote_depth = self.antiquote_depth.saturating_sub(1);
        }
    }
}

/// Compute a per-node hygiene map for the alpha-rename of `syntax`
/// under `macro_scope`.  See [`HygienicRenameMap`] for semantics.
///
/// This is the granular counterpart to [`hygienic_rename`]: the latter
/// hands back a root-tagged `Syntax` and lets `.children()` flow the
/// tag; the former enumerates every reachable node so a name resolver
/// can consult the correct tag per identifier — including the
/// antiquote-descendant reset to [`HYGIENIC_ID_UNTAGGED`].
#[must_use]
pub fn hygienic_rename_map(
    arena: &AstArena,
    root: paideia_as_ast::NodeId,
    macro_scope: MacroScopeId,
) -> HygienicRenameMap {
    let mut assigner = HygieneAssigner {
        scope_tag: HygienicId::for_macro_scope(macro_scope),
        antiquote_depth: 0,
        map: HygienicRenameMap::new(),
    };
    walk_syntax(&mut assigner, arena, root);
    assigner.map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_hygienic_ids_are_distinct() {
        let a = fresh_hygienic_id();
        let b = fresh_hygienic_id();
        assert_ne!(a, b);
        assert!(a.get() >= 2);
        assert!(b.get() >= 2);
    }

    #[test]
    fn fresh_hygienic_id_never_returns_untagged_sentinel() {
        for _ in 0..1024 {
            let id = fresh_hygienic_id();
            assert!(!id.is_untagged(), "fresh id must not be the untagged sentinel");
            assert!(id.get() >= 2);
        }
    }

    #[test]
    fn fresh_hygienic_id_stays_in_unscoped_range() {
        // R220.M2 invariant: unscoped-fresh ids never claim the
        // MACRO_SCOPE_BIT so they cannot alias a macro-scope id.
        for _ in 0..1024 {
            let id = fresh_hygienic_id();
            assert!(!id.is_macro_scope(), "unscoped fresh id must not carry the macro-scope bit");
            assert!(id.macro_scope().is_none());
        }
    }

    #[test]
    fn untagged_sentinel_is_untagged() {
        assert!(HYGIENIC_ID_UNTAGGED.is_untagged());
        assert!(!HYGIENIC_ID_UNTAGGED.is_macro_scope());
        assert!(HYGIENIC_ID_UNTAGGED.macro_scope().is_none());
        assert_eq!(HYGIENIC_ID_UNTAGGED.get(), 1);
    }

    #[test]
    fn from_raw_round_trip() {
        let id = fresh_hygienic_id();
        let raw = id.get();
        let round = HygienicId::from_raw(raw).unwrap();
        assert_eq!(round, id);
    }

    #[test]
    fn from_raw_rejects_zero() {
        assert!(HygienicId::from_raw(0).is_none());
    }

    #[test]
    fn display_formats() {
        assert_eq!(format!("{}", HYGIENIC_ID_UNTAGGED), "h#untagged");
        let id = HygienicId::from_raw(42).unwrap();
        assert_eq!(format!("{}", id), "h#42");
        let scope = fresh_macro_scope_id();
        let scoped = HygienicId::for_macro_scope(scope);
        assert_eq!(format!("{}", scoped), format!("h#scope={}", scope.get()));
    }

    // ── R220.M2: MacroScopeId + for_macro_scope + is_macro_scope ──────

    #[test]
    fn fresh_macro_scope_ids_are_distinct() {
        let a = fresh_macro_scope_id();
        let b = fresh_macro_scope_id();
        assert_ne!(a, b);
        assert!(a.get() >= 1);
        assert!(b.get() >= 1);
    }

    #[test]
    fn macro_scope_id_from_raw_rejects_zero_and_out_of_range() {
        assert!(MacroScopeId::from_raw(0).is_none());
        assert!(MacroScopeId::from_raw(MAX_MACRO_SCOPE_ID).is_some());
        assert!(MacroScopeId::from_raw(MAX_MACRO_SCOPE_ID + 1).is_none());
        assert!(MacroScopeId::from_raw(u32::MAX).is_none());
    }

    #[test]
    fn hygienic_id_for_macro_scope_round_trips() {
        let scope = fresh_macro_scope_id();
        let tag = HygienicId::for_macro_scope(scope);
        assert!(tag.is_macro_scope());
        assert!(!tag.is_untagged());
        assert_eq!(tag.macro_scope(), Some(scope));
    }

    #[test]
    fn hygienic_id_macro_scope_is_none_for_unscoped_fresh() {
        let unscoped = fresh_hygienic_id();
        assert!(!unscoped.is_macro_scope());
        assert!(unscoped.macro_scope().is_none());
    }

    #[test]
    fn different_scope_ids_produce_distinct_hygienic_ids() {
        let s1 = fresh_macro_scope_id();
        let s2 = fresh_macro_scope_id();
        let h1 = HygienicId::for_macro_scope(s1);
        let h2 = HygienicId::for_macro_scope(s2);
        assert_ne!(h1, h2);
    }

    #[test]
    fn scope_encoding_matches_docs() {
        // The docs promise the packed encoding sets the MACRO_SCOPE_BIT
        // and stores the scope id in the low 31 bits.  Freeze that
        // guarantee so a future re-encoding notices at compile time.
        let scope = MacroScopeId::from_raw(7).unwrap();
        let tag = HygienicId::for_macro_scope(scope);
        assert_eq!(tag.get() & MACRO_SCOPE_BIT, MACRO_SCOPE_BIT);
        assert_eq!(tag.get() & MAX_MACRO_SCOPE_ID, 7);
    }

    // ── hygienic_rename structural behaviour ─────────────────────────

    fn span() -> paideia_as_diagnostics::Span {
        use paideia_as_diagnostics::{FileId, Span};
        Span::new(FileId::new(1).unwrap(), 0, 1)
    }

    #[test]
    fn hygienic_rename_tags_top_level_with_scope() {
        use paideia_as_ast::{AstArena, NodeKind};
        let mut arena = AstArena::new();
        let ident = arena.alloc(NodeKind::Ident, span());
        let path = Syntax::var(&mut arena, span(), ident);
        let s = Syntax::from_node(&arena, path);
        assert!(s.hygiene().is_untagged());

        let scope = fresh_macro_scope_id();
        let renamed = hygienic_rename(&s, scope);
        assert!(renamed.hygiene().is_macro_scope());
        assert_eq!(renamed.hygiene().macro_scope(), Some(scope));
    }

    #[test]
    fn hygienic_rename_preserves_node_id() {
        use paideia_as_ast::{AstArena, NodeKind};
        let mut arena = AstArena::new();
        let ident = arena.alloc(NodeKind::Ident, span());
        let path = Syntax::var(&mut arena, span(), ident);
        let s = Syntax::from_node(&arena, path);

        let scope = fresh_macro_scope_id();
        let renamed = hygienic_rename(&s, scope);
        assert_eq!(renamed.node_id(), s.node_id());
    }

    #[test]
    fn different_scopes_yield_alpha_distinct_hygiene() {
        use paideia_as_ast::{AstArena, NodeKind};
        let mut arena = AstArena::new();
        let ident = arena.alloc(NodeKind::Ident, span());
        let path = Syntax::var(&mut arena, span(), ident);
        let s = Syntax::from_node(&arena, path);

        let scope_a = fresh_macro_scope_id();
        let scope_b = fresh_macro_scope_id();
        let a = hygienic_rename(&s, scope_a);
        let b = hygienic_rename(&s, scope_b);
        assert_ne!(a.hygiene(), b.hygiene(),
            "two macro invocations must produce alpha-distinct scope tags");
    }

    #[test]
    fn hygienic_rename_map_tags_every_node() {
        use paideia_as_ast::{AstArena, NodeKind};
        let mut arena = AstArena::new();
        let ident = arena.alloc(NodeKind::Ident, span());
        let callee = Syntax::var(&mut arena, span(), ident);
        let arg_ph = arena.alloc(NodeKind::Placeholder, span());
        let arg = Syntax::literal(&mut arena, span(), arg_ph);
        let call = Syntax::app(&mut arena, span(), callee, vec![arg]);

        let scope = fresh_macro_scope_id();
        let map = hygienic_rename_map(&arena, call, scope);
        let tag = HygienicId::for_macro_scope(scope);

        assert_eq!(map.get(call), Some(tag));
        assert_eq!(map.get(callee), Some(tag));
        assert_eq!(map.get(ident), Some(tag));
        assert_eq!(map.get(arg), Some(tag));
        assert_eq!(map.get(arg_ph), Some(tag));
    }

    #[test]
    fn hygienic_rename_map_leaves_antiquote_descendants_untagged() {
        use paideia_as_ast::{AstArena, ExprData, NodeKind};
        let mut arena = AstArena::new();
        // Build:  Path( ident )  wrapped in  Antiquote(...)  — the
        // antiquote descendant should be untagged even though the
        // enclosing tree is renamed under a macro scope.
        let ident = arena.alloc(NodeKind::Ident, span());
        let use_site_path = arena.alloc_expr(
            NodeKind::ExprPath,
            span(),
            ExprData::Path { segments: vec![ident] },
        );
        let anti = arena.alloc_expr(
            NodeKind::ExprAntiquote,
            span(),
            ExprData::Antiquote { value: use_site_path },
        );

        let scope = fresh_macro_scope_id();
        let map = hygienic_rename_map(&arena, anti, scope);
        let scope_tag = HygienicId::for_macro_scope(scope);

        // The antiquote root itself is the boundary; per the pass it is
        // recorded as untagged (already inside the antiquote scope from
        // the visitor's point of view).
        assert_eq!(map.get(anti), Some(HYGIENIC_ID_UNTAGGED));
        // Descendants are untagged (use-site).
        assert_eq!(map.get(use_site_path), Some(HYGIENIC_ID_UNTAGGED));
        assert_eq!(map.get(ident), Some(HYGIENIC_ID_UNTAGGED));
        // Sanity check: scope_tag differs from untagged.
        assert_ne!(scope_tag, HYGIENIC_ID_UNTAGGED);
    }

    #[test]
    fn hygienic_rename_map_mixes_scope_and_untagged_across_antiquote_boundary() {
        use paideia_as_ast::{AstArena, ExprData, NodeKind};
        let mut arena = AstArena::new();
        // Build:  Call( macro_ident, Antiquote(use_site_ident) )
        // Every node outside the antiquote gets the macro's scope; the
        // antiquote descendant is untagged.
        let macro_ident = arena.alloc(NodeKind::Ident, span());
        let macro_callee = arena.alloc_expr(
            NodeKind::ExprPath,
            span(),
            ExprData::Path { segments: vec![macro_ident] },
        );
        let use_site_ident = arena.alloc(NodeKind::Ident, span());
        let use_site_path = arena.alloc_expr(
            NodeKind::ExprPath,
            span(),
            ExprData::Path { segments: vec![use_site_ident] },
        );
        let anti = arena.alloc_expr(
            NodeKind::ExprAntiquote,
            span(),
            ExprData::Antiquote { value: use_site_path },
        );
        let call = arena.alloc_expr(
            NodeKind::ExprCall,
            span(),
            ExprData::Call { callee: macro_callee, args: vec![anti] },
        );

        let scope = fresh_macro_scope_id();
        let scope_tag = HygienicId::for_macro_scope(scope);
        let map = hygienic_rename_map(&arena, call, scope);

        // Macro side.
        assert_eq!(map.get(call), Some(scope_tag));
        assert_eq!(map.get(macro_callee), Some(scope_tag));
        assert_eq!(map.get(macro_ident), Some(scope_tag));
        // Antiquote side: untagged all the way down.
        assert_eq!(map.get(anti), Some(HYGIENIC_ID_UNTAGGED));
        assert_eq!(map.get(use_site_path), Some(HYGIENIC_ID_UNTAGGED));
        assert_eq!(map.get(use_site_ident), Some(HYGIENIC_ID_UNTAGGED));
    }
}
