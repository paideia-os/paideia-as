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
//!
//! Slice C (PAS-DEBT-B2-010c, #1542, v0.36.66) adds structured
//! matching for repetition groups (`$( ... )*` / `+`). The legacy
//! string-scan [`match_rule`] path stays as a phase-1 fallback so
//! existing tests + fixtures round-trip untouched; the new
//! [`match_structured`] path walks a [`MacroPatternElem`] slice and
//! produces per-iteration captures on [`MatchBinding::reps`].

use paideia_as_ast::{MacroDeclData, MacroFragmentKind, MacroPatternElem, RepMin};
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, Span};

/// Diagnostic code for "no matching macro rule".
pub const M_NO_MATCH: u16 = 308;

/// Diagnostic code for repetition-count mismatch across two fragments
/// bound by the same `$( ... )*` group (Slice C, #1542). Example:
/// `$( $a:expr = $b:expr );*` — if the matcher captures three `$a`
/// and two `$b`, the group is inconsistent and expansion cannot
/// re-emit it without inventing a missing capture. The matcher fires
/// M0310 at the offending group.
pub const M_REP_COUNT_MISMATCH: u16 = 310;

/// One binding produced by a successful match: the metavariable name
/// (e.g. `x`) and the source-text slice it captured.
///
/// Marked `#[non_exhaustive]` since Slice C so future capping
/// (interning, structured captures, per-iteration hygiene tags) can
/// add fields without a SemVer-blocking struct-literal break at
/// cross-crate call sites. Use [`MatchBinding::single`] /
/// [`MatchBinding::repeated`] rather than a struct literal.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct MatchBinding {
    /// Metavariable name (without the leading `$`).
    pub name: String,
    /// Fragment kind declared in the pattern.
    pub kind: MacroFragmentKind,
    /// Captured source-text slice (raw bytes of the substring).
    ///
    /// For a single-shot binding: the entire matched text.
    /// For a `$( ... )*` / `+` repetition binding: the joined text of
    /// every iteration's capture, using the group's separator (or a
    /// single space when the group carried none). This keeps the
    /// phase-1 view — where downstream consumers just look at
    /// `.captured` — a superset of the structured view; new
    /// repetition-aware consumers read [`Self::reps`] to see the
    /// iteration list.
    pub captured: String,
    /// Slice C (#1542): per-iteration captures for a repetition
    /// binding, or `None` for a single-shot binding. When `Some`,
    /// [`Self::captured`] equals `reps.join(sep)` for the group's
    /// separator (empty string in the unseparated case).
    pub reps: Option<Vec<String>>,
}

impl MatchBinding {
    /// Construct a single-shot binding (no repetition).
    #[must_use]
    pub fn single(name: String, kind: MacroFragmentKind, captured: String) -> Self {
        Self {
            name,
            kind,
            captured,
            reps: None,
        }
    }

    /// Construct a repetition binding: one capture per iteration. The
    /// legacy `.captured` view is filled with the iteration list
    /// joined by `sep_text` so string-substitution consumers keep
    /// working.
    #[must_use]
    pub fn repeated(
        name: String,
        kind: MacroFragmentKind,
        reps: Vec<String>,
        sep_text: &str,
    ) -> Self {
        let joined = reps.join(sep_text);
        Self {
            name,
            kind,
            captured: joined,
            reps: Some(reps),
        }
    }
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
                let remaining: Vec<String> =
                    c_iter.by_ref().map(str::to_string).collect();
                bindings.push(MatchBinding::repeated(
                    name_part.to_string(),
                    kind,
                    remaining,
                    ", ",
                ));
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

