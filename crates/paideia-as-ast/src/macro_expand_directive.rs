//! Compile-time `@macro_expand` directive side-table
//! (paideia-as#1556, PAS-DEBT-B7-001-b, v0.36.74).
//!
//! # Purpose
//!
//! Slice B7-001 (paideia-as#1529) discovered that although the
//! elaborator's `expand_macro` implementation had already landed via
//! Slices A/B/C (paideia-as#1503, #1541, #1542), the `cmd_build`
//! pipeline never invoked it — no user-authored macro invocation
//! syntax exists yet (that is R221.M4's concern). The corpus tests in
//! `tests/reflection-corpus` therefore had no way to exercise the
//! structured expander end-to-end.
//!
//! This side-table stores a list of `@macro_expand(...)` directives
//! recovered by the parser at top-level. Each directive names a
//! previously-declared macro (by identifier) plus a string literal
//! carrying the input token text to feed the matcher/expander. The
//! elaborator's `macro_expand_pass` walks the table after parse and
//! runs the matched macro through `match_structured` +
//! `expand_macro`, piping any diagnostics (M0308 / M0309 / M0310 /
//! M0311 / M0314) into the normal `DiagnosticSink → SARIF` route.
//!
//! # Design
//!
//! `@macro_expand([<macro_ident>, "<input>"])` is a top-level
//! standalone directive rather than an attribute prefix on the macro
//! declaration itself: this decouples the fixture author from having
//! to touch the declaration's grammar (which Slices A/B/C freeze) and
//! lets a single macro be exercised against multiple inputs by
//! stacking several directives.
//!
//! Storage is a flat `Vec<MacroExpandDirective>` on the arena rather
//! than a `HashMap<NodeId, ...>` because the directive has no owning
//! macro-decl NodeId at parse time (name resolution happens later in
//! the elaborator pass).
//!
//! # Scope
//!
//! **INTERNAL-ONLY.** The `@macro_expand` annotation is a
//! compile-time driver hook — an intermediate before the R221.M4
//! macro-invocation parser lands. User macros should NOT rely on it
//! as a public surface: once `foo!(...)` invocation syntax exists,
//! this directive can be retired (Slice D of B7-001).
//!
//! # Diagnostics
//!
//! Emitted by the elaborator pass, not by the parser (which just
//! records the directive):
//!
//! - **M0308** — no rule in the referenced macro's pattern list
//!   matched the supplied input (reused from `macro_match::M_NO_MATCH`).
//! - **M0309** — unbound `$name` reference in the template
//!   (reused from `macro_expand::M_UNBOUND_META`).
//! - **M0310** — repetition-count mismatch inside a matched rule
//!   (reused from `macro_match::M_REP_COUNT_MISMATCH`).
//! - **M0311** — recursion limit exceeded during expansion
//!   (reused from `macro_expand::M_RECURSION_LIMIT`).
//! - **M0314** — template repetition misuse
//!   (reused from `macro_expand::M_TEMPLATE_REP_MISUSE`).
//! - **M0315** — referenced macro name not found in scope (new to
//!   Slice B7-001-b; the resolver falls back to a substring scan of
//!   MacroDecl items in the arena).

use paideia_as_diagnostics::Span;

/// One `@macro_expand([<macro>, "<input>"])` directive recorded by the
/// parser at top-level.
///
/// The parser stores plain owned strings rather than `NodeId`s because
/// the referenced macro's declaration may not exist yet (forward
/// reference across source order) and because the input text is a
/// re-lex-time byte range, not an AST node.
#[derive(Clone, Debug)]
pub struct MacroExpandDirective {
    /// Name of the referenced macro, exactly as spelled in the
    /// `@macro_expand([<name>, ...])` first argument.
    pub macro_name: String,
    /// Input token text: the raw source-level bytes the elaborator's
    /// expander receives as the invocation's argument list. Recovered
    /// from the string literal's interior (with surrounding `"` stripped).
    pub input: String,
    /// Span of the whole `@macro_expand(...)` directive, used as the
    /// anchor for downstream diagnostics.
    pub span: Span,
}

/// Flat list of every `@macro_expand(...)` directive the parser has
/// recorded.
///
/// The order of insertion mirrors source order so the elaborator pass
/// emits diagnostics in a stable, human-readable sequence.
#[derive(Debug, Default)]
pub struct MacroExpandDirectiveTable {
    entries: Vec<MacroExpandDirective>,
}

impl MacroExpandDirectiveTable {
    /// Construct an empty table.
    #[must_use]
    pub fn new() -> Self {
        Self { entries: Vec::new() }
    }

    /// Append `directive` to the table.
    pub fn push(&mut self, directive: MacroExpandDirective) {
        self.entries.push(directive);
    }

    /// Borrow the underlying slice for read-only traversal.
    #[must_use]
    pub fn entries(&self) -> &[MacroExpandDirective] {
        &self.entries
    }

    /// Number of recorded directives.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` iff no directive has been recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::{FileId, Span};

    fn span() -> Span {
        Span::new(FileId::new(1).unwrap(), 0, 1)
    }

    #[test]
    fn new_is_empty() {
        let t = MacroExpandDirectiveTable::new();
        assert!(t.is_empty());
        assert_eq!(t.len(), 0);
        assert!(t.entries().is_empty());
    }

    #[test]
    fn push_appends() {
        let mut t = MacroExpandDirectiveTable::new();
        t.push(MacroExpandDirective {
            macro_name: "id".to_string(),
            input: "42".to_string(),
            span: span(),
        });
        assert_eq!(t.len(), 1);
        assert_eq!(t.entries()[0].macro_name, "id");
        assert_eq!(t.entries()[0].input, "42");
    }

    #[test]
    fn push_preserves_insertion_order() {
        let mut t = MacroExpandDirectiveTable::new();
        t.push(MacroExpandDirective {
            macro_name: "a".to_string(),
            input: "1".to_string(),
            span: span(),
        });
        t.push(MacroExpandDirective {
            macro_name: "b".to_string(),
            input: "2".to_string(),
            span: span(),
        });
        let e = t.entries();
        assert_eq!(e[0].macro_name, "a");
        assert_eq!(e[1].macro_name, "b");
    }
}
