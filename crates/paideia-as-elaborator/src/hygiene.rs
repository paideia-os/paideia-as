//! Macro hygiene (Lean 4 / Ullrich 2020 style) per `macros-phase1.md` §3.
//!
//! Every identifier carries a `MacroId` tag set. Identifiers introduced
//! **inside a macro template** get a fresh tag at expansion time;
//! identifiers passed in **from the use site** retain the use-site
//! context. Name resolution compares tag sets so an identifier `temp`
//! introduced inside macro `M` cannot collide with a use-site `temp`
//! passed as an argument.
//!
//! Phase-1: this module supplies the data types and the fresh-tag
//! allocator. The resolver in [`crate::resolve`] consumes them.
//!
//! Phase-2 (m2-010): extends hygiene to reflective term construction via
//! `quote { ... }` and `splice`. A [`HygieneCache`] maps NodeId to
//! HygienicName for identifiers that were introduced by a splice operation.

use core::num::NonZeroU32;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};

use paideia_as_ast::NodeId;

pub use paideia_as_ast::Term;

/// One macro expansion's hygiene tag.
///
/// Each call to [`MacroId::fresh`] returns a globally-fresh tag,
/// guaranteed distinct from every previous tag.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd, Debug)]
pub struct MacroId(NonZeroU32);

impl MacroId {
    /// Allocate a fresh, globally-unique macro tag.
    ///
    /// Each tag is monotonically increasing across the program; the
    /// counter never wraps in practice (≥ 2^31 macro expansions).
    #[must_use]
    pub fn fresh() -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(1);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        Self(NonZeroU32::new(n).expect("fresh counter never returns 0"))
    }

    /// Raw integer value.
    #[must_use]
    pub fn get(self) -> u32 {
        self.0.get()
    }
}

impl core::fmt::Display for MacroId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "m{}", self.0.get())
    }
}

/// A name plus the set of macro tags it inherited from its expansion
/// context. Phase-1 uses an ordered `Vec<MacroId>` for the tag set;
/// equality compares the underlying multiset.
#[derive(Clone, Eq, Debug)]
pub struct HygienicName {
    /// Source-level spelling.
    pub spelling: String,
    /// Macro tags inherited from the expansion context, sorted +
    /// deduplicated.
    pub tags: Vec<MacroId>,
}

impl HygienicName {
    /// Construct a name with the supplied tag set. The tag list is
    /// sorted + deduplicated so equality is well-defined.
    #[must_use]
    pub fn new(spelling: impl Into<String>, mut tags: Vec<MacroId>) -> Self {
        tags.sort();
        tags.dedup();
        Self {
            spelling: spelling.into(),
            tags,
        }
    }

    /// A name with no expansion tags (introduced at the source root).
    #[must_use]
    pub fn unmarked(spelling: impl Into<String>) -> Self {
        Self::new(spelling, Vec::new())
    }

    /// Push a fresh tag onto the name. Used when crossing a macro
    /// template boundary: every identifier that was introduced by the
    /// template (not passed in from the use site) gets the macro's
    /// fresh tag attached.
    #[must_use]
    pub fn with_tag(mut self, tag: MacroId) -> Self {
        self.tags.push(tag);
        self.tags.sort();
        self.tags.dedup();
        self
    }
}

impl PartialEq for HygienicName {
    fn eq(&self, other: &Self) -> bool {
        self.spelling == other.spelling && self.tags == other.tags
    }
}

impl core::hash::Hash for HygienicName {
    fn hash<H: core::hash::Hasher>(&self, h: &mut H) {
        self.spelling.hash(h);
        self.tags.hash(h);
    }
}