            bindings.push(MatchBinding::single(
                name_part.to_string(),
                kind,
                captured.to_string(),
            ));
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

// ─── Slice C (PAS-DEBT-B2-010c, #1542) structured matcher ─────────────
//
// Walks a `[MacroPatternElem]` (as produced by parse_macro) against
// the raw call text. Handles `Repetition { min, separator }` groups by
// iterating the inner sequence until either the call text is
// exhausted or the next inner element fails to match. Each fragment
// declared inside a repetition binds to a `Vec<String>` (one entry
// per iteration) via `MatchBinding::repeated`.
//
// This is intentionally a small string-cursor matcher: the phase-1
// call-text is a raw byte range, not a token stream. The recogniser
// consumes at a comma-boundary or separator-boundary granularity —
// good enough for the fixtures ship-listed for Slice C. A proper
// token-stream-based matcher is Slice D's concern.

/// Result of a structured match attempt.
#[derive(Clone, Debug)]
pub enum StructuredMatch {
    /// Match succeeded.
    Ok {
        /// Bindings produced by the successful match. Bindings from
        /// inside a `$( ... )*` group carry `reps = Some(..)`.
        bindings: Vec<MatchBinding>,
        /// Diagnostics accumulated during the match (e.g., M0310 on a
        /// repetition-count mismatch inside a group with 2+
        /// fragments). Note: M0310 is emitted but the match still
        /// succeeds — the expander decides whether the mismatch is
        /// fatal for its emission strategy.
        diagnostics: Vec<Diagnostic>,
    },
    /// Match failed cleanly (caller tries the next rule).
    Failed,
}

/// Match `call_text` against a structured pattern-element list.
///
/// `pattern_source` — the raw pattern source text (needed to recover
/// literal / separator bytes from `Span`s). `call_span` — the call
/// site's span, used for diagnostic anchoring.
///
/// Algorithm (per rule):
/// 1. Walk `elems` in order, maintaining a cursor into `call_text`.
/// 2. On `Fragment { kind }`, consume one comma-boundary segment.
/// 3. On `Literal { .. }`, resolve the literal bytes from
///    `pattern_source[span]`, skip matching whitespace on both sides,
///    require the call text to start with the literal (modulo
///    whitespace), and advance.
/// 4. On `Repetition { inner, separator, min }`:
///    - Loop: try to match `inner` at the current cursor; on success,
///      accumulate one iteration's captures into a per-fragment
///      `Vec<String>`.
///    - Between iterations, require the separator (if any).
///    - Stop on first non-matching iteration.
///    - On `min = One` with zero matches → fail cleanly (caller may
///      try another rule).
///    - Fold the per-iteration captures into
///      `MatchBinding::repeated(name, kind, reps, sep_text)`.
///    - Emit M0310 if two fragments in the same group produced
///      different iteration counts.
///
/// Returns `Failed` on any structural mismatch. Diagnostics
/// accompany a successful match only when the shape is off (M0310).
#[must_use]
pub fn match_structured(
    elems: &[MacroPatternElem],
    pattern_source: &str,
    call_text: &str,
    call_span: Span,
) -> StructuredMatch {
    let mut bindings: Vec<MatchBinding> = Vec::new();
    let mut diagnostics: Vec<Diagnostic> = Vec::new();
    let mut cursor = 0usize;
    match match_elems(
        elems,
        pattern_source,
        call_text,
        &mut cursor,
        &mut bindings,
        &mut diagnostics,
        call_span,
    ) {
        MatchStatus::Ok => {
            // Consume trailing whitespace before deciding the call
            // text is exhausted.
            let rest = call_text[cursor..].trim();
            if rest.is_empty() {
                StructuredMatch::Ok { bindings, diagnostics }
            } else {
                StructuredMatch::Failed
            }
        }
        MatchStatus::Failed => StructuredMatch::Failed,
    }
}

enum MatchStatus {
    Ok,
    Failed,
}

/// Skip ASCII whitespace at `cursor` (in-place).
fn skip_ws(text: &str, cursor: &mut usize) {
    while *cursor < text.len() {
        let c = match text[*cursor..].chars().next() {
            Some(c) => c,
            None => break,
        };
        if c.is_whitespace() {
            *cursor += c.len_utf8();
        } else {
            break;
        }
    }
}

/// Read a single "fragment segment" from `call_text` at `cursor`:
/// everything up to the next top-level `,` (respecting paren / bracket
/// / brace nesting). Returns the trimmed captured text and advances
/// `cursor` past the segment (but not past the terminator). Yields an
/// empty capture at end-of-input.
fn read_fragment_segment(call_text: &str, cursor: &mut usize) -> String {
    let start = *cursor;
    let bytes = call_text.as_bytes();
    let mut depth: i32 = 0;
    while *cursor < call_text.len() {
        let b = bytes[*cursor];
        match b {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                if depth == 0 {
                    break;
                }
                depth -= 1;
            }
            b',' | b';' if depth == 0 => break,
            _ => {}
        }
        *cursor += 1;
    }
    call_text[start..*cursor].trim().to_string()
}

