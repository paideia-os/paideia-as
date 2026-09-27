//! Compile-time `@macro_expand([<macro>, "<input>"])` driver pass
//! (paideia-as#1556, PAS-DEBT-B7-001-b, v0.36.74).
//!
//! # Purpose
//!
//! Slice B7-001 (paideia-as#1529) discovered that although
//! [`crate::macro_expand::expand_macro`] and
//! [`crate::macro_match::match_structured`] have been in place since
//! Slices A/B/C (paideia-as#1503, #1541, #1542), the `cmd_build`
//! pipeline never invoked them — user-authored macro invocation
//! syntax (`foo!(...)`) is R221.M4's concern and had not yet landed.
//!
//! This pass walks the arena's
//! [`paideia_as_ast::MacroExpandDirectiveTable`], resolves each
//! directive's macro name against the file's macro declarations, runs
//! match + expand, and forwards every diagnostic
//! (M0308 / M0309 / M0310 / M0311 / M0314 / M0315) into the caller's
//! [`paideia_as_diagnostics::DiagnosticSink`]. A successful
//! expansion produces no downstream side-effect — the annotation is
//! a compile-time assertion that the referenced macro CAN expand
//! cleanly for the given input, matching the "Option b'" fallback
//! called out in the task brief.
//!
//! # Scope
//!
//! **INTERNAL-ONLY** driver hook. Retires once R221.M4 lands
//! user-facing macro invocation syntax and reflection-corpus /
//! codes-corpus tests can drive the expander through ordinary
//! `foo!(...)` call sites.
//!
//! # Diagnostics
//!
//! Reuses existing macro codes rather than minting a per-directive
//! M-code family:
//!
//! - **M0308** — no rule in the referenced macro's pattern list
//!   accepted the supplied input.
//! - **M0309** — template referenced an unbound `$name` metavariable
//!   (originated by [`crate::macro_expand::expand_macro`]).
//! - **M0310** — repetition-count mismatch inside a matched rule
//!   (originated by [`crate::macro_match::match_structured`]).
//! - **M0311** — expansion depth exceeded
//!   [`crate::macro_expand::MAX_EXPANSION_DEPTH`].
//! - **M0314** — template `$( )*` misuse.
//! - **M0315** — referenced macro name not resolvable in scope (new to
//!   Slice B7-001-b; see [`M_MACRO_NOT_FOUND`]).

use paideia_as_ast::{
    AstArena, ItemData, MacroDeclData, NodeId, NodeKind,
};
use paideia_as_diagnostics::{
    Category, Diagnostic, DiagnosticCode, DiagnosticSink, Severity, Span,
};

use crate::macro_expand::{bindings_by_name, expand_macro};
use crate::macro_match::{StructuredMatch, match_structured};

/// Diagnostic code for "@macro_expand references an unknown macro
/// name" (paideia-as#1556). Slots into the macro range
/// (M0308..M0315) alongside the existing match / expand codes.
pub const M_MACRO_NOT_FOUND: u16 = 315;

/// Diagnostic code for "no rule matched" — re-emitted here after a
/// pattern-side match failure so the anchor points at the
/// `@macro_expand(...)` directive rather than the macro declaration.
/// Numerically identical to [`crate::macro_match::M_NO_MATCH`].
pub const M_NO_MATCH: u16 = 308;

