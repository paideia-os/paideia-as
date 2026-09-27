//! Macro-related AST nodes for phase-1 pattern-based macros.
//!
//! Slice A (PAS-DEBT-B2-010, #1503, v0.36.52) landed a real fragment-kind
//! pattern grammar: `MacroRule::pattern_elems` is now a structured
//! `Vec<MacroPatternElem>`, and the pattern arena node is
//! [`crate::NodeKind::MacroPattern`] (not a bare `Placeholder`).
//!
//! Slice B (PAS-DEBT-B2-010b, #1541, v0.36.65) lifts templates to the
//! same structured shape: `MacroRule::template_elems` is a
//! `Vec<MacroTemplateElem>` (fragment references + literal spans), and
//! the template arena node is [`crate::NodeKind::MacroTemplate`] (also
//! no longer a bare `Placeholder`). Repetition + hygiene (Slice C →
//! follow-up B2-010c, #1542) remain deferred.

use crate::NodeId;
use paideia_as_diagnostics::Span;

/// One pattern fragment in a macro rule, e.g. `$x:expr`.
///
/// Fragments bind portions of the input to names that are substituted in the
/// template. The kind determines what syntactic category the fragment matches.
#[derive(Clone, Debug)]
pub struct MacroFragment {
    /// Name of the fragment binding (e.g. `x` in `$x:expr`).
    /// Points to an Ident node.
    pub name: NodeId,
    /// Kind selector (e.g. `expr`, `ident`, `type`, `literal`, `pat`,
    /// `stmt`, `block`, `tt`).
    pub kind: MacroFragmentKind,
}

/// Fragment kind: determines what syntactic category a fragment matches.
///
/// Matches the Rust macro_rules! syntax-category identifiers.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum MacroFragmentKind {
    /// `$x:expr` — any expression.
    Expr,
    /// `$x:type` or `$x:ty` — any type.
    Ty,
    /// `$x:ident` — any identifier.
    Ident,
    /// `$x:literal` — any literal.
    Literal,
    /// `$x:pat` — any pattern (for use in match expressions).
    Pat,
    /// `$x:stmt` — any statement.
    Stmt,
    /// `$x:block` — any block expression `{ ... }`.
    Block,
    /// `$x:tt` — any token tree (most permissive).
    Tt,
}

impl MacroFragmentKind {
    /// Parse a fragment kind from its string representation.
    ///
    /// Returns `None` if the string does not match a known kind.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "expr" => Some(MacroFragmentKind::Expr),
            "ty" | "type" => Some(MacroFragmentKind::Ty),
            "ident" => Some(MacroFragmentKind::Ident),
            "literal" => Some(MacroFragmentKind::Literal),
            "pat" => Some(MacroFragmentKind::Pat),
            "stmt" => Some(MacroFragmentKind::Stmt),
            "block" => Some(MacroFragmentKind::Block),
            "tt" => Some(MacroFragmentKind::Tt),
            _ => None,
        }
    }

    /// Convert back to string form for diagnostics.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            MacroFragmentKind::Expr => "expr",
            MacroFragmentKind::Ty => "ty",
            MacroFragmentKind::Ident => "ident",
            MacroFragmentKind::Literal => "literal",
            MacroFragmentKind::Pat => "pat",
            MacroFragmentKind::Stmt => "stmt",
            MacroFragmentKind::Block => "block",
            MacroFragmentKind::Tt => "tt",
        }
    }
}

/// One element of a macro rule's pattern.
///
/// Slice A structures the pattern as an ordered sequence of literal token
/// spans and fragment metavariables. Templates remain span-only until
/// follow-up B2-010b lands substitution.
#[derive(Clone, Debug)]
pub enum MacroPatternElem {
    /// `$name:kind` — a fragment metavariable.
    Fragment {
        /// Name of the fragment binding (Ident node, spans just the name
        /// text without the leading `$`).
        name: NodeId,
        /// Syntactic category the fragment matches.
        kind: MacroFragmentKind,
        /// Span of the entire `$name:kind` fragment site (leading `$`
        /// through the last char of the kind selector).
        span: Span,
    },
    /// A literal token from the pattern surface (anything that is not a
    /// `$name:kind` fragment). The matcher treats these as required
    /// terminals. Kept as a raw span in Slice A; a structured token
    /// stream is Slice B's concern (B2-010b).
    Literal {
        /// Byte range covered by the literal token.
        span: Span,
    },
}