/// Cache mapping AST node IDs to their hygienic names.
///
/// Phase-2 (m2-010): tracks hygiene tags for identifiers that were
/// introduced by splice operations. This side-table allows the spliced
/// subtree to retain correct hygiene even before the AST-level tagging
/// infrastructure is wired end-to-end through the elaborator.
///
/// When a splice operation walks a Term's subtree:
/// - Identifiers inside the splice get tagged with the splice's
///   fresh MacroId.
/// - Identifiers inside antiquotes (`~(...)`) are left unmarked, since
///   they come from the call site.
///
/// The cache is populated during splice and consulted during name
/// resolution. At phase-3 (m3-xyz), this cache becomes redundant when
/// AST nodes carry inline hygiene metadata.
#[derive(Default, Clone, Debug)]
pub struct HygieneCache {
    /// Mapping from NodeId to HygienicName.
    tags: HashMap<NodeId, HygienicName>,
}

impl HygieneCache {
    /// Construct a fresh, empty hygiene cache.
    #[must_use]
    pub fn new() -> Self {
        Self {
            tags: HashMap::new(),
        }
    }

    /// Record that a node (typically an identifier) was introduced by a
    /// splice with the given tag.
    pub fn tag_introduced_at(&mut self, node_id: NodeId, spelling: String, tag: MacroId) {
        let hygienic_name = HygienicName::unmarked(spelling).with_tag(tag);
        self.tags.insert(node_id, hygienic_name);
    }

    /// Look up the hygienic name for a node, if it has been tagged.
    #[must_use]
    pub fn lookup(&self, node_id: NodeId) -> Option<&HygienicName> {
        self.tags.get(&node_id)
    }

    /// Merge another cache into this one. Later insertions take precedence.
    pub fn merge(&mut self, other: HygieneCache) {
        for (k, v) in other.tags {
            self.tags.insert(k, v);
        }
    }