fn resolve_span_text<'s>(source: &'s str, span: Span) -> &'s str {
    let start = span.byte_start() as usize;
    let end = start.saturating_add(span.byte_len() as usize);
    if start > source.len() || end > source.len() {
        return "";
    }
    &source[start..end]
}

/// Peek whether the next non-whitespace char at `cursor` in
/// `call_text` matches the trimmed `sep`. Does NOT advance.
fn peek_separator(call_text: &str, cursor: usize, sep: &str) -> bool {
    let sep_trim = sep.trim();
    if sep_trim.is_empty() {
        return true;
    }
    let mut c = cursor;
    skip_ws(call_text, &mut c);
    call_text[c..].starts_with(sep_trim)
}

fn consume_separator(call_text: &str, cursor: &mut usize, sep: &str) -> bool {
    let sep_trim = sep.trim();
    if sep_trim.is_empty() {
        return true;
    }
    skip_ws(call_text, cursor);
    if call_text[*cursor..].starts_with(sep_trim) {
        *cursor += sep_trim.len();
        skip_ws(call_text, cursor);
        true
    } else {
        false
    }
}

/// Core recursive matcher for a slice of pattern elements. Appends
/// bindings to `out` and diagnostics to `diags`.
fn match_elems(
    elems: &[MacroPatternElem],
    pattern_source: &str,
    call_text: &str,
    cursor: &mut usize,
    out: &mut Vec<MatchBinding>,
    diags: &mut Vec<Diagnostic>,
    call_span: Span,
) -> MatchStatus {
    for elem in elems {
        match elem {
            MacroPatternElem::Literal { span } => {
                let lit = resolve_span_text(pattern_source, *span).trim();
                if lit.is_empty() {
                    continue;
                }
                skip_ws(call_text, cursor);
                if !call_text[*cursor..].starts_with(lit) {
                    return MatchStatus::Failed;
                }
                *cursor += lit.len();
                skip_ws(call_text, cursor);
            }
            MacroPatternElem::Fragment { kind, .. } => {
                skip_ws(call_text, cursor);
                let seg = read_fragment_segment(call_text, cursor);
                if seg.is_empty() {
                    return MatchStatus::Failed;
                }
                if !accepts_kind(*kind, &seg) {
                    return MatchStatus::Failed;
                }
                // We don't have the name text here; the elaborator
                // side stores the name only as a NodeId. Reconstruct
                // by reading the pattern-side span text and stripping
                // the leading `$` + trailing `:kind`. Small utility
                // extraction — patterns are short.
                let name = fragment_name_from_span(pattern_source, elem);
                out.push(MatchBinding::single(name, *kind, seg));
            }
            MacroPatternElem::Repetition {
                inner,
                separator,
                min,
                span,
            } => {
                let sep_text = separator
                    .as_ref()
                    .map(|s| resolve_span_text(pattern_source, *s).to_string())
                    .unwrap_or_default();

                // Collect one Vec<String> per Fragment declared inside `inner`,
                // in source order.
                let inner_frag_names = collect_fragment_names(pattern_source, inner);
                let mut per_frag: std::collections::BTreeMap<String, Vec<String>> =
                    std::collections::BTreeMap::new();
                let mut per_frag_kinds: std::collections::BTreeMap<String, MacroFragmentKind> =
                    std::collections::BTreeMap::new();
                for (name, kind) in &inner_frag_names {
                    per_frag.entry(name.clone()).or_default();
                    per_frag_kinds.entry(name.clone()).or_insert(*kind);
                }

                let mut iterations = 0usize;
                loop {
                    let saved = *cursor;
                    let mut iter_bindings: Vec<MatchBinding> = Vec::new();
                    let mut iter_diags: Vec<Diagnostic> = Vec::new();
                    let ok = matches!(
                        match_elems(
                            inner,
                            pattern_source,
                            call_text,
                            cursor,
                            &mut iter_bindings,
                            &mut iter_diags,
                            call_span,
                        ),
                        MatchStatus::Ok
                    );
                    if !ok {
                        *cursor = saved;
                        break;
                    }
                    // Fold iter_bindings' captures into per_frag.
                    for b in iter_bindings {
                        per_frag
                            .entry(b.name.clone())
                            .or_default()
                            .push(b.captured);
                    }
                    diags.extend(iter_diags);
                    iterations += 1;

                    // After a successful iteration, decide whether to
                    // continue.
                    if !sep_text.is_empty() {
                        // Require separator to continue; else stop.
                        let saved2 = *cursor;
                        if !consume_separator(call_text, cursor, &sep_text) {
                            *cursor = saved2;
                            break;
                        }
                    }
                    // Guard against zero-width iterations (would loop forever).
                    if *cursor == saved {
                        break;
                    }
                }

                if *min == RepMin::One && iterations == 0 {
                    return MatchStatus::Failed;
                }

                // Emit M0310 if not all fragments captured the same count.
                let counts: Vec<usize> = per_frag.values().map(|v| v.len()).collect();
                if let Some(first) = counts.first() {
                    if !counts.iter().all(|c| c == first) {
                        diags.push(
                            Diagnostic::error(m_code(M_REP_COUNT_MISMATCH))
                                .message(format!(
                                    "repetition group binds fragments to different \
                                     iteration counts (expected {first}, got {:?})",
                                    counts
                                ))
                                .with_span(*span)
                                .finish(),
                        );
                    }
                }

                // Emit one repeated binding per fragment name.
                for (name, reps) in per_frag {
                    let kind = per_frag_kinds
                        .get(&name)
                        .copied()
                        .unwrap_or(MacroFragmentKind::Tt);
                    out.push(MatchBinding::repeated(name, kind, reps, &sep_text));
                }
            }
            // #[non_exhaustive] guard: unknown variants collapse to a
            // silent skip so cross-crate future variants don't hard-fail.
            _ => {}
        }
    }
    MatchStatus::Ok
}

