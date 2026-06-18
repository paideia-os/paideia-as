//! Macro-pattern matcher (no hygiene yet) per `macros-phase1.md` §1.2.
//!
//! Phase-1 strategy: walk the rule list at a macro invocation; for each
//! rule, try to match the call-site **string** (raw byte range) against
//! the rule's pattern. A successful match binds each metavariable
//! `$name` to a substring slice. First-match wins per Scheme
//! `syntax-rules` semantics.
//!
//! This is a deliberately small implementation: the AST stores pattern
//! and template as `Placeholder` nodes whose spans cover their byte
//! range (see PR-46), so the matcher walks raw text rather than a
//! structured token tree. The hygiene story arrives in PR-49.

use paideia_as_ast::{MacroDeclData, MacroFragmentKind};
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, Span};

/// Diagnostic code for "no matching macro rule".
pub const M_NO_MATCH: u16 = 308;

/// One binding produced by a successful match: the metavariable name
/// (e.g. `x`) and the source-text slice it captured.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MatchBinding {
    /// Metavariable name (without the leading `$`).
    pub name: String,
    /// Fragment kind declared in the pattern.
    pub kind: MacroFragmentKind,
    /// Captured source-text slice (raw bytes of the substring).
    pub captured: String,
}

/// Outcome of attempting to match a single rule.
#[derive(Clone, Debug)]
pub enum RuleMatch {
    /// Match succeeded; the bindings can drive expansion.
    Ok {
        /// Bindings produced from the successful match.
        bindings: Vec<MatchBinding>,
    },
    /// Match failed (without an error — the caller tries the next rule).
    Failed,
}

/// Outcome of attempting to match an invocation against an entire
/// macro declaration.
#[derive(Debug)]
pub struct InvocationMatch {
    /// Bindings from the first matching rule, or empty if no rule
    /// matched.
    pub bindings: Vec<MatchBinding>,
    /// Index of the matching rule, or `None` if none matched.
    pub rule_index: Option<usize>,
    /// Diagnostics emitted by the matcher.
    pub diagnostics: Vec<Diagnostic>,
}

/// Try every rule of `decl` against the invocation text.
///
/// `pattern_texts` is one string per rule, giving the pattern's raw
/// byte text. `call_text` is the raw byte text of the call's argument
/// list (everything between `(` and `)` at the call site). `call_span`
/// is the source span of the call site, used for diagnostics.
///
/// Phase-1: returns the bindings from the first matching rule. If no
/// rule matches, emits one `M0308` diagnostic.
#[must_use]
pub fn match_invocation(
    decl: &MacroDeclData,
    pattern_texts: &[&str],
    call_text: &str,
    call_span: Span,
) -> InvocationMatch {
    assert_eq!(
        decl.rules.len(),
        pattern_texts.len(),
        "pattern_texts must have one entry per rule"
    );

    for (i, _rule) in decl.rules.iter().enumerate() {
        let pattern = pattern_texts[i];
        if let RuleMatch::Ok { bindings } = match_rule(pattern, call_text) {
            return InvocationMatch {
                bindings,
                rule_index: Some(i),
                diagnostics: Vec::new(),
            };
        }
    }

    InvocationMatch {
        bindings: Vec::new(),
        rule_index: None,
        diagnostics: vec![
            Diagnostic::error(m_code(M_NO_MATCH))
                .message("no matching rule for macro invocation")
                .with_span(call_span)
                .finish(),
        ],
    }
}

