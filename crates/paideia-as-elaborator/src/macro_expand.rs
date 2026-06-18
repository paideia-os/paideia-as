//! Macro template expansion (phase-1, no hygiene yet).
//!
//! Given a matched rule and its bindings (from [`macro_match`]),
//! substitute each `$name` reference in the template with the bound
//! fragment, producing an expanded source string. Macro invocations in
//! the expanded text are themselves expanded, up to
//! [`MAX_EXPANSION_DEPTH`] = 100 nested invocations.
//!
//! Phase-1 stores templates as raw byte ranges (per PR-46); expansion
//! is a string substitution. The hygiene story arrives in PR-49 — at
//! that point the renamer can be inserted as a single pass over the
//! expanded text before re-parsing.
//!
//! [`macro_match`]: crate::macro_match

use std::collections::HashMap;

use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, Span};

use crate::macro_match::MatchBinding;

/// Maximum nested macro expansion depth before [`M_RECURSION_LIMIT`]
/// fires. Phase-1 picks 100 per Walker / standard practice; tunable
/// per-invocation later.
pub const MAX_EXPANSION_DEPTH: usize = 100;

/// Diagnostic code for unbound metavariable reference in a template.
pub const M_UNBOUND_META: u16 = 309;

/// Diagnostic code for macro recursion-depth overflow.
pub const M_RECURSION_LIMIT: u16 = 311;

/// Result of expanding a template.
#[derive(Debug, Clone)]
pub struct ExpansionOutcome {
    /// The substituted source text (ready to be re-parsed).
    pub expanded: String,
    /// Diagnostics emitted during substitution.
    pub diagnostics: Vec<Diagnostic>,
}

/// Substitute `$name` references in `template` with the corresponding
/// `MatchBinding::captured` text.
///
/// References to an unbound name emit one `M0309` diagnostic per
/// unique unbound name; the substitution leaves the `$name` text in
/// place so the re-parser surfaces a parse error at a useful location.
#[must_use]
pub fn expand_template(
    template: &str,
    bindings: &[MatchBinding],
    invocation_span: Span,
) -> ExpansionOutcome {
    let by_name: HashMap<&str, &str> = bindings
        .iter()
        .map(|b| (b.name.as_str(), b.captured.as_str()))
        .collect();
    let mut out = String::with_capacity(template.len());
    let mut diags = Vec::new();
    let mut reported_unbound: HashMap<String, ()> = HashMap::new();

    let mut iter = template.char_indices().peekable();
    while let Some((i, ch)) = iter.next() {
        if ch == '$' {
            // Collect a `name` identifier following the `$`.
            let name_start = i + 1;
            let mut name_end = name_start;
            while let Some(&(j, c)) = iter.peek() {
                if c.is_alphanumeric() || c == '_' {
                    name_end = j + c.len_utf8();
                    iter.next();
                } else {
                    break;
                }
            }

            if name_end == name_start {
                // Lone `$`, leave in place.
                out.push('$');
                continue;
            }

            let name = &template[name_start..name_end];
            if let Some(replacement) = by_name.get(name) {
                out.push_str(replacement);
            } else {
                // Unbound — record + leave the literal in place.
                if !reported_unbound.contains_key(name) {
                    diags.push(
                        Diagnostic::error(m_code(M_UNBOUND_META))
                            .message(format!("unbound metavariable `${name}` in macro template",))
                            .with_span(invocation_span)
                            .finish(),
                    );
                    reported_unbound.insert(name.to_string(), ());
                }
                out.push('$');
                out.push_str(name);
            }
        } else {
            out.push(ch);
        }
    }

    ExpansionOutcome {
        expanded: out,
        diagnostics: diags,
    }
}

/// Track the depth of nested macro expansions. Returns one
/// `M0311` diagnostic if `depth` exceeds [`MAX_EXPANSION_DEPTH`].
#[must_use]
pub fn check_depth(depth: usize, invocation_span: Span) -> Vec<Diagnostic> {
    if depth > MAX_EXPANSION_DEPTH {
        vec![
            Diagnostic::error(m_code(M_RECURSION_LIMIT))
                .message(format!(
                    "macro expansion depth {depth} exceeds limit of {MAX_EXPANSION_DEPTH}; \
                     possible self-referential macro"
                ))
                .with_span(invocation_span)
                .finish(),
        ]
    } else {
        Vec::new()
    }
}

fn m_code(n: u16) -> DiagnosticCode {
    DiagnosticCode::new(Category::M, Severity::Error, n).expect("valid M code")
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_ast::MacroFragmentKind;
    use paideia_as_diagnostics::FileId;

    fn span() -> Span {
        Span::new(FileId::new(1).unwrap(), 0, 1)
    }

    fn bind(name: &str, captured: &str) -> MatchBinding {
        MatchBinding {
            name: name.to_string(),
            kind: MacroFragmentKind::Expr,
            captured: captured.to_string(),
        }
    }

    #[test]
    fn simple_substitution() {
        let out = expand_template("$x + 1", &[bind("x", "42")], span());
        assert!(out.diagnostics.is_empty());
        assert_eq!(out.expanded, "42 + 1");
    }

    #[test]
    fn with_handler_macro_expansion_shape() {
        // The §1.4 `with_handler` example expands an invocation into a
        // multi-line block; we sanity-check the substitution shape.
        let template = "{ with $handler handle $eff { $body } }";
        let bindings = vec![
            bind("handler", "io_h"),
            bind("eff", "Io"),
            bind("body", "read()"),
        ];
        let out = expand_template(template, &bindings, span());
        assert!(out.diagnostics.is_empty());
        assert!(out.expanded.contains("with io_h handle Io"));
        assert!(out.expanded.contains("read()"));
    }

    #[test]
    fn unbound_metavariable_emits_m0309() {
        let out = expand_template("$x + $y", &[bind("x", "1")], span());
        assert_eq!(out.diagnostics.len(), 1);
        assert_eq!(out.diagnostics[0].code().number(), 309);
        assert_eq!(out.expanded, "1 + $y");
    }

    #[test]
    fn unbound_reported_once_per_unique_name() {
        let out = expand_template("$y + $y", &[], span());
        // Only one M0309, not two.
        assert_eq!(out.diagnostics.len(), 1);
    }

    #[test]
    fn lone_dollar_passes_through() {
        let out = expand_template("a $ b", &[], span());
        assert!(out.diagnostics.is_empty());
        assert_eq!(out.expanded, "a $ b");
    }

    #[test]
    fn no_substitution_when_no_dollars() {
        let out = expand_template("hello world", &[], span());
        assert!(out.diagnostics.is_empty());
        assert_eq!(out.expanded, "hello world");
    }

    #[test]
    fn recursion_limit_check_within_limit_passes() {
        assert!(check_depth(50, span()).is_empty());
        assert!(check_depth(MAX_EXPANSION_DEPTH, span()).is_empty());
    }

    #[test]
    fn recursion_limit_check_overflow_emits_m0311() {
        let diags = check_depth(MAX_EXPANSION_DEPTH + 1, span());
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), 311);
    }

    #[test]
    fn multi_char_metavariable_name() {
        let out = expand_template("$long_name + 1", &[bind("long_name", "42")], span());
        assert!(out.diagnostics.is_empty());
        assert_eq!(out.expanded, "42 + 1");
    }

    #[test]
    fn unicode_in_captured_text() {
        // The captured text can contain Unicode (e.g. operator glyphs).
        let out = expand_template("$x", &[bind("x", "α → β")], span());
        assert_eq!(out.expanded, "α → β");
    }
}