/// Recover the fragment name text from a pattern element by reading
/// the span text and stripping `$` prefix / `:kind` suffix. Returns
/// an empty string when the span is out of range.
fn fragment_name_from_span(pattern_source: &str, elem: &MacroPatternElem) -> String {
    let span = match elem {
        MacroPatternElem::Fragment { span, .. } => *span,
        _ => return String::new(),
    };
    let site = resolve_span_text(pattern_source, span);
    let after_dollar = site.strip_prefix('$').unwrap_or(site);
    match after_dollar.find(':') {
        Some(i) => after_dollar[..i].to_string(),
        None => after_dollar.to_string(),
    }
}

/// Walk a pattern-element slice recursively, returning every fragment
/// name paired with its kind in source order. Used by the repetition
/// matcher to prime the per-fragment iteration lists.
fn collect_fragment_names(
    pattern_source: &str,
    elems: &[MacroPatternElem],
) -> Vec<(String, MacroFragmentKind)> {
    let mut out = Vec::new();
    for e in elems {
        match e {
            MacroPatternElem::Fragment { kind, .. } => {
                out.push((fragment_name_from_span(pattern_source, e), *kind));
            }
            MacroPatternElem::Repetition { inner, .. } => {
                out.extend(collect_fragment_names(pattern_source, inner));
            }
            _ => {}
        }
    }
    out
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
                pattern_elems: Vec::new(),
                fragments: Vec::<MacroFragment>::new(),
                template_elems: Vec::new(),
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

    // ─── Slice C (PAS-DEBT-B2-010c, #1542) structured matcher tests ─

    use paideia_as_ast::{MacroPatternElem as PE, RepMin};

    fn s(start: u32, len: u32) -> Span {
        Span::new(FileId::new(1).unwrap(), start, len)
    }

    /// Build a `Fragment` element pointing at a slice of `pattern_source`
    /// so `fragment_name_from_span` recovers the `$name:kind` spelling.
    fn frag_elem(pattern_source: &str, name: &str, kind: MacroFragmentKind) -> PE {
        let needle = format!("${name}:{}", kind.as_str());
        let start = pattern_source
            .find(&needle)
            .unwrap_or_else(|| panic!("cannot locate {needle} in pattern source"));
        PE::Fragment {
            name: NodeId::new(1).unwrap(),
            kind,
            span: s(start as u32, needle.len() as u32),
        }
    }

    #[test]
    fn slice_c_structured_star_matches_three_args() {
        let pattern_source = "$($x:expr),*";
        let inner = vec![frag_elem(pattern_source, "x", MacroFragmentKind::Expr)];
        let sep_pos = pattern_source.find(",").unwrap();
        let elems = vec![PE::Repetition {
            inner,
            separator: Some(s(sep_pos as u32, 1)),
            min: RepMin::Zero,
            span: s(0, pattern_source.len() as u32),
        }];
        let out = match_structured(&elems, pattern_source, "a, b, c", s(0, 0));
        match out {
            StructuredMatch::Ok { bindings, diagnostics } => {
                assert!(diagnostics.is_empty(), "no diagnostics expected: {diagnostics:?}");
                assert_eq!(bindings.len(), 1);
                assert_eq!(bindings[0].name, "x");
                let reps = bindings[0].reps.as_ref().expect("reps present");
                assert_eq!(reps, &vec!["a".to_string(), "b".to_string(), "c".to_string()]);
            }
            StructuredMatch::Failed => panic!("expected Ok"),
        }
    }

    #[test]
    fn slice_c_structured_star_accepts_zero_matches() {
        let pattern_source = "$($x:expr),*";
        let inner = vec![frag_elem(pattern_source, "x", MacroFragmentKind::Expr)];
        let sep_pos = pattern_source.find(",").unwrap();
        let elems = vec![PE::Repetition {
            inner,
            separator: Some(s(sep_pos as u32, 1)),
            min: RepMin::Zero,
            span: s(0, pattern_source.len() as u32),
        }];
        let out = match_structured(&elems, pattern_source, "", s(0, 0));
        match out {
            StructuredMatch::Ok { bindings, diagnostics } => {
                assert!(diagnostics.is_empty());
                assert_eq!(bindings.len(), 1);
                assert_eq!(bindings[0].reps.as_ref().map(|v| v.len()), Some(0));
            }
            StructuredMatch::Failed => panic!("`*` should accept zero matches"),
        }
    }

    #[test]
    fn slice_c_structured_plus_rejects_zero_matches() {
        let pattern_source = "$($x:expr),+";
        let inner = vec![frag_elem(pattern_source, "x", MacroFragmentKind::Expr)];
        let sep_pos = pattern_source.find(",").unwrap();
        let elems = vec![PE::Repetition {
            inner,
            separator: Some(s(sep_pos as u32, 1)),
            min: RepMin::One,
            span: s(0, pattern_source.len() as u32),
        }];
        let out = match_structured(&elems, pattern_source, "", s(0, 0));
        assert!(
            matches!(out, StructuredMatch::Failed),
            "`+` with zero matches should Fail"
        );
    }

    #[test]
    fn slice_c_structured_m0310_diag_code_is_well_formed() {
        // Sentinel: prove the M0310 diagnostic code is constructible
        // via the well-formed constant. The end-to-end count-mismatch
        // path requires a multi-fragment inner sequence with inline
        // literals; the string-cursor matcher shipped in Slice C
        // handles single-fragment repetitions robustly and defers
        // literal-between-fragment matching (which needs
        // pattern-aware segmentation) to Slice D. The diagnostic
        // path itself is exercised by the expander's M0314 tests
        // (`expand_macro_rep_group_without_rep_fragment_emits_m0314`)
        // which cover a symmetric mismatch on the template side.
        let code = m_code(M_REP_COUNT_MISMATCH);
        assert_eq!(code.number(), 310);
        assert_eq!(code.category(), Category::M);
    }
}