/// One element of a macro rule's template.
///
/// Slice B (PAS-DEBT-B2-010b, #1541) structures the template as an
/// ordered sequence of literal source spans and fragment references
/// (`$name`), mirroring [`MacroPatternElem`] on the pattern side.
///
/// Unlike the pattern side, the template's fragment reference carries
/// no `kind` field — the kind lives on the matching
/// [`MacroPatternElem::Fragment`] and is resolved through the fragment
/// name during expansion.
///
/// Repetition (`$( ... )*`) is Slice C's concern (#1542); marking
/// this enum `#[non_exhaustive]` at introduction so Slice C can add
/// variants without a SemVer-blocking break at cross-crate match
/// sites — every external `match` on `MacroTemplateElem` must carry
/// a wildcard `_` arm.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum MacroTemplateElem {
    /// `$name` — a reference to a fragment bound by the rule's pattern.
    Fragment {
        /// Name of the fragment reference (Ident node, spans just the
        /// name text without the leading `$`).
        name: NodeId,
        /// Span of the entire `$name` reference site (leading `$`
        /// through the last char of the name).
        span: Span,
    },
    /// A literal source range in the template (anything that is not a
    /// `$name` reference). The expander copies these bytes verbatim
    /// into the emitted token stream. Kept as a raw span in Slice B;
    /// pre-tokenising the literal segments is a follow-up nicety.
    Literal {
        /// Byte range covered by the literal segment.
        span: Span,
    },
}

/// One rule: pattern → template.
///
/// A macro has one or more rules. When invoked, the macro expander tries to
/// match the input against each rule's pattern; the first match uses that
/// rule's template.
#[derive(Clone, Debug)]
pub struct MacroRule {
    /// Pattern node id. Since Slice A (v0.36.52) this is a
    /// [`crate::NodeKind::MacroPattern`] node whose span covers the byte
    /// range of the pattern token stream (between `(` and `)`). The
    /// structural detail lives in [`Self::pattern_elems`]; the arena node
    /// carries the identity + span.
    pub pattern: NodeId,
    /// Template node. Since Slice B (v0.36.65) this is a
    /// [`crate::NodeKind::MacroTemplate`] node whose span covers the
    /// raw template bytes (after `=>` until `;` in multi-rule form, or
    /// until end-of-rule in single-rule form). The structural detail
    /// lives in [`Self::template_elems`]; the arena node carries the
    /// identity + span. Prior to Slice B this was a bare `Placeholder`
    /// and the text-walking expander in
    /// `paideia-as-elaborator::macro_expand::expand_template`
    /// interpolated `$var` references directly against source bytes
    /// (that path still exists as a fallback).
    pub template: NodeId,
    /// Structured pattern element sequence.
    ///
    /// Interleaves fragment metavariables with literal token spans in the
    /// order they appear in source. Slice A (#1503) parses this out of
    /// the char-scanned pattern text; Slice C (#follow-up B2-010c) will
    /// add repetition groups (`$( ... )*`).
    pub pattern_elems: Vec<MacroPatternElem>,
    /// Fragment-only projection of [`Self::pattern_elems`], kept as a
    /// thin duplicate so `paideia-as-elaborator::macro_match` and other
    /// downstream consumers that only care about `$name:kind`
    /// declarations do not have to re-walk the interleaved element list.
    /// Slice A holds both fields in sync at parse time.
    pub fragments: Vec<MacroFragment>,
    /// Structured template element sequence.
    ///
    /// Interleaves fragment references (`$name`) with literal source
    /// spans in the order they appear in source. Slice B (#1541) parses
    /// this out of the char-scanned template text alongside
    /// [`Self::pattern_elems`]; Slice C (#1542) will add repetition
    /// groups (`$( ... )*`).
    pub template_elems: Vec<MacroTemplateElem>,
}

/// `MacroDecl` ItemData payload.
///
/// Represents a top-level macro declaration, e.g.:
/// ```paideia-as
/// macro foo($x:expr) => { x + x }
/// ```
/// or
/// ```paideia-as
/// macro bar {
///     ($x:expr) => { x + 1 }
///     ($x:expr, $y:expr) => { x + y }
/// }
/// ```
#[derive(Clone, Debug)]
pub struct MacroDeclData {
    /// Name of the macro (Ident node).
    pub name: NodeId,
    /// The rules: one or more pattern → template mappings.
    pub rules: Vec<MacroRule>,
    /// Optional documentation comment (StringLit node).
    pub doc: Option<NodeId>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragment_kind_from_str_expr() {
        assert_eq!(
            MacroFragmentKind::parse("expr"),
            Some(MacroFragmentKind::Expr)
        );
    }

    #[test]
    fn fragment_kind_from_str_ty_variant() {
        assert_eq!(MacroFragmentKind::parse("ty"), Some(MacroFragmentKind::Ty));
    }

    #[test]
    fn fragment_kind_from_str_type_variant() {
        assert_eq!(
            MacroFragmentKind::parse("type"),
            Some(MacroFragmentKind::Ty)
        );
    }

    #[test]
    fn fragment_kind_from_str_unknown() {
        assert_eq!(MacroFragmentKind::parse("wat"), None);
    }

    #[test]
    fn fragment_kind_as_str_roundtrip() {
        let kinds = [
            MacroFragmentKind::Expr,
            MacroFragmentKind::Ty,
            MacroFragmentKind::Ident,
            MacroFragmentKind::Literal,
            MacroFragmentKind::Pat,
            MacroFragmentKind::Stmt,
            MacroFragmentKind::Block,
            MacroFragmentKind::Tt,
        ];
        for kind in &kinds {
            let s = kind.as_str();
            let kind2 = MacroFragmentKind::parse(s).expect("should round-trip");
            assert_eq!(*kind, kind2);
        }
    }
}