/// Match `call_text` against a single rule `pattern`.
///
/// Phase-1 matcher: simple text scanner.
///
/// - Pattern tokens are split on whitespace and `,`. The character `$`
///   begins a metavariable: `$x:kind` matches one fragment.
/// - For `$x:expr|literal|ident|pat|stmt|block|type|ty`: captures one
///   comma-delimited fragment.
/// - For repetitions `$x:expr*`: captures the entire remaining
///   comma-separated tail into one binding whose `captured` is the
///   joined text.
/// - All other pattern chars must match literally (with whitespace
///   normalisation).
///
/// Returns `Failed` on any mismatch. The matcher is intentionally
/// permissive in phase-1; PR-49 sharpens this with proper hygiene.
#[must_use]
pub fn match_rule(pattern: &str, call_text: &str) -> RuleMatch {
    let mut bindings = Vec::new();
    let mut p_iter = pattern.split(',').map(|s| s.trim()).peekable();
    let mut c_iter = call_text.split(',').map(|s| s.trim());

    while let Some(p_token) = p_iter.next() {
        if p_token.is_empty() && p_iter.peek().is_none() {
            // Trailing pattern empty; OK if call is also empty.
            return if c_iter.next().is_none_or(str::is_empty) {
                RuleMatch::Ok { bindings }
            } else {
                RuleMatch::Failed
            };
        }

        // Strip surrounding parens from pattern token if present.
        let p_token = p_token
            .strip_prefix('(')
            .unwrap_or(p_token)
            .strip_suffix(')')
            .unwrap_or(p_token)
            .trim();

        if let Some(stripped) = p_token.strip_prefix('$') {
            // Metavariable. Parse `name:kind` or `name:kind*`.
            let (name_part, rest) = stripped.split_once(':').unwrap_or((stripped, "expr"));
            let (kind_str, repetition) = if let Some(s) = rest.strip_suffix('*') {
                (s, true)
            } else {
                (rest, false)
            };

            let Some(kind) = MacroFragmentKind::parse(kind_str) else {
                return RuleMatch::Failed;
            };

            if repetition {
                // Consume everything remaining in call as a Vec.
                let remaining: Vec<&str> = c_iter.by_ref().collect();
                bindings.push(MatchBinding {
                    name: name_part.to_string(),
                    kind,
                    captured: remaining.join(", "),
                });
                // After a repetition, no more pattern tokens.
                if p_iter.peek().is_some_and(|t| !t.is_empty()) {
                    return RuleMatch::Failed;
                }
                return RuleMatch::Ok { bindings };
            }

            // Single-fragment metavariable: pull one item from call.
            let Some(captured) = c_iter.next() else {
                return RuleMatch::Failed;
            };

            // Validate kind-specific shape (very loose phase-1 check).
            if !accepts_kind(kind, captured) {
                return RuleMatch::Failed;
            }

            bindings.push(MatchBinding {
                name: name_part.to_string(),
                kind,
                captured: captured.to_string(),
            });
        } else {
            // Literal pattern token must equal the call token verbatim
            // (after normalisation).
            let Some(c_token) = c_iter.next() else {
                return RuleMatch::Failed;
            };
            if p_token != c_token {
                return RuleMatch::Failed;
            }
        }
    }

    if c_iter.next().is_some_and(|s| !s.is_empty()) {
        RuleMatch::Failed
    } else {
        RuleMatch::Ok { bindings }
    }
}

/// Phase-1 fragment-kind shape check. Very loose; PR-49 will use the
/// actual sub-parsers.
fn accepts_kind(kind: MacroFragmentKind, text: &str) -> bool {
    let text = text.trim();
    if text.is_empty() {
        return false;
    }
    match kind {
        MacroFragmentKind::Ident => text.chars().all(|c| c.is_alphanumeric() || c == '_'),
        MacroFragmentKind::Literal => {
            text.chars().all(|c| c.is_ascii_digit())
                || text.starts_with('"') && text.ends_with('"')
                || text.starts_with('\'') && text.ends_with('\'')
                || text == "true"
                || text == "false"
        }
        MacroFragmentKind::Stmt => !text.starts_with("let "),
        _ => true,
    }
}

