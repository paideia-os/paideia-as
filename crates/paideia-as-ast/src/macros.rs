//! Macro-related AST nodes for phase-1 pattern-based macros.
//!
//! Supports macro declarations with pattern → template rules. Pattern matching
//! and expansion are deferred to later PRs (PR-47+).

use crate::NodeId;

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

/// One rule: pattern → template.
///
/// A macro has one or more rules. When invoked, the macro expander tries to
/// match the input against each rule's pattern; the first match uses that
/// rule's template.
#[derive(Clone, Debug)]
pub struct MacroRule {
    /// Pattern node. In phase-1, this is a `Placeholder` node whose span
    /// covers the byte range of the pattern token stream (between `(` and `)`).
    /// The matcher (PR-47) walks the raw source text to match the pattern.
    pub pattern: NodeId,
    /// Template node. In phase-1, this is a `Placeholder` node whose span
    /// covers the byte range of the template token stream (after `=>` until `;`
    /// in multi-rule form, or until `}` in single-rule or final rule).
    /// The expander (PR-47) walks the raw source text to interpolate `$var`
    /// references.
    pub template: NodeId,
    /// Fragment list extracted from the pattern.
    ///
    /// Contains all `$name:kind` declarations found during pattern parsing.
    /// Used for quick lookup during expansion and validation of template
    /// references (PR-47+).
    pub fragments: Vec<MacroFragment>,
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