/// Run every recorded `@macro_expand([<macro>, "<input>"])`
/// directive against the arena.
///
/// The pass reads the arena's
/// [`paideia_as_ast::MacroExpandDirectiveTable`] and the top-level
/// `Structure` at `root_id`. For every directive:
///
/// 1. Resolve `directive.macro_name` against MacroDecl items reachable
///    from `root_id` (walks nested Module / Structure / Functor
///    bodies).
/// 2. If unresolved → emit M0315 and continue.
/// 3. Try each rule via [`match_structured`] against
///    `directive.input`. Any diagnostics from a successful match still
///    flow through (M0310).
/// 4. On the first successful match, invoke [`expand_macro`] with the
///    bindings and forward every returned diagnostic
///    (M0309 / M0311 / M0314) into `sink`.
/// 5. If NO rule matched → emit M0308 anchored at the directive's
///    span.
///
/// # Panics
///
/// Does not panic on missing arena data — every dereference is
/// checked and degrades to a skip rather than an ICE.
pub fn run_macro_expand_directives(
    arena: &AstArena,
    source: &str,
    root_id: NodeId,
    sink: &mut dyn DiagnosticSink,
) {
    let table = arena.macro_expand_directives();
    if table.is_empty() {
        return;
    }

    let macros = collect_macro_decls(arena, root_id);

    for directive in table.entries() {
        // Resolve macro name.
        let resolved = macros.iter().find(|(_, m)| {
            arena
                .get(m.name)
                .and_then(|n| slice_source(source, n.span))
                .map(|s| s == directive.macro_name.as_str())
                .unwrap_or(false)
        });

        let Some((_decl_id, decl_data)) = resolved else {
            emit_m(
                sink,
                M_MACRO_NOT_FOUND,
                format!(
                    "@macro_expand references macro '{}', which is not \
                     declared in this file",
                    directive.macro_name,
                ),
                directive.span,
            );
            continue;
        };

        // Try each rule.
        let mut matched_ok = false;
        for rule in &decl_data.rules {
            let pattern_text = arena
                .get(rule.pattern)
                .and_then(|n| slice_source(source, n.span))
                .unwrap_or("");
            let template_text = arena
                .get(rule.template)
                .and_then(|n| slice_source(source, n.span))
                .unwrap_or("");

            match match_structured(
                &rule.pattern_elems,
                pattern_text,
                &directive.input,
                directive.span,
            ) {
                StructuredMatch::Ok { bindings, diagnostics } => {
                    for d in diagnostics {
                        let _ = sink.emit(d);
                    }
                    let by_name = bindings_by_name(&bindings);
                    let file = directive.span.file();
                    let expansion = expand_macro(
                        &rule.pattern_elems,
                        &rule.template_elems,
                        &by_name,
                        template_text,
                        file,
                        directive.span,
                        None, // hygiene scope: not needed for compile-time assertion
                    );
                    for d in expansion.diagnostics {
                        let _ = sink.emit(d);
                    }
                    matched_ok = true;
                    break;
                }
                StructuredMatch::Failed => {
                    // Try next rule.
                }
            }
        }

        if !matched_ok {
            emit_m(
                sink,
                M_NO_MATCH,
                format!(
                    "@macro_expand: no rule in macro '{}' matches input '{}'",
                    directive.macro_name, directive.input,
                ),
                directive.span,
            );
        }
    }
}

/// Walk `root_id`'s Structure item list (and every nested
/// Module / Structure / Functor body) and collect every MacroDecl.
fn collect_macro_decls<'a>(
    arena: &'a AstArena,
    root_id: NodeId,
) -> Vec<(NodeId, &'a MacroDeclData)> {
    let mut out: Vec<(NodeId, &MacroDeclData)> = Vec::new();
    collect_from(arena, root_id, &mut out);
    out
}

fn collect_from<'a>(
    arena: &'a AstArena,
    id: NodeId,
    out: &mut Vec<(NodeId, &'a MacroDeclData)>,
) {
    let Some(node) = arena.get(id) else {
        return;
    };
    match node.kind {
        NodeKind::MacroDecl => {
            if let Some(ItemData::MacroDecl(m)) = arena.item_data(id) {
                out.push((id, m));
            }
        }
        NodeKind::Structure => {
            if let Some(ItemData::Structure { items, .. }) = arena.item_data(id) {
                for child in items {
                    collect_from(arena, *child, out);
                }
            }
        }
        NodeKind::Module => {
            if let Some(ItemData::Module { body, .. }) = arena.item_data(id) {
                collect_from(arena, *body, out);
            }
        }
        NodeKind::Functor => {
            if let Some(ItemData::Functor { body, .. }) = arena.item_data(id) {
                collect_from(arena, *body, out);
            }
        }
        // Every other node kind: not a container of macro items.
        // Wildcard guard preserves forward-compatibility with future
        // NodeKind additions (NodeKind is #[non_exhaustive]).
        _ => {}
    }
}

/// Slice a byte range from `source` bounded by a [`Span`]. Returns
/// `None` on out-of-range spans.
fn slice_source(source: &str, span: Span) -> Option<&str> {
    let start = span.byte_start() as usize;
    let end = start.saturating_add(span.byte_len() as usize);
    if start <= source.len() && end <= source.len() && start <= end {
        Some(&source[start..end])
    } else {
        None
    }
}

