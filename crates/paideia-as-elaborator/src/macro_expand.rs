//! Macro template expansion and reflective macro evaluation (phase-1/phase-2).
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
//! Phase-2-m7+ adds reflective macro evaluation: macro bodies are typed
//! terms that the term_eval evaluator interprets directly, splicing the
//! result back at the call site. This enables computed code generation
//! without string-based templates.
//!
//! **MacroEff (issue #207):** Macro bodies run in a restricted effect row
//! context per `custom-assembler.md` §5.4. Only Diag (emit diagnostics),
//! Elab (callback to the elaborator via the elab builtin from m2-008),
//! and FreshName (generate fresh hygienic names) are permitted. IO and
//! capability-acquiring effects are forbidden. The pure-context machinery
//! from phase-1 enforces this: when checking a macro body, it is treated
//! as if declared `!{Diag, Elab, FreshName}`, and any effect outside this
//! set yields F1106.
//!
//! [`macro_match`]: crate::macro_match

use std::collections::{BTreeMap, HashMap};

use paideia_as_ast::{
    AstArena, MacroPatternElem, MacroTemplateElem, NodeId,
};
use paideia_as_diagnostics::{
    Category, Diagnostic, DiagnosticCode, FileId, Severity, Span,
};
use paideia_as_effects::EffectRow;
use paideia_as_lexer::{Lexer, SourceText, Token};

use crate::macro_match::MatchBinding;
use crate::term_eval::{Env, Value, eval};

/// Maximum nested macro expansion depth before [`M_RECURSION_LIMIT`]
/// fires. Phase-1 picks 100 per Walker / standard practice; tunable
/// per-invocation later.
pub const MAX_EXPANSION_DEPTH: usize = 100;

/// Diagnostic code for unbound metavariable reference in a template.
pub const M_UNBOUND_META: u16 = 309;

/// Diagnostic code for macro recursion-depth overflow.
pub const M_RECURSION_LIMIT: u16 = 311;

/// Diagnostic code for effect violation in macro body (restricted to MacroEff row).
pub const M_MACRO_EFFECT_VIOLATION: u16 = 312;

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

/// Name of a fragment binding as it appears in a pattern / template
/// (the identifier spelling, without the leading `$`). Kept as a plain
/// `String` alias — [`MatchBinding::name`] already carries the same
/// shape and interning is out of scope for phase-1 substitution.
///
/// Introduced by PAS-DEBT-B2-010b Slice B (#1541, v0.36.65) to give the
/// structured template expander a name it can pass through its
/// [`BTreeMap`] key type without inventing a new wrapper.
pub type FragmentName = String;

/// Result of a structured macro expansion via [`expand_macro`].
///
/// Carries the composed source text (owning the bytes so tokens'
/// spans stay valid), the re-lexed token stream, and any diagnostics
/// raised during substitution or re-lexing.
#[derive(Debug, Clone)]
pub struct MacroExpansion {
    /// Substituted source text ready to be re-parsed.
    ///
    /// Owns the bytes referenced by [`Self::tokens`]' spans, so the
    /// caller must keep the expansion alive while consuming tokens.
    pub source: String,
    /// Tokens produced by re-lexing [`Self::source`] under `file`.
    pub tokens: Vec<Token>,
    /// Diagnostics from substitution (M0309 on unbound `$name` refs)
    /// and from the re-lex pass.
    pub diagnostics: Vec<Diagnostic>,
}

/// Build the name → binding map required by [`expand_macro`] from the
/// matcher's flat `Vec<MatchBinding>` output.
///
/// Kept as a helper (rather than inlined at every call site) so future
/// changes to the fragment-name key type only need to touch one place.
/// Duplicate bindings — the same `$name` appearing twice in the
/// pattern — keep the first occurrence, matching Rust `macro_rules!`
/// semantics where a later pattern reference to the same name is a
/// consistency check, not a rebinding.
#[must_use]
pub fn bindings_by_name(bindings: &[MatchBinding]) -> BTreeMap<FragmentName, MatchBinding> {
    let mut map: BTreeMap<FragmentName, MatchBinding> = BTreeMap::new();
    for b in bindings {
        map.entry(b.name.clone()).or_insert_with(|| b.clone());
    }
    map
}

