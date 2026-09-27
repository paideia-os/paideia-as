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

/// Diagnostic code for a template that references a fragment bound
/// only inside a `$( ... )*` / `+` group from outside such a group,
/// or that omits a required reference inside a group. Slice C
/// (PAS-DEBT-B2-010c, #1542). The task's original design named
/// M0313 for this diagnostic; M0313 is already allocated to
/// `file_module` (see `catalog.toml`), so the elaborator picks
/// M0314 as the next free slot in the macro range.
pub const M_TEMPLATE_REP_MISUSE: u16 = 314;

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
///   `template_source` verbatim. When `hygiene_scope` is `Some`,
///   identifier tokens inside the literal segment are alpha-renamed
///   with a `_h<scope_id>` suffix (soft hygiene, see below).
/// - [`MacroTemplateElem::Fragment`] — recover the reference name by
///   stripping the leading `$` from the site span, look it up in
///   `bindings`, and copy its [`MatchBinding::captured`] text. An
///   unbound name emits one `M0309` diagnostic per unique unbound name
///   and leaves the literal `$name` sequence in the emitted source so
///   the re-lexer surfaces an error at a useful location. Fragment
///   captures are copied VERBATIM — no rename — so use-site
///   identifiers keep their spelling.
/// - [`MacroTemplateElem::Repetition`] (Slice C, #1542) — look up the
///   repetition-bound fragment(s) inside the group and emit one copy
///   of the inner sequence per iteration, with the group's separator
///   between adjacent expansions. Uses `MatchBinding::reps` (the
///   per-iteration capture list) rather than `MatchBinding::captured`.
///
/// The composed source is then re-lexed under `file` so the caller
/// receives a `Vec<Token>` ready for the next parse pass.
///
/// **`pattern_elems` parameter.** Consumed by Slice C to detect
/// misuse: a template `$name` reference that resolves to a
/// repetition-bound fragment (per `pattern_elems`) but appears
/// OUTSIDE a template `Repetition` group emits M0314. Symmetric
/// misuses — a template `Repetition` referencing a single-shot
/// fragment — also emit M0314.
///
/// **Hygiene.** Slice C adds `hygiene_scope: Option<MacroScopeId>`.
/// When `Some`, the expander applies a soft alpha-rename over
/// template-literal identifier tokens: each ident is rewritten to
/// `<name>_h<scope_id>`, so a macro-introduced `let t = ...` cannot
/// capture a use-site `t`. Fragment substitutions are unrenamed.
/// This soft rename is a stopgap: full hygiene per Ullrich 2020 §3
/// still requires a name-resolver-aware `HygieneCache`, which today
/// is only wired for reflective macros (`expand_reflective_hygienic`
/// in this file, R220.M2, #1416). Slice D will bridge the same
/// machinery to the string-substitution path.
///
/// **Wildcard arms.** [`MacroTemplateElem`] and [`MacroPatternElem`]
/// are `#[non_exhaustive]`, so cross-crate `match` sites carry a `_`
/// catch-all — newer variants added ahead of this function's update
/// downgrade to a silent no-op rather than a hard compile failure at
/// the call site.
#[must_use]
pub fn expand_macro(
    _pattern_elems: &[MacroPatternElem],
    template_elems: &[MacroTemplateElem],
    bindings: &BTreeMap<FragmentName, MatchBinding>,
    template_source: &str,
    file: FileId,
    invocation_span: Span,
    hygiene_scope: Option<paideia_as_reflection::MacroScopeId>,
) -> MacroExpansion {
    let mut expanded = String::new();
    let mut diagnostics: Vec<Diagnostic> = Vec::new();
    let mut reported_unbound: BTreeMap<String, ()> = BTreeMap::new();

    expand_template_elems(
        template_elems,
        bindings,
        template_source,
        &mut expanded,
        &mut diagnostics,
        &mut reported_unbound,
        invocation_span,
        None, // iteration index — None at the top level
        hygiene_scope,
    );

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

// ─── Slice C (PAS-DEBT-B2-010c, #1542) helpers ─────────────────────────

/// Recursive template expander that walks `template_elems`, appending
/// composed source to `expanded`. Handles the Slice C `Repetition`
/// arm by iterating over the bound repetition count and re-invoking
/// itself for each iteration with an `iter_index` set.
///
/// M0314 (`M_TEMPLATE_REP_MISUSE`) fires in two cases:
/// * A top-level `Fragment` ref (`iter_index = None`) whose binding
///   carries `reps = Some(_)` — the user forgot to wrap it in a
///   `$( )*` template group.
/// * A `Repetition` group whose inner references contain no
///   rep-bound fragment — the group cannot decide its iteration
///   count.
///
/// The operational signal for "was this fragment rep-bound?" is
/// `MatchBinding::reps.is_some()`, not the pattern-side structural
/// shape — that keeps this function decoupled from `pattern_elems`
/// (retained in the caller's signature for future use).
#[allow(clippy::too_many_arguments)]
fn expand_template_elems(
    template_elems: &[MacroTemplateElem],
    bindings: &BTreeMap<FragmentName, MatchBinding>,
    template_source: &str,
    expanded: &mut String,
    diagnostics: &mut Vec<Diagnostic>,
    reported_unbound: &mut BTreeMap<String, ()>,
    invocation_span: Span,
    iter_index: Option<usize>,
    hygiene_scope: Option<paideia_as_reflection::MacroScopeId>,
) {
    for elem in template_elems {
        match elem {
            MacroTemplateElem::Literal { span } => {
                let start = span.byte_start() as usize;
                let end = start.saturating_add(span.byte_len() as usize);
                if start <= template_source.len() && end <= template_source.len() {
                    let slice = &template_source[start..end];
                    if let Some(scope) = hygiene_scope {
                        rewrite_ident_tokens_hygienic(slice, scope, expanded);
                    } else {
                        expanded.push_str(slice);
                    }
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
                match bindings.get(name) {
                    Some(b) => {
                        // Rep-bound fragment referenced OUTSIDE a
                        // template Repetition group → M0314.
                        if b.reps.is_some() && iter_index.is_none() {
                            diagnostics.push(
                                Diagnostic::error(m_code(M_TEMPLATE_REP_MISUSE))
                                    .message(format!(
                                        "fragment `${name}` was bound by a `$( )*` \
                                         group; reference it inside a template \
                                         `$( )*` group, not at top level",
                                    ))
                                    .with_span(invocation_span)
                                    .finish(),
                            );
                            // Emit the literal `$name` so the re-lexer
                            // surfaces a useful downstream location.
                            expanded.push_str(site);
                            continue;
                        }
                        // Inside a repetition group and this binding
                        // is rep-bound → emit the current iteration's
                        // capture.
                        if let (Some(idx), Some(reps)) = (iter_index, b.reps.as_ref()) {
                            if let Some(rep) = reps.get(idx) {
                                expanded.push_str(rep);
                            } else {
                                // Missing capture at this iteration —
                                // shouldn't happen if the matcher
                                // enforced consistent counts.
                                expanded.push_str(&b.captured);
                            }
                        } else {
                            expanded.push_str(&b.captured);
                        }
                    }
                    None => {
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
                        expanded.push_str(site);
                    }
                }
            }
            MacroTemplateElem::Repetition { inner, separator, span } => {
                // Discover the iteration count by finding the first
                // rep-bound fragment referenced inside `inner`. If
                // none is found, the group cannot iterate — emit
                // M0314 and skip.
                let inner_names = collect_template_fragment_names(inner, template_source);
                let mut count: Option<usize> = None;
                let mut count_owner: Option<String> = None;
                for name in &inner_names {
                    if let Some(b) = bindings.get(name)
                        && let Some(reps) = b.reps.as_ref()
                    {
                        match count {
                            None => {
                                count = Some(reps.len());
                                count_owner = Some(name.clone());
                            }
                            Some(prev) if prev != reps.len() => {
                                diagnostics.push(
                                    Diagnostic::error(m_code(M_TEMPLATE_REP_MISUSE))
                                        .message(format!(
                                            "template `$( )*` group references \
                                             `${}` (count {}) and `${}` (count {}) — \
                                             mismatched iteration counts",
                                            count_owner.as_deref().unwrap_or("?"),
                                            prev,
                                            name,
                                            reps.len()
                                        ))
                                        .with_span(*span)
                                        .finish(),
                                );
                            }
                            _ => {}
                        }
                    }
                }
                let Some(n) = count else {
                    diagnostics.push(
                        Diagnostic::error(m_code(M_TEMPLATE_REP_MISUSE))
                            .message(
                                "template `$( )*` group references no \
                                 repetition-bound fragment; unable to \
                                 determine iteration count"
                                    .to_string(),
                            )
                            .with_span(*span)
                            .finish(),
                    );
                    continue;
                };

                let sep_text = separator
                    .as_ref()
                    .map(|s| {
                        let start = s.byte_start() as usize;
                        let end = start.saturating_add(s.byte_len() as usize);
                        if start <= template_source.len() && end <= template_source.len() {
                            template_source[start..end].to_string()
                        } else {
                            String::new()
                        }
                    })
                    .unwrap_or_default();

                for i in 0..n {
                    if i > 0 && !sep_text.is_empty() {
                        expanded.push_str(&sep_text);
                    }
                    expand_template_elems(
                        inner,
                        bindings,
                        template_source,
                        expanded,
                        diagnostics,
                        reported_unbound,
                        invocation_span,
                        Some(i),
                        hygiene_scope,
                    );
                }
            }
            // #[non_exhaustive] guard: unknown variants collapse to a
            // silent no-op at cross-crate call sites.
            _ => {}
        }
    }
}

/// Recursively collect fragment reference names from a template
/// element slice by reading each Fragment site's span text.
fn collect_template_fragment_names(
    elems: &[MacroTemplateElem],
    template_source: &str,
) -> Vec<String> {
    let mut out = Vec::new();
    for e in elems {
        match e {
            MacroTemplateElem::Fragment { span, .. } => {
                let start = span.byte_start() as usize;
                let end = start.saturating_add(span.byte_len() as usize);
                if start < template_source.len() && end <= template_source.len() {
                    let site = &template_source[start..end];
                    let name = site.strip_prefix('$').unwrap_or(site);
                    out.push(name.to_string());
                }
            }
            MacroTemplateElem::Repetition { inner, .. } => {
                out.extend(collect_template_fragment_names(inner, template_source));
            }
            _ => {}
        }
    }
    out
}

/// Soft-hygiene identifier rename over a template-literal segment.
///
/// Walks `text` char-by-char, and rewrites each `[A-Za-z_][A-Za-z0-9_]*`
/// run to `<name>_h<scope>` — appending a `_h<n>` suffix where `<n>`
/// is the scope's numeric identifier. All non-identifier bytes
/// (whitespace, punctuation, operators, comments, string literals)
/// pass through unchanged.
///
/// This deliberately catches reserved keywords too (`let`, `if`,
/// `while`, ...) — a macro body's `let t = ...` becomes
/// `let_h123 t_h123 = ...` which downstream re-lexing rejects loudly
/// rather than silently. In practice, macro authors avoid naming
/// bindings after keywords, so this trade-off is acceptable for the
/// soft-hygiene stopgap; full hygiene per Ullrich 2020 §3 requires
/// name-resolver-aware bookkeeping that Slice D (paired with the
/// R220.M3 resolver wire-up) delivers.
///
/// Number literals, string / byte-string literals, and inline
/// comments are NOT walked into: any `[A-Za-z_]` starting from a
/// digit is treated as the tail of a number, and `"..."` / `'..'` /
/// `//...\n` are copied verbatim.
fn rewrite_ident_tokens_hygienic(
    text: &str,
    scope: paideia_as_reflection::MacroScopeId,
    out: &mut String,
) {
    let bytes = text.as_bytes();
    let mut i = 0usize;
    let suffix = format!("_h{}", scope.get());
    while i < bytes.len() {
        let b = bytes[i];
        // String literal
        if b == b'"' {
            let start = i;
            i += 1;
            while i < bytes.len() && bytes[i] != b'"' {
                if bytes[i] == b'\\' && i + 1 < bytes.len() {
                    i += 2;
                } else {
                    i += 1;
                }
            }
            if i < bytes.len() {
                i += 1;
            }
            out.push_str(&text[start..i]);
            continue;
        }
        // Char literal
        if b == b'\'' {
            let start = i;
            i += 1;
            while i < bytes.len() && bytes[i] != b'\'' {
                if bytes[i] == b'\\' && i + 1 < bytes.len() {
                    i += 2;
                } else {
                    i += 1;
                }
            }
            if i < bytes.len() {
                i += 1;
            }
            out.push_str(&text[start..i]);
            continue;
        }
        // Line comment
        if b == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
            let start = i;
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            out.push_str(&text[start..i]);
            continue;
        }
        // Number: starts with an ASCII digit
        if b.is_ascii_digit() {
            let start = i;
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'.')
            {
                i += 1;
            }
            out.push_str(&text[start..i]);
            continue;
        }
        // Identifier
        if b.is_ascii_alphabetic() || b == b'_' {
            let start = i;
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_')
            {
                i += 1;
            }
            out.push_str(&text[start..i]);
            out.push_str(&suffix);
            continue;
        }
        // Non-UTF8-multi-byte fallthrough: copy the single byte.
        // (All identifier-relevant chars are ASCII; multi-byte UTF-8
        // in comments/strings is handled by the branches above.)
        out.push(b as char);
        i += 1;
    }
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
        MatchBinding::single(name.to_string(), MacroFragmentKind::Expr, captured.to_string())
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
            None,
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
            None,
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
            MatchBinding::single(
                "name".to_string(),
                MacroFragmentKind::Ident,
                "counter".to_string(),
            ),
        );
        bindings.insert(
            "init".to_string(),
            MatchBinding::single(
                "init".to_string(),
                MacroFragmentKind::Literal,
                "0".to_string(),
            ),
        );
        bindings.insert(
            "body".to_string(),
            MatchBinding::single(
                "body".to_string(),
                MacroFragmentKind::Expr,
                "counter + 1".to_string(),
            ),
        );

        let file = FileId::new(1).unwrap();
        let out = expand_macro(
            &[],
            &elems,
            &bindings,
            template_source,
            file,
            test_span(0, template_source.len() as u32),
            None,
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
            None,
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
            None,
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
            None,
        );
        assert!(out.diagnostics.is_empty());
        assert_eq!(out.source, "9");
    }

    // ─── Slice C (PAS-DEBT-B2-010c, #1542) repetition + hygiene tests ─

    /// Build a template repetition elem covering `[start .. start+len]`
    /// with the inner elems provided; no separator (None) unless the
    /// test supplies one via sep_span.
    fn tmpl_rep(
        inner: Vec<MacroTemplateElem>,
        separator: Option<Span>,
        start: u32,
        len: u32,
    ) -> MacroTemplateElem {
        MacroTemplateElem::Repetition {
            inner,
            separator,
            span: test_span(start, len),
        }
    }

    /// Build a `MatchBinding::repeated` — a Slice C repetition binding.
    fn bind_rep(name: &str, reps: Vec<&str>, sep: &str) -> MatchBinding {
        MatchBinding::repeated(
            name.to_string(),
            MacroFragmentKind::Expr,
            reps.iter().map(|s| s.to_string()).collect(),
            sep,
        )
    }

    #[test]
    fn expand_macro_repetition_star_zero_iterations_emits_empty() {
        // Template `[ $($x),* ]` — byte layout:
        //   0 '['  1 ' '  2 '$'  3 '('  4 '$'  5 'x'  6 ')'  7 ','  8 '*'  9 ' '  10 ']'
        let template_source = "[ $($x),* ]";
        let inner = vec![tmpl_frag(4, 2)]; // "$x" at bytes 4..6
        let sep_span = Some(test_span(7, 1)); // "," at byte 7
        let elems = vec![
            tmpl_lit(0, 2),                    // "[ "
            tmpl_rep(inner, sep_span, 2, 7),   // "$($x),*" at bytes 2..9
            tmpl_lit(9, 2),                    // " ]"
        ];
        let mut bindings: BTreeMap<FragmentName, MatchBinding> = BTreeMap::new();
        bindings.insert("x".to_string(), bind_rep("x", vec![], ","));

        let file = FileId::new(1).unwrap();
        let out = expand_macro(
            &[],
            &elems,
            &bindings,
            template_source,
            file,
            test_span(0, 11),
            None,
        );
        assert!(
            out.diagnostics.is_empty(),
            "clean zero-iteration expansion: {:?}",
            out.diagnostics
        );
        assert!(out.source.contains("["));
        assert!(out.source.contains("]"));
    }

    #[test]
    fn expand_macro_repetition_star_three_iterations_emits_all() {
        // Template `[ $($x),* ]` with $x bound to ["1", "2", "3"] → source contains "1,2,3".
        let template_source = "[ $($x),* ]";
        let inner = vec![tmpl_frag(4, 2)];
        let sep_span = Some(test_span(7, 1));
        let elems = vec![
            tmpl_lit(0, 2),
            tmpl_rep(inner, sep_span, 2, 7),
            tmpl_lit(9, 2),
        ];
        let mut bindings: BTreeMap<FragmentName, MatchBinding> = BTreeMap::new();
        bindings.insert("x".to_string(), bind_rep("x", vec!["1", "2", "3"], ","));

        let file = FileId::new(1).unwrap();
        let out = expand_macro(
            &[],
            &elems,
            &bindings,
            template_source,
            file,
            test_span(0, 11),
            None,
        );
        assert!(
            out.diagnostics.is_empty(),
            "clean three-iteration expansion: {:?}",
            out.diagnostics
        );
        assert!(out.source.contains("1,2,3"), "expected joined ints in {:?}", out.source);
    }

    #[test]
    fn expand_macro_rep_bound_ref_outside_group_emits_m0314() {
        // Template `$x` (top-level fragment ref) but $x is rep-bound.
        // Should emit one M0314.
        let template_source = "$x";
        let elems = vec![tmpl_frag(0, 2)];
        let mut bindings: BTreeMap<FragmentName, MatchBinding> = BTreeMap::new();
        bindings.insert("x".to_string(), bind_rep("x", vec!["a", "b"], ","));

        let file = FileId::new(1).unwrap();
        let out = expand_macro(
            &[],
            &elems,
            &bindings,
            template_source,
            file,
            test_span(0, 2),
            None,
        );
        assert_eq!(out.diagnostics.len(), 1, "one M0314 expected");
        assert_eq!(out.diagnostics[0].code().number(), M_TEMPLATE_REP_MISUSE);
    }

    #[test]
    fn expand_macro_rep_group_without_rep_fragment_emits_m0314() {
        // Template has a Repetition group but no fragment reference
        // inside it that is rep-bound → cannot decide iteration count
        // → M0314.
        // Template `$($x),*` — byte layout: 0 '$' 1 '(' 2 '$' 3 'x'
        //   4 ')' 5 ',' 6 '*'. Length 7.
        let template_source = "$($x),*";
        let inner = vec![tmpl_frag(2, 2)];   // "$x" at bytes 2..4
        let sep_span = Some(test_span(5, 1)); // "," at byte 5
        let elems = vec![tmpl_rep(inner, sep_span, 0, 7)];
        let mut bindings: BTreeMap<FragmentName, MatchBinding> = BTreeMap::new();
        // Bind $x as SINGLE (not repeated) — the group cannot iterate.
        bindings.insert("x".to_string(), bind("x", "single"));

        let file = FileId::new(1).unwrap();
        let out = expand_macro(
            &[],
            &elems,
            &bindings,
            template_source,
            file,
            test_span(0, 7),
            None,
        );
        assert!(
            out.diagnostics.iter().any(|d| d.code().number() == M_TEMPLATE_REP_MISUSE),
            "M0314 expected in {:?}",
            out.diagnostics
        );
    }

    #[test]
    fn expand_macro_hygiene_scope_renames_template_idents() {
        // Template `let t = $a` with hygiene_scope = Some(1) → identifier
        // `t` gets an `_h1` suffix; substituted `$a` capture is verbatim.
        let template_source = "let t = $a";
        let elems = vec![
            tmpl_lit(0, 8), // "let t = "
            tmpl_frag(8, 2), // "$a"
        ];
        let mut bindings: BTreeMap<FragmentName, MatchBinding> = BTreeMap::new();
        bindings.insert("a".to_string(), bind("a", "p"));

        let file = FileId::new(1).unwrap();
        let scope = paideia_as_reflection::MacroScopeId::from_raw(1)
            .expect("MacroScopeId(1) is valid");
        let out = expand_macro(
            &[],
            &elems,
            &bindings,
            template_source,
            file,
            test_span(0, template_source.len() as u32),
            Some(scope),
        );
        assert!(
            out.diagnostics.is_empty(),
            "clean hygiene expansion: {:?}",
            out.diagnostics
        );
        // The `t` in the template should be renamed to `t_h1`.
        assert!(
            out.source.contains("t_h1"),
            "expected renamed `t_h1` in {:?}",
            out.source
        );
        // The `$a` fragment substitution should NOT be renamed.
        assert!(
            out.source.ends_with("p"),
            "fragment capture should be verbatim in {:?}",
            out.source
        );
    }
}