fn emit_m(sink: &mut dyn DiagnosticSink, number: u16, msg: String, span: Span) {
    let code = DiagnosticCode::new(Category::M, Severity::Error, number)
        .expect("valid M code");
    let d = Diagnostic::error(code)
        .message(msg)
        .with_span(span)
        .finish();
    let _ = sink.emit(d);
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_ast::{
        AstArena, ItemData, MacroDeclData, MacroExpandDirective, MacroFragment,
        MacroFragmentKind, MacroPatternElem, MacroRule, MacroTemplateElem, NodeKind,
    };
    use paideia_as_diagnostics::{FileId, Span, VecSink};

    fn file() -> FileId {
        FileId::new(1).unwrap()
    }

    fn s(start: u32, len: u32) -> Span {
        Span::new(file(), start, len)
    }

    /// Build a synthetic arena for a two-item file:
    ///
    /// ```text
    /// bytes 0..2   "id"        ← macro name Ident
    /// bytes 2..4   "$x"        ← pattern
    /// bytes 4..10  "{ $x }"    ← template
    ///   4..6 "{ "
    ///   6..8 "$x"
    ///   8..10 " }"
    /// ```
    fn build_identity_arena() -> (AstArena, String, NodeId) {
        let source = "id$x{ $x }".to_string();
        let mut arena = AstArena::new();

        let name_id = arena.alloc(NodeKind::Ident, s(0, 2));
        let pat_id = arena.alloc(NodeKind::MacroPattern, s(2, 2));
        let frag_name_id = arena.alloc(NodeKind::Ident, s(3, 1));
        let tmpl_id = arena.alloc(NodeKind::MacroTemplate, s(4, 6));
        let tmpl_frag_name_id = arena.alloc(NodeKind::Ident, s(7, 1));

        let rule = MacroRule {
            pattern: pat_id,
            template: tmpl_id,
            pattern_elems: vec![MacroPatternElem::Fragment {
                name: frag_name_id,
                kind: MacroFragmentKind::Expr,
                span: s(2, 2),
            }],
            fragments: vec![MacroFragment {
                name: frag_name_id,
                kind: MacroFragmentKind::Expr,
            }],
            template_elems: vec![
                MacroTemplateElem::Literal { span: s(4, 2) }, // "{ "
                MacroTemplateElem::Fragment {
                    name: tmpl_frag_name_id,
                    span: s(6, 2), // "$x"
                },
                MacroTemplateElem::Literal { span: s(8, 2) }, // " }"
            ],
        };

        let macro_id = arena.alloc_item(
            NodeKind::MacroDecl,
            s(0, 10),
            ItemData::MacroDecl(MacroDeclData {
                name: name_id,
                rules: vec![rule],
                doc: None,
            }),
        );

        let root_id = arena.alloc_item(
            NodeKind::Structure,
            s(0, 10),
            ItemData::Structure {
                items: vec![macro_id],
                inner_attrs: Vec::new(),
                doc: None,
            },
        );
        (arena, source, root_id)
    }

    #[test]
    fn identity_directive_expands_cleanly() {
        let (mut arena, source, root_id) = build_identity_arena();
        arena
            .macro_expand_directives_mut()
            .push(MacroExpandDirective {
                macro_name: "id".to_string(),
                input: "42".to_string(),
                span: s(0, 1),
            });

        let mut sink = VecSink::new();
        run_macro_expand_directives(&arena, &source, root_id, &mut sink);
        assert!(
            sink.diagnostics().is_empty(),
            "identity should expand clean; got {:?}",
            sink.diagnostics()
        );
    }

    #[test]
    fn missing_macro_name_emits_m0315() {
        let (mut arena, source, root_id) = build_identity_arena();
        arena
            .macro_expand_directives_mut()
            .push(MacroExpandDirective {
                macro_name: "not_there".to_string(),
                input: "42".to_string(),
                span: s(0, 1),
            });

        let mut sink = VecSink::new();
        run_macro_expand_directives(&arena, &source, root_id, &mut sink);
        let diags = sink.diagnostics();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().category().letter(), 'M');
        assert_eq!(diags[0].code().number(), M_MACRO_NOT_FOUND);
    }

    #[test]
    fn no_rule_matches_emits_m0308() {
        // Build a macro whose pattern requires two args, then fire a
        // directive with one arg. The structured matcher fails and
        // we expect M0308.
        //
        // Byte layout of "swap$a,$b{($a,$b)}":
        //   0..4  "swap"
        //   4..6  "$a"
        //   6..7  ","
        //   7..9  "$b"
        //   9..18 "{($a,$b)}"
        //     11..13 "$a"
        //     14..16 "$b"
        let source = "swap$a,$b{($a,$b)}".to_string();
        let mut arena = AstArena::new();

        let name_id = arena.alloc(NodeKind::Ident, s(0, 4));

        let pat_id = arena.alloc(NodeKind::MacroPattern, s(4, 5));
        let frag_a = arena.alloc(NodeKind::Ident, s(5, 1));
        let frag_b = arena.alloc(NodeKind::Ident, s(8, 1));

        let tmpl_id = arena.alloc(NodeKind::MacroTemplate, s(9, 9));
        let tmpl_frag_a = arena.alloc(NodeKind::Ident, s(12, 1));
        let tmpl_frag_b = arena.alloc(NodeKind::Ident, s(15, 1));

        let rule = MacroRule {
            pattern: pat_id,
            template: tmpl_id,
            pattern_elems: vec![
                MacroPatternElem::Fragment {
                    name: frag_a,
                    kind: MacroFragmentKind::Expr,
                    span: s(4, 2),
                },
                MacroPatternElem::Literal { span: s(6, 1) },
                MacroPatternElem::Fragment {
                    name: frag_b,
                    kind: MacroFragmentKind::Expr,
                    span: s(7, 2),
                },
            ],
            fragments: vec![
                MacroFragment { name: frag_a, kind: MacroFragmentKind::Expr },
                MacroFragment { name: frag_b, kind: MacroFragmentKind::Expr },
            ],
            template_elems: vec![
                MacroTemplateElem::Literal { span: s(9, 2) },
                MacroTemplateElem::Fragment {
                    name: tmpl_frag_a,
                    span: s(11, 2),
                },
                MacroTemplateElem::Literal { span: s(13, 1) },
                MacroTemplateElem::Fragment {
                    name: tmpl_frag_b,
                    span: s(14, 2),
                },
                MacroTemplateElem::Literal { span: s(16, 2) },
            ],
        };

        let macro_id = arena.alloc_item(
            NodeKind::MacroDecl,
            s(0, 18),
            ItemData::MacroDecl(MacroDeclData {
                name: name_id,
                rules: vec![rule],
                doc: None,
            }),
        );
        let root_id = arena.alloc_item(
            NodeKind::Structure,
            s(0, 18),
            ItemData::Structure {
                items: vec![macro_id],
                inner_attrs: Vec::new(),
                doc: None,
            },
        );

        arena
            .macro_expand_directives_mut()
            .push(MacroExpandDirective {
                macro_name: "swap".to_string(),
                input: "1".to_string(), // Missing second arg.
                span: s(0, 1),
            });

        let mut sink = VecSink::new();
        run_macro_expand_directives(&arena, &source, root_id, &mut sink);
        let diags = sink.diagnostics();
        assert!(
            diags.iter().any(|d| d.code().number() == M_NO_MATCH),
            "expected M0308; got {:?}",
            diags
        );
    }

    #[test]
    fn empty_table_no_op() {
        let (arena, source, root_id) = build_identity_arena();
        let mut sink = VecSink::new();
        run_macro_expand_directives(&arena, &source, root_id, &mut sink);
        assert!(sink.diagnostics().is_empty());
    }

    #[test]
    fn resolver_walks_into_module_body() {
        // Build a Module wrapping a Structure that holds the macro,
        // so the resolver has to recurse through the Module.
        let (mut arena, source, _) = build_identity_arena();
        // Wrap in a Module → Structure. Reuse the identity source.
        let (macro_id, existing_structure_id) = {
            let mut mid = None;
            let mut sid = None;
            for i in 1..=arena.len() as u32 {
                let nid = paideia_as_ast::NodeId::new(i).unwrap();
                if let Some(nd) = arena.get(nid) {
                    match nd.kind {
                        NodeKind::MacroDecl if mid.is_none() => mid = Some(nid),
                        NodeKind::Structure if sid.is_none() => sid = Some(nid),
                        _ => {}
                    }
                }
            }
            (mid.expect("macro exists"), sid.expect("structure exists"))
        };
        let name_id = arena.alloc(NodeKind::Ident, s(0, 2));
        let module_id = arena.alloc_item(
            NodeKind::Module,
            s(0, 10),
            ItemData::Module {
                name: name_id,
                sig: None,
                body: existing_structure_id,
                inner_attrs: Vec::new(),
                doc: None,
            },
        );
        // New root wrapping the module.
        let new_root = arena.alloc_item(
            NodeKind::Structure,
            s(0, 10),
            ItemData::Structure {
                items: vec![module_id],
                inner_attrs: Vec::new(),
                doc: None,
            },
        );

        arena
            .macro_expand_directives_mut()
            .push(MacroExpandDirective {
                macro_name: "id".to_string(),
                input: "42".to_string(),
                span: s(0, 1),
            });

        let mut sink = VecSink::new();
        run_macro_expand_directives(&arena, &source, new_root, &mut sink);
        assert!(
            sink.diagnostics().is_empty(),
            "recursive resolver should find nested macro; got {:?}",
            sink.diagnostics()
        );
        let _ = macro_id; // referenced only for clarity above
    }
}