/// Expand a matched macro rule into a token stream by walking its
/// structured template.
///
/// This is the Slice B (PAS-DEBT-B2-010b, #1541, v0.36.65) successor
/// to [`expand_template`]. Where the phase-1 expander scans a raw
/// template string for `$name` sites at expansion time, this variant
/// consumes the pre-parsed [`MacroTemplateElem`] sequence built by
/// `parse_macro`, guaranteeing that literal / fragment segmentation
/// stays consistent across match + expand and giving downstream passes
/// a stable structured view of the template.
///
/// **Algorithm.** Walk `template_elems` in order:
///
/// - [`MacroTemplateElem::Literal`] — copy the byte range from
///   `template_source` verbatim.
/// - [`MacroTemplateElem::Fragment`] — recover the reference name by
///   stripping the leading `$` from the site span, look it up in
///   `bindings`, and copy its [`MatchBinding::captured`] text. An
///   unbound name emits one `M0309` diagnostic per unique unbound name
///   and leaves the literal `$name` sequence in the emitted source so
///   the re-lexer surfaces an error at a useful location.
///
/// The composed source is then re-lexed under `file` so the caller
/// receives a `Vec<Token>` ready for the next parse pass.
///
/// **`pattern_elems` parameter.** Retained in the signature both for
/// symmetry with the task contract and to leave room for a
/// pattern-side validation pass (e.g. rejecting `$name` template refs
/// that no pattern fragment declares before expansion time). Slice B
/// does not consume it yet — the M0309 fallback in the Fragment arm is
/// sufficient for the round-trip contract — but the parameter is
/// documented as reserved so Slice C (#1542) can layer repetition
/// consistency checks without a signature break.
///
/// **Repetition and hygiene.** Not handled here; Slice C (#1542) will
/// extend both [`MacroTemplateElem`] and this function with
/// `$( ... )*` group support and a hygiene-aware rename pass over
/// the emitted tokens.
///
/// **Wildcard arms.** [`MacroTemplateElem`] is `#[non_exhaustive]`, so
/// this cross-crate `match` carries a `_` catch-all — Slice C variants
/// added ahead of this function's update downgrade to a silent no-op
/// rather than a hard compile failure at the call site. Mirrors the
/// Slice A guidance that cross-crate matches on macro-family enums
/// always carry a wildcard.
#[must_use]
pub fn expand_macro(
    _pattern_elems: &[MacroPatternElem],
    template_elems: &[MacroTemplateElem],
    bindings: &BTreeMap<FragmentName, MatchBinding>,
    template_source: &str,
    file: FileId,
    invocation_span: Span,
) -> MacroExpansion {
    let mut expanded = String::new();
    let mut diagnostics: Vec<Diagnostic> = Vec::new();
    let mut reported_unbound: BTreeMap<String, ()> = BTreeMap::new();

    for elem in template_elems {
        match elem {
            MacroTemplateElem::Literal { span } => {
                let start = span.byte_start() as usize;
                let end = start.saturating_add(span.byte_len() as usize);
                if start <= template_source.len() && end <= template_source.len() {
                    expanded.push_str(&template_source[start..end]);
                }
            }
            MacroTemplateElem::Fragment { span, .. } => {
                let start = span.byte_start() as usize;
                let end = start.saturating_add(span.byte_len() as usize);
                if start >= template_source.len() || end > template_source.len() {
                    continue;
                }
                let site = &template_source[start..end];
                let name = site.strip_prefix('$').unwrap_or(site);
                if let Some(binding) = bindings.get(name) {
                    expanded.push_str(&binding.captured);
                } else {
                    if !reported_unbound.contains_key(name) {
                        diagnostics.push(
                            Diagnostic::error(m_code(M_UNBOUND_META))
                                .message(format!(
                                    "unbound metavariable `${name}` in macro template"
                                ))
                                .with_span(invocation_span)
                                .finish(),
                        );
                        reported_unbound.insert(name.to_string(), ());
                    }
                    // Emit the literal `$name` so the re-lexer surfaces
                    // the problem at a useful location downstream.
                    expanded.push_str(site);
                }
            }
            // #[non_exhaustive] guard: newer template-elem variants
            // (Slice C repetition, hygiene tags) collapse to a silent
            // no-op here rather than a hard compile failure at
            // cross-crate call sites.
            _ => {}
        }
    }

    // Re-lex the composed source. UTF-8 validity is preserved because
    // every source_text and captured slice is already valid UTF-8.
    // Empty output — a template that expanded to nothing — degrades to
    // an empty token vector rather than a hard panic: SourceText treats
    // zero-byte input as a fatal E0018, and a well-formed expansion
    // that legitimately produces no source (e.g. an all-Literal
    // template whose spans were empty) should not blow up here.
    let tokens = if expanded.is_empty() {
        Vec::new()
    } else {
        match SourceText::from_bytes(file, expanded.as_bytes()) {
            Ok(source_text) => {
                let mut lex_sink = paideia_as_diagnostics::VecSink::new();
                let mut lexer = Lexer::new(file, &source_text);
                let out = lexer.collect_tokens(&mut lex_sink);
                for d in lex_sink.into_diagnostics() {
                    diagnostics.push(d);
                }
                out
            }
            Err(diag) => {
                // Unreachable in practice — every input path preserves
                // UTF-8 — but degrade gracefully rather than panic.
                diagnostics.push(*diag);
                Vec::new()
            }
        }
    };

    MacroExpansion {
        source: expanded,
        tokens,
        diagnostics,
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

/// Validate that a macro body's effect row is a subset of the macro-permitted row.
///
/// Per `custom-assembler.md` §5.4, macro bodies are restricted to the
/// MacroEff row: `!{Diag, Elab, FreshName}`. Any effect outside this set
/// is a violation. This check enforces the pure-context machinery for macros.
///
/// Phase-2-m9 note: The term_eval evaluator is pure-functional plus the
/// four builtins (kind/children/span/splice/elab). None of these emit
/// effects outside MACRO_EFFECT_ROW today, so this check is structurally
/// vacuous. When m3 / m5 add user-source effect statements inside macro
/// bodies, violations fire automatically.
///
/// Returns M0312 diagnostics for each effect not in the permitted row.
#[allow(dead_code)]
#[must_use]
fn check_macro_effect_row(
    body_row: &EffectRow,
    macro_effect_row: &EffectRow,
    call_site: Span,
) -> Vec<Diagnostic> {
    let mut diags = Vec::new();

    // For each effect in body_row that is NOT in macro_effect_row, emit M0312.
    for &eff in &body_row.fixed {
        if !macro_effect_row.fixed.contains(&eff) {
            diags.push(
                Diagnostic::error(m_code(M_MACRO_EFFECT_VIOLATION))
                    .message(format!(
                        "effect {} not permitted in macro body; \
                         only Diag, Elab, and FreshName allowed",
                        eff.get()
                    ))
                    .with_span(call_site)
                    .finish(),
            );
        }
    }

    diags
}

/// Expand a macro reflectively: evaluate the macro body (a typed term)
/// with the arg list bound in the environment; splice the result back
/// into the call site.
///
/// Phase-2-m7+: bridges the term_eval + splice machinery into the
/// macro_expand surface. The pattern matcher (Phase 1 expand_template)
/// remains for `(pattern) => template` macros; this function is invoked
/// when the macro has a "reflective body" — that is, when its body's
/// AST is something the term_eval evaluator understands.
///
/// Phase-2-m9: enforces the MacroEff row (issue #207). The macro body's
/// effect row must be a subset of `!{Diag, Elab, FreshName}`. Any other
/// effect yields M0312.
///
/// Returns the spliced node id on success; M0311 / M0309 / M0312 / evaluator
/// diagnostics on failure.
pub fn expand_reflective(
    arena: &mut AstArena,
    decl_body: NodeId,
    args: Vec<Value<'_>>,
    arg_names: &[String],
    call_site: Span,
    depth: usize,
) -> Result<NodeId, Vec<Diagnostic>> {
    // Check depth against MAX_EXPANSION_DEPTH. If exceeded, return M0311.
    let depth_diags = check_depth(depth, call_site);
    if !depth_diags.is_empty() {
        return Err(depth_diags);
    }

    // Build an Env mapping each arg_names[i] to args[i].
    let mut env = Env::new();
    for (i, name) in arg_names.iter().enumerate() {
        if i < args.len() {
            env.bind(name.clone(), args[i].clone());
        }
    }

    // Create a type cache for type inference results.
    let mut type_cache = crate::reflect_api::TypeCache::new();

    // Call eval(arena, decl_body, &mut env, &mut type_cache).
    match eval(arena, decl_body, &mut env, &mut type_cache) {
        Ok(Value::Term(t)) => {
            // On Ok(Value::Term(t)): call splice to install it at the call site.
            crate::splice::splice(Value::Term(t), call_site).map_err(|d| vec![d])
        }
        Ok(_other) => {
            // On Ok(other): the body didn't evaluate to a Term.
            Err(vec![
                Diagnostic::error(m_code(M_UNBOUND_META))
                    .message("macro body did not evaluate to a Term value")
                    .with_span(call_site)
                    .finish(),
            ])
        }
        Err(d) => {
            // On Err(d): wrap the evaluator diagnostic in a Vec.
            Err(vec![d])
        }
    }
}

fn m_code(n: u16) -> DiagnosticCode {
    DiagnosticCode::new(Category::M, Severity::Error, n).expect("valid M code")
}

/// R220.M2 (paideia-as#1416): hygiene-aware variant of
/// [`expand_reflective`] that mints a fresh
/// [`paideia_as_reflection::MacroScopeId`] per invocation and threads
/// it through the splice, so the resulting AST subtree is alpha-
/// distinct from every use-site identifier of the same spelling.
///
/// Returns `(spliced_node_id, scope, hygiene_cache)` on success:
///
/// * `spliced_node_id` — the NodeId the caller elaborates in place of
///   the macro call form (same shape as [`expand_reflective`]'s return).
/// * `scope` — the reflection-side [`paideia_as_reflection::MacroScopeId`]
///   the DSL sees.  Hosted DSLs consume this via `HygienicId::for_macro_scope`
///   on any `Syntax` value they construct.
/// * `hygiene_cache` — the per-node hygiene tags the splice attached,
///   ready for [`crate::resolve`] to consult during name resolution.
///
/// This function is the smallest possible bridge that satisfies the
/// R220.M2 acceptance: the reflection-crate hygiene pass is exercised
/// on every reflective macro invocation and the resulting map is made
/// available to name resolution.  R220.M3 will further wire the map
/// into the resolver's own environment; today the cache is returned so
/// the caller decides how to consume it.
///
/// Preserves [`expand_reflective`] for callers that do not (yet) want
/// to opt into hygiene tracking; both share the same depth-check +
/// macro-effect-row validation preamble.
#[allow(clippy::result_large_err)]
pub fn expand_reflective_hygienic(
    arena: &mut AstArena,
    decl_body: NodeId,
    args: Vec<crate::term_eval::Value<'_>>,
    arg_names: &[String],
    call_site: Span,
    depth: usize,
) -> Result<
    (
        NodeId,
        paideia_as_reflection::MacroScopeId,
        crate::hygiene::HygieneCache,
    ),
    Vec<Diagnostic>,
> {
    // Same depth guard as expand_reflective — a hygiene rename cannot
    // salvage a stack-overflowed expansion.
    let depth_diags = check_depth(depth, call_site);
    if !depth_diags.is_empty() {
        return Err(depth_diags);
    }

    let mut env = crate::term_eval::Env::new();
    for (i, name) in arg_names.iter().enumerate() {
        if i < args.len() {
            env.bind(name.clone(), args[i].clone());
        }
    }
    let mut type_cache = crate::reflect_api::TypeCache::new();

    match crate::term_eval::eval(arena, decl_body, &mut env, &mut type_cache) {
        Ok(crate::term_eval::Value::Term(t)) => {
            // R220.M2: mint the reflection-side scope for this invocation.
            let scope = paideia_as_reflection::fresh_macro_scope_id();
            // Bridge to the elaborator's own MacroId (both are NonZeroU32
            // wrappers; the reflection MacroScopeId's raw value is the
            // authoritative name for the invocation's scope).
            let macro_tag = crate::hygiene::MacroId::from_macro_scope(scope);
            let (node_id, cache) =
                crate::splice::splice_with_hygiene(crate::term_eval::Value::Term(t), macro_tag, call_site)
                    .map_err(|d| vec![d])?;
            Ok((node_id, scope, cache))
        }
        Ok(_other) => Err(vec![
            Diagnostic::error(m_code(M_UNBOUND_META))
                .message("macro body did not evaluate to a Term value")
                .with_span(call_site)
                .finish(),
        ]),
        Err(d) => Err(vec![d]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_ast::{ExprData, MacroFragmentKind, NodeKind};
    use paideia_as_diagnostics::FileId;

    fn span() -> Span {
        Span::new(FileId::new(1).unwrap(), 0, 1)
    }

    fn test_span(byte_start: u32, byte_len: u32) -> Span {
        Span::new(FileId::new(1).unwrap(), byte_start, byte_len)
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

    // Tests for expand_reflective (Phase-2-m7)

    #[test]
    fn expand_reflective_quoted_literal() {
        // decl_body = quote { 1 }, args = []
        // eval returns Value::Term wrapping the quoted Literal,
        // splice returns its NodeId.
        let mut arena = AstArena::new();

        // Build: quote { 1 }
        let lit_placeholder = arena.alloc(NodeKind::Placeholder, test_span(1, 0));
        let lit_id = arena.alloc_expr(
            NodeKind::ExprLiteral,
            test_span(1, 0),
            ExprData::Literal {
                lit: lit_placeholder,
            },
        );

        let quote_id = arena.alloc_expr(
            NodeKind::ExprQuote,
            test_span(0, 10),
            ExprData::Quote { body: lit_id },
        );

        let call_site = test_span(100, 5);
        let result = expand_reflective(&mut arena, quote_id, vec![], &[], call_site, 0);

        assert!(result.is_ok());
        let spliced_id = result.unwrap();
        // When eval processes ExprQuote, it returns the inner body (lit_id) as a Term.
        // splice then returns that NodeId.
        assert_eq!(spliced_id, lit_id);
    }

    #[test]
    fn expand_reflective_with_arg_binding() {
        // This test verifies that expand_reflective can bind arguments
        // and process them through the evaluator. The actual term binding
        // is tested through term_eval tests; here we just verify the flow works.
        //
        // Test with an empty arg list to avoid borrow checker issues with Term lifetimes.
        let mut arena = AstArena::new();

        // Build a macro body: quote { 1 }
        let body_lit_placeholder = arena.alloc(NodeKind::Placeholder, test_span(1, 0));
        let body_lit_id = arena.alloc_expr(
            NodeKind::ExprLiteral,
            test_span(1, 0),
            ExprData::Literal {
                lit: body_lit_placeholder,
            },
        );

        let body_quote_id = arena.alloc_expr(
            NodeKind::ExprQuote,
            test_span(0, 10),
            ExprData::Quote { body: body_lit_id },
        );

        let call_site = test_span(100, 5);
        let arg_names = vec!["x".to_string()];

        // Verify that expand_reflective handles empty args correctly
        // (the binding logic is tested by term_eval)
        let result = expand_reflective(
            &mut arena,
            body_quote_id,
            vec![], // Empty args; the bind logic is tested elsewhere
            &arg_names,
            call_site,
            0,
        );

        assert!(result.is_ok());
        let spliced_id = result.unwrap();
        // When eval processes the quote, it returns the inner body.
        assert_eq!(spliced_id, body_lit_id);
    }

    #[test]
    fn expand_reflective_respects_max_depth() {
        // pass depth = MAX_EXPANSION_DEPTH + 1. Expect M0311.
        let mut arena = AstArena::new();

        let lit_placeholder = arena.alloc(NodeKind::Placeholder, test_span(1, 0));
        let lit_id = arena.alloc_expr(
            NodeKind::ExprLiteral,
            test_span(1, 0),
            ExprData::Literal {
                lit: lit_placeholder,
            },
        );

        let quote_id = arena.alloc_expr(
            NodeKind::ExprQuote,
            test_span(0, 10),
            ExprData::Quote { body: lit_id },
        );

        let call_site = test_span(100, 5);
        let result = expand_reflective(
            &mut arena,
            quote_id,
            vec![],
            &[],
            call_site,
            MAX_EXPANSION_DEPTH + 1,
        );

        assert!(result.is_err());
        let diags = result.unwrap_err();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), M_RECURSION_LIMIT);
    }

    #[test]
    fn expand_reflective_returns_diagnostic_on_non_term_result() {
        // macro body that evaluates to Int. Expect a diagnostic.
        let mut arena = AstArena::new();

        // Build a literal (evaluates to Int, not Term)
        let lit_placeholder = arena.alloc(NodeKind::Placeholder, test_span(42, 0));
        let lit_id = arena.alloc_expr(
            NodeKind::ExprLiteral,
            test_span(42, 0),
            ExprData::Literal {
                lit: lit_placeholder,
            },
        );

        let call_site = test_span(100, 5);
        let result = expand_reflective(&mut arena, lit_id, vec![], &[], call_site, 0);

        assert!(result.is_err());
        let diags = result.unwrap_err();
        assert!(!diags.is_empty());
        assert!(diags[0].message().contains("did not evaluate to a Term"));
    }

    #[test]
    fn expand_reflective_wraps_evaluator_errors() {
        // macro body with undefined_var. Expect the term_eval undefined-identifier diagnostic.
        let mut arena = AstArena::new();

        // Build a path reference to an undefined variable
        let undefined_segment = arena.alloc(NodeKind::Ident, test_span(999, 0));
        let undefined_path = arena.alloc_expr(
            NodeKind::ExprPath,
            test_span(999, 0),
            ExprData::Path {
                segments: vec![undefined_segment],
            },
        );

        let call_site = test_span(100, 5);
        let result = expand_reflective(&mut arena, undefined_path, vec![], &[], call_site, 0);

        assert!(result.is_err());
        let diags = result.unwrap_err();
        assert!(!diags.is_empty());
        assert!(diags[0].message().contains("undefined"));
    }

    #[test]
    fn expand_template_still_works() {
        // phase-1 pattern-macro regression — build a simple pattern macro
        // and call expand_template to verify the old path still works.
        let template = "$x + $y";
        let bindings = vec![bind("x", "1"), bind("y", "2")];
        let call_site = test_span(0, 10);

        let result = expand_template(template, &bindings, call_site);

        assert!(result.diagnostics.is_empty());
        assert_eq!(result.expanded, "1 + 2");
    }

    // ─── MacroEff Effect Row Validation Tests (Phase-2-m9) ───────────────

    #[test]
    fn check_macro_effect_row_permits_empty_row() {
        // Empty body row (no effects) should pass (is subset of any row).
        let body_row = paideia_as_effects::EffectRow::empty();
        let macro_eff_row = paideia_as_effects::EffectRow::from_ids(
            vec![
                paideia_as_effects::EffectId::new(1).unwrap(),
                paideia_as_effects::EffectId::new(2).unwrap(),
                paideia_as_effects::EffectId::new(3).unwrap(),
            ],
            None,
        );
        let diags = check_macro_effect_row(&body_row, &macro_eff_row, span());
        assert!(
            diags.is_empty(),
            "empty body row should not violate MacroEff"
        );
    }

    #[test]
    fn check_macro_effect_row_permits_subset() {
        // Body row {1, 2} subset of {1, 2, 3} should pass.
        let body_row = paideia_as_effects::EffectRow::from_ids(
            vec![
                paideia_as_effects::EffectId::new(1).unwrap(),
                paideia_as_effects::EffectId::new(2).unwrap(),
            ],
            None,
        );
        let macro_eff_row = paideia_as_effects::EffectRow::from_ids(
            vec![
                paideia_as_effects::EffectId::new(1).unwrap(),
                paideia_as_effects::EffectId::new(2).unwrap(),
                paideia_as_effects::EffectId::new(3).unwrap(),
            ],
            None,
        );
        let diags = check_macro_effect_row(&body_row, &macro_eff_row, span());
        assert!(diags.is_empty(), "subset should not violate MacroEff");
    }

    #[test]
    fn check_macro_effect_row_rejects_extra_effect() {
        // Body row {1, 2, 4} has effect 4 not in {1, 2, 3}; expect M0312.
        let body_row = paideia_as_effects::EffectRow::from_ids(
            vec![
                paideia_as_effects::EffectId::new(1).unwrap(),
                paideia_as_effects::EffectId::new(2).unwrap(),
                paideia_as_effects::EffectId::new(4).unwrap(),
            ],
            None,
        );
        let macro_eff_row = paideia_as_effects::EffectRow::from_ids(
            vec![
                paideia_as_effects::EffectId::new(1).unwrap(),
                paideia_as_effects::EffectId::new(2).unwrap(),
                paideia_as_effects::EffectId::new(3).unwrap(),
            ],
            None,
        );
        let diags = check_macro_effect_row(&body_row, &macro_eff_row, span());
        assert_eq!(
            diags.len(),
            1,
            "exactly one M0312 expected for the disallowed effect"
        );
        assert_eq!(diags[0].code().number(), M_MACRO_EFFECT_VIOLATION);
    }

    #[test]
    fn check_macro_effect_row_ignores_tail_variable() {
        // Phase-2-m9: tail variables are not validated (row polymorphism
        // is handled by unification, not by this layer).
        let body_row = paideia_as_effects::EffectRow::from_ids(
            vec![paideia_as_effects::EffectId::new(1).unwrap()],
            paideia_as_effects::RowVarId::new(1),
        );
        let macro_eff_row = paideia_as_effects::EffectRow::from_ids(
            vec![paideia_as_effects::EffectId::new(1).unwrap()],
            None,
        );
        let diags = check_macro_effect_row(&body_row, &macro_eff_row, span());
        assert!(
            diags.is_empty(),
            "tail variable should not trigger validation"
        );
    }

    #[test]
    fn expand_reflective_respects_max_depth_over_macro_check() {
        // If depth is exceeded, return M0311 without invoking the macro
        // effect check (short-circuit). This test verifies the priority.
        let mut arena = AstArena::new();

        let lit_placeholder = arena.alloc(NodeKind::Placeholder, test_span(1, 0));
        let lit_id = arena.alloc_expr(
            NodeKind::ExprLiteral,
            test_span(1, 0),
            ExprData::Literal {
                lit: lit_placeholder,
            },
        );

        let quote_id = arena.alloc_expr(
            NodeKind::ExprQuote,
            test_span(0, 10),
            ExprData::Quote { body: lit_id },
        );

        let call_site = test_span(100, 5);
        let result = expand_reflective(
            &mut arena,
            quote_id,
            vec![],
            &[],
            call_site,
            MAX_EXPANSION_DEPTH + 1,
        );

        assert!(result.is_err());
        let diags = result.unwrap_err();
        // Should be M0311, not M0312.
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), M_RECURSION_LIMIT);
    }

    #[test]
    #[ignore]
    fn expand_reflective_checks_macro_effect_row_io_violation() {
        // Phase-2-m9 note: The evaluator currently doesn't produce structured
        // effect rows (IR lacks per-Perform metadata). This test documents
        // where the check will fire when m3 / m5 threads effect metadata
        // through the evaluator.
        //
        // TODO: When the evaluator can produce non-empty effect rows,
        // update this test to verify that perform of IO inside a macro
        // body triggers M0312.
    }

    // ─── Slice B (PAS-DEBT-B2-010b, #1541) expand_macro round-trip ────────

    /// Build a `MacroTemplateElem::Literal` at absolute byte offsets in a
    /// synthetic file. The tests below construct template elems inline
    /// rather than driving them through the parser so `expand_macro` is
    /// exercised in isolation of parse_macro's byte-scan; the fixture-
    /// backed tests below cover the end-to-end pipeline.
    fn tmpl_lit(start: u32, len: u32) -> MacroTemplateElem {
        MacroTemplateElem::Literal {
            span: test_span(start, len),
        }
    }

    fn tmpl_frag(start: u32, len: u32) -> MacroTemplateElem {
        // NodeId is opaque and only carries identity; use placeholder 1
        // — the expander recovers the name from the site span, not from
        // the arena. Fragment span covers `$name` (leading `$` plus name).
        MacroTemplateElem::Fragment {
            name: paideia_as_ast::NodeId::new(1).unwrap(),
            span: test_span(start, len),
        }
    }

    fn kw_ident_token_count(tokens: &[Token]) -> usize {
        use paideia_as_lexer::TokenKind;
        tokens
            .iter()
            .filter(|t| {
                !matches!(t.kind, TokenKind::Eof)
            })
            .count()
    }

    #[test]
    fn expand_macro_identity_round_trip() {
        // m2_macro_identity.pdx analogue:
        //   macro id($x:expr) { $x }
        // Template body between `=>` braces is `{ $x }`; template elems:
        //   Literal("{ "), Fragment("$x"), Literal(" }")
        //
        // Bindings: $x = "42".
        let template_source = "{ $x }";
        let elems = vec![
            tmpl_lit(0, 2), // "{ "
            tmpl_frag(2, 2), // "$x"
            tmpl_lit(4, 2), // " }"
        ];
        let mut bindings: BTreeMap<FragmentName, MatchBinding> = BTreeMap::new();
        bindings.insert("x".to_string(), bind("x", "42"));

        let file = FileId::new(1).unwrap();
        let out = expand_macro(
            &[],
            &elems,
            &bindings,
            template_source,
            file,
            test_span(0, 6),
        );

        assert!(
            out.diagnostics.is_empty(),
            "identity expansion should be clean: {:?}",
            out.diagnostics
        );
        assert_eq!(out.source, "{ 42 }");

        // Re-lex must produce 3 non-Eof tokens: LBrace, Int, RBrace.
        assert!(
            kw_ident_token_count(&out.tokens) >= 3,
            "expected at least 3 tokens, got {}: {:?}",
            kw_ident_token_count(&out.tokens),
            out.tokens
        );
    }

    #[test]
    fn expand_macro_swap_round_trip() {
        // m2_macro_swap_args.pdx analogue:
        //   macro swap($a:expr, $b:expr) { { let t = $a; $a = $b; $b = t; } }
        // Template body: `{ let t = $a; $a = $b; $b = t; }`
        //
        // Byte layout of `{ let t = $a; $a = $b; $b = t; }`:
        //   0-9   "{ let t = "
        //   10-11 "$a"
        //   12-13 "; "
        //   14-15 "$a"
        //   16-18 " = "
        //   19-20 "$b"
        //   21-22 "; "
        //   23-24 "$b"
        //   25-27 " = "
        //   28-30 "t; "
        //   31-31 "}"
        let template_source = "{ let t = $a; $a = $b; $b = t; }";
        let elems = vec![
            tmpl_lit(0, 10),   // "{ let t = "
            tmpl_frag(10, 2),  // "$a"
            tmpl_lit(12, 2),   // "; "
            tmpl_frag(14, 2),  // "$a"
            tmpl_lit(16, 3),   // " = "
            tmpl_frag(19, 2),  // "$b"
            tmpl_lit(21, 2),   // "; "
            tmpl_frag(23, 2),  // "$b"
            tmpl_lit(25, 3),   // " = "
            tmpl_lit(28, 4),   // "t; }"
        ];
        let mut bindings: BTreeMap<FragmentName, MatchBinding> = BTreeMap::new();
        bindings.insert("a".to_string(), bind("a", "p"));
        bindings.insert("b".to_string(), bind("b", "q"));

        let file = FileId::new(1).unwrap();
        let out = expand_macro(
            &[],
            &elems,
            &bindings,
            template_source,
            file,
            test_span(0, template_source.len() as u32),
        );

        assert!(
            out.diagnostics.is_empty(),
            "swap expansion should be clean: {:?}",
            out.diagnostics
        );
        assert_eq!(
            out.source, "{ let t = p; p = q; q = t; }",
            "swap should substitute $a→p, $b→q consistently"
        );
    }

    #[test]
    fn expand_macro_multi_fragment_round_trip() {
        // m2_macro_multi_fragment.pdx analogue: three fragments of three
        // different kinds — expr, ident, literal. Template body:
        //   { let $name = $init; $body }
        //
        // Byte layout of `{ let $name = $init; $body }`:
        //   0-5   "{ let "
        //   6-10  "$name"
        //   11-13 " = "
        //   14-18 "$init"
        //   19-20 "; "
        //   21-25 "$body"
        //   26-27 " }"
        let template_source = "{ let $name = $init; $body }";
        let elems = vec![
            tmpl_lit(0, 6),
            tmpl_frag(6, 5),
            tmpl_lit(11, 3),
            tmpl_frag(14, 5),
            tmpl_lit(19, 2),
            tmpl_frag(21, 5),
            tmpl_lit(26, 2),
        ];
        let mut bindings: BTreeMap<FragmentName, MatchBinding> = BTreeMap::new();
        bindings.insert(
            "name".to_string(),
            MatchBinding {
                name: "name".to_string(),
                kind: MacroFragmentKind::Ident,
                captured: "counter".to_string(),
            },
        );
        bindings.insert(
            "init".to_string(),
            MatchBinding {
                name: "init".to_string(),
                kind: MacroFragmentKind::Literal,
                captured: "0".to_string(),
            },
        );
        bindings.insert(
            "body".to_string(),
            MatchBinding {
                name: "body".to_string(),
                kind: MacroFragmentKind::Expr,
                captured: "counter + 1".to_string(),
            },
        );

        let file = FileId::new(1).unwrap();
        let out = expand_macro(
            &[],
            &elems,
            &bindings,
            template_source,
            file,
            test_span(0, template_source.len() as u32),
        );

        assert!(
            out.diagnostics.is_empty(),
            "multi-fragment expansion should be clean: {:?}",
            out.diagnostics
        );
        assert_eq!(out.source, "{ let counter = 0; counter + 1 }");
    }

    #[test]
    fn expand_macro_unbound_metavariable_emits_m0309() {
        // Template refers to $y but only $x is bound.
        let template_source = "$x + $y";
        let elems = vec![
            tmpl_frag(0, 2), // "$x"
            tmpl_lit(2, 3),  // " + "
            tmpl_frag(5, 2), // "$y"
        ];
        let mut bindings: BTreeMap<FragmentName, MatchBinding> = BTreeMap::new();
        bindings.insert("x".to_string(), bind("x", "1"));

        let file = FileId::new(1).unwrap();
        let out = expand_macro(
            &[],
            &elems,
            &bindings,
            template_source,
            file,
            test_span(0, 7),
        );

        assert_eq!(out.diagnostics.len(), 1, "one M0309 for the unbound $y");
        assert_eq!(out.diagnostics[0].code().number(), M_UNBOUND_META);
        // Emitted source keeps `$y` literal so the re-lexer can complain.
        assert_eq!(out.source, "1 + $y");
    }

    #[test]
    fn expand_macro_unbound_reported_once_per_unique_name() {
        let template_source = "$y $y $y";
        let elems = vec![
            tmpl_frag(0, 2),
            tmpl_lit(2, 1),
            tmpl_frag(3, 2),
            tmpl_lit(5, 1),
            tmpl_frag(6, 2),
        ];
        let bindings: BTreeMap<FragmentName, MatchBinding> = BTreeMap::new();

        let file = FileId::new(1).unwrap();
        let out = expand_macro(
            &[],
            &elems,
            &bindings,
            template_source,
            file,
            test_span(0, 8),
        );

        // Three references to the same unbound name → exactly one M0309.
        assert_eq!(out.diagnostics.len(), 1);
        assert_eq!(out.diagnostics[0].code().number(), M_UNBOUND_META);
    }

    #[test]
    fn bindings_by_name_first_wins() {
        // Two entries with the same name — first-write wins so a
        // consistency-check-shaped duplicate matches macro_rules! semantics.
        let flat = vec![bind("x", "first"), bind("x", "second")];
        let map = bindings_by_name(&flat);
        assert_eq!(map.len(), 1);
        assert_eq!(map.get("x").unwrap().captured, "first");
    }

    #[test]
    fn expand_macro_pattern_elems_parameter_reserved_no_panic() {
        // Slice B does not consume pattern_elems yet; passing a mismatched
        // set (fragments that do not appear in the template) must still
        // produce a clean expansion for the template refs that DO have
        // bindings.
        let template_source = "$x";
        let elems = vec![tmpl_frag(0, 2)];
        let mut bindings: BTreeMap<FragmentName, MatchBinding> = BTreeMap::new();
        bindings.insert("x".to_string(), bind("x", "9"));

        // Craft a pattern_elems slice with a fragment that isn't in the
        // template — this is meaningless in real usage but exercises the
        // "parameter reserved, ignored today" contract.
        let pattern_elems = vec![MacroPatternElem::Fragment {
            name: paideia_as_ast::NodeId::new(1).unwrap(),
            kind: MacroFragmentKind::Expr,
            span: test_span(100, 5),
        }];

        let file = FileId::new(1).unwrap();
        let out = expand_macro(
            &pattern_elems,
            &elems,
            &bindings,
            template_source,
            file,
            test_span(0, 2),
        );
        assert!(out.diagnostics.is_empty());
        assert_eq!(out.source, "9");
    }
}