fn m_code(n: u16) -> DiagnosticCode {
    DiagnosticCode::new(Category::M, Severity::Error, n).expect("valid M code")
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_ast::{MacroFragment, MacroRule, NodeId};
    use paideia_as_diagnostics::FileId;

    fn span() -> Span {
        Span::new(FileId::new(1).unwrap(), 0, 1)
    }

    fn placeholder_id() -> NodeId {
        NodeId::new(1).unwrap()
    }

    fn decl_with_rules(rules: Vec<&str>) -> (MacroDeclData, Vec<&str>) {
        let macro_rules = rules
            .iter()
            .map(|_| MacroRule {
                pattern: placeholder_id(),
                template: placeholder_id(),
                fragments: Vec::<MacroFragment>::new(),
            })
            .collect();
        let decl = MacroDeclData {
            name: placeholder_id(),
            rules: macro_rules,
            doc: None,
        };
        (decl, rules)
    }

    #[test]
    fn matches_simple_expr_fragment() {
        let (decl, pats) = decl_with_rules(vec!["$x:expr"]);
        let r = match_invocation(&decl, &pats, "1 + 2", span());
        assert!(r.diagnostics.is_empty());
        assert_eq!(r.rule_index, Some(0));
        assert_eq!(r.bindings.len(), 1);
        assert_eq!(r.bindings[0].name, "x");
        assert_eq!(r.bindings[0].captured, "1 + 2");
    }

    #[test]
    fn rejects_let_stmt_against_expr_fragment() {
        // `let x = 1` is a stmt, not an expr; the loose phase-1 check
        // catches the `let` prefix.
        let (decl, pats) = decl_with_rules(vec!["$x:stmt"]);
        let r = match_invocation(&decl, &pats, "let x = 1", span());
        assert!(r.rule_index.is_none());
        assert_eq!(r.diagnostics.len(), 1);
        assert_eq!(r.diagnostics[0].code().number(), 308);
    }

    #[test]
    fn matches_repetition_into_one_binding() {
        let (decl, pats) = decl_with_rules(vec!["$x:expr*"]);
        let r = match_invocation(&decl, &pats, "a, b, c", span());
        assert!(r.diagnostics.is_empty());
        assert_eq!(r.bindings.len(), 1);
        assert_eq!(r.bindings[0].captured, "a, b, c");
    }

    #[test]
    fn no_matching_rule_emits_m0308() {
        let (decl, pats) = decl_with_rules(vec!["$x:literal"]);
        let r = match_invocation(&decl, &pats, "foo()", span());
        assert!(r.rule_index.is_none());
        assert_eq!(r.diagnostics.len(), 1);
        assert_eq!(r.diagnostics[0].code().number(), 308);
        assert_eq!(r.diagnostics[0].code().category(), Category::M);
    }

    #[test]
    fn first_match_wins_across_rules() {
        let (decl, pats) = decl_with_rules(vec!["$x:literal", "$x:expr"]);
        let r = match_invocation(&decl, &pats, "42", span());
        assert_eq!(r.rule_index, Some(0));
        assert_eq!(r.bindings[0].kind, MacroFragmentKind::Literal);
    }

    #[test]
    fn ident_kind_accepts_simple_ident() {
        let (decl, pats) = decl_with_rules(vec!["$x:ident"]);
        let r = match_invocation(&decl, &pats, "hello", span());
        assert!(r.diagnostics.is_empty());
        assert_eq!(r.bindings[0].kind, MacroFragmentKind::Ident);
    }

    #[test]
    fn ident_kind_rejects_complex_expression() {
        let (decl, pats) = decl_with_rules(vec!["$x:ident"]);
        let r = match_invocation(&decl, &pats, "1 + 2", span());
        assert!(r.rule_index.is_none());
    }

    #[test]
    fn two_fragment_pattern_matches_two_args() {
        let (decl, pats) = decl_with_rules(vec!["$x:expr, $y:expr"]);
        let r = match_invocation(&decl, &pats, "a, b", span());
        assert!(r.diagnostics.is_empty());
        assert_eq!(r.bindings.len(), 2);
        assert_eq!(r.bindings[0].captured, "a");
        assert_eq!(r.bindings[1].captured, "b");
    }

    #[test]
    fn match_invocation_dispatches_across_two_rules() {
        // Rule 0: single expr
        // Rule 1: two exprs
        let (decl, pats) = decl_with_rules(vec!["$x:expr", "$x:expr, $y:expr"]);

        // Call with single arg should match rule 0
        let r = match_invocation(&decl, &pats, "3", span());
        assert!(r.diagnostics.is_empty());
        assert_eq!(r.rule_index, Some(0), "single arg should match rule 0");
        assert_eq!(r.bindings.len(), 1);
        assert_eq!(r.bindings[0].captured, "3");

        // Call with two args should match rule 1
        let r = match_invocation(&decl, &pats, "3, 4", span());
        assert!(r.diagnostics.is_empty());
        assert_eq!(r.rule_index, Some(1), "two args should match rule 1");
        assert_eq!(r.bindings.len(), 2);
        assert_eq!(r.bindings[0].captured, "3");
        assert_eq!(r.bindings[1].captured, "4");

        // Call with no args should match neither
        let r = match_invocation(&decl, &pats, "", span());
        assert!(!r.diagnostics.is_empty());
        assert_eq!(r.rule_index, None);
    }
}