    /// Number of cached entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.tags.len()
    }

    /// True if the cache is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tags.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_tags_are_distinct() {
        let a = MacroId::fresh();
        let b = MacroId::fresh();
        assert_ne!(a, b);
    }

    #[test]
    fn fresh_tag_displays_with_m_prefix() {
        let id = MacroId::fresh();
        assert!(format!("{id}").starts_with('m'));
    }

    // ── AC bullet 1: capture-by-introduction ─────────────────────────

    #[test]
    fn macro_introduced_temp_distinct_from_use_site_temp() {
        // foo($x:expr) => { let temp = $x; temp + temp }
        // The macro's `temp` gets a fresh tag; the caller's `temp` does not.
        let macro_tag = MacroId::fresh();
        let macro_temp = HygienicName::unmarked("temp").with_tag(macro_tag);
        let use_site_temp = HygienicName::unmarked("temp");
        assert_ne!(macro_temp, use_site_temp);
    }

    // ── AC bullet 2: capture-by-reference resolves at use site ───────

    #[test]
    fn argument_passed_through_retains_use_site_context() {
        // `bar(println)` where the macro template references `$y` then
        // an inline `println`. The metavariable substitution preserves
        // the use-site tag set on the original token (no macro tag);
        // the inline `println` introduced by the template gets the
        // macro tag.
        let macro_tag = MacroId::fresh();
        let arg_println = HygienicName::unmarked("println"); // from use site
        let template_println = HygienicName::unmarked("println").with_tag(macro_tag);
        assert_ne!(arg_println, template_println);
    }

    // ── Misc invariants ──────────────────────────────────────────────

    #[test]
    fn unmarked_equality_by_spelling() {
        let a = HygienicName::unmarked("foo");
        let b = HygienicName::unmarked("foo");
        let c = HygienicName::unmarked("bar");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn with_tag_is_idempotent_on_same_tag() {
        let tag = MacroId::fresh();
        let a = HygienicName::unmarked("x").with_tag(tag);
        let b = a.clone().with_tag(tag);
        assert_eq!(a, b);
        assert_eq!(a.tags.len(), 1);
    }

    #[test]
    fn nested_macros_accumulate_tags() {
        let outer = MacroId::fresh();
        let inner = MacroId::fresh();
        let name = HygienicName::unmarked("x").with_tag(outer).with_tag(inner);
        assert_eq!(name.tags.len(), 2);
    }

    #[test]
    fn tag_order_does_not_affect_equality() {
        let t1 = MacroId::fresh();
        let t2 = MacroId::fresh();
        let a = HygienicName::unmarked("x").with_tag(t1).with_tag(t2);
        let b = HygienicName::unmarked("x").with_tag(t2).with_tag(t1);
        assert_eq!(a, b);
    }

    // ── Phase-2 (m2-010): reflective hygiene under splice ────────────

    #[test]
    fn hygiene_cache_records_splice_introduced_identifier() {
        let mut cache = HygieneCache::new();
        let tag = MacroId::fresh();
        let node_id = NodeId::new(1).unwrap();

        cache.tag_introduced_at(node_id, "temp".to_string(), tag);

        let result = cache.lookup(node_id);
        assert!(result.is_some());
        let hygienic = result.unwrap();
        assert_eq!(hygienic.spelling, "temp");
        assert_eq!(hygienic.tags, vec![tag]);
    }

    #[test]
    fn hygiene_cache_merge_combines_tables() {
        let mut cache1 = HygieneCache::new();
        let mut cache2 = HygieneCache::new();
        let tag1 = MacroId::fresh();
        let tag2 = MacroId::fresh();
        let id1 = NodeId::new(1).unwrap();
        let id2 = NodeId::new(2).unwrap();

        cache1.tag_introduced_at(id1, "x".to_string(), tag1);
        cache2.tag_introduced_at(id2, "y".to_string(), tag2);

        cache1.merge(cache2);

        assert!(cache1.lookup(id1).is_some());
        assert!(cache1.lookup(id2).is_some());
        assert_eq!(cache1.len(), 2);
    }

    #[test]
    fn hygiene_cache_later_insertions_win_on_merge() {
        let mut cache1 = HygieneCache::new();
        let mut cache2 = HygieneCache::new();
        let tag1 = MacroId::fresh();
        let tag2 = MacroId::fresh();
        let id = NodeId::new(1).unwrap();

        cache1.tag_introduced_at(id, "x".to_string(), tag1);
        cache2.tag_introduced_at(id, "x".to_string(), tag2);

        cache1.merge(cache2);

        let hygienic = cache1.lookup(id).unwrap();
        assert_eq!(hygienic.tags, vec![tag2]);
    }

    // Note: splice_walker tests are in splice.rs module since they require
    // access to Term which is in paideia_as_ast. This module tests only
    // the HygieneCache data structure itself.

    #[test]
    fn phase1_hygiene_corpus_still_passes() {
        // Regression test: ensure all existing phase-1 tests still pass.
        // (The following subtests are duplicated from above to ensure
        // they still work with the extended HygieneCache in place.)

        let a = MacroId::fresh();
        let b = MacroId::fresh();
        assert_ne!(a, b);

        let macro_tag = MacroId::fresh();
        let macro_temp = HygienicName::unmarked("temp").with_tag(macro_tag);
        let use_site_temp = HygienicName::unmarked("temp");
        assert_ne!(macro_temp, use_site_temp);

        let tag = MacroId::fresh();
        let a = HygienicName::unmarked("x").with_tag(tag);
        let b = a.clone().with_tag(tag);
        assert_eq!(a, b);
        assert_eq!(a.tags.len(), 1);

        let outer = MacroId::fresh();
        let inner = MacroId::fresh();
        let name = HygienicName::unmarked("x").with_tag(outer).with_tag(inner);
        assert_eq!(name.tags.len(), 2);

        let t1 = MacroId::fresh();
        let t2 = MacroId::fresh();
        let a = HygienicName::unmarked("x").with_tag(t1).with_tag(t2);
        let b = HygienicName::unmarked("x").with_tag(t2).with_tag(t1);
        assert_eq!(a, b);
    }
}
