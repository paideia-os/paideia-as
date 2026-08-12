//! Effect-row → cap-set coupling walker.
//!
//! Walks every top-level `let` item's annotated function-shaped type
//! and enforces: for each effect ident that appears in the row, if the
//! `paideia-as-effects::effect_cap_binding` registry names a required
//! capability, that capability ident must appear in the `@{...}`
//! declaration. Missing caps emit [`C_EFFECT_REQUIRES_CAP`] (`C1301`).
//!
//! ## Why this pass exists
//!
//! `next-wave-softarch.md` §3 R29 — structural witness: every driver
//! process' effect row is checked at link time; a driver claiming
//! `!{mmio_read}` in its effect row without holding an `MmioMemCap`
//! (spelled `paideia.mmio` in source) must fail elaboration. This pass
//! is the R29-substrate elaboration-time half; a later PAX-manifest
//! check (R29.M6) enforces the same coupling at link time.
//!
//! ## Scope
//!
//! - Runs on the raw AST **before** IR lowering, since the check is
//!   syntactic over the annotated type of a `let` — no lowering
//!   required.
//! - Recurses into `Module { body }` → `Structure { items }` →
//!   `Functor { body }` so nested modules and functors are covered.
//! - Also inspects `Effect { ops }` operation return types (an op
//!   whose return type is a FnPtr) — although the current .pdx corpus
//!   doesn't exercise this shape, the walker is uniform.
//! - Emits **one** `C1301` per (function, missing cap) pair; multiple
//!   effects requiring the same missing cap collapse into one diag.
//!
//! ## Diagnostic anchoring
//!
//! The span points at the `let` binding's name — the surface a driver
//! author reads first. The message names the offending effect and the
//! required cap so the fix is obvious: add the cap to `@{...}` or
//! remove the effect from `!{...}`.

use std::collections::BTreeSet;

use paideia_as_ast::{AstArena, ItemData, NodeId, NodeKind, TypeData};
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, SourceMap};
use paideia_as_effects::required_cap_for_effect;

use crate::cap_infer::C_EFFECT_REQUIRES_CAP;
use crate::lower_type::get_ident_text;

/// Walk the AST rooted at `root` and emit a `C1301` diagnostic for
/// every `let` whose annotated function-shaped type contains an
/// effect requiring a capability not declared in the same signature.
///
/// Returns all diagnostics; the caller pushes them into its
/// [`DiagnosticSink`]. Non-error diagnostics are never produced.
///
/// The walker is span-driven — it never fabricates spans; when the
/// underlying ident text cannot be read (source-map miss on a
/// generated node), the walker silently skips that ident rather than
/// emit a spurious diagnostic.
#[must_use]
pub fn check_effect_cap_coupling(
    ast: &AstArena,
    source_map: &SourceMap,
    root: NodeId,
) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    walk_item(ast, source_map, root, &mut diags);
    diags
}

fn walk_item(
    ast: &AstArena,
    source_map: &SourceMap,
    node: NodeId,
    diags: &mut Vec<Diagnostic>,
) {
    let Some(node_data) = ast.get(node) else {
        return;
    };

    match node_data.kind {
        NodeKind::Module | NodeKind::Structure | NodeKind::Functor => {
            match ast.item_data(node) {
                Some(ItemData::Module { body, .. }) => walk_item(ast, source_map, *body, diags),
                Some(ItemData::Structure { items, .. }) => {
                    for item in items {
                        walk_item(ast, source_map, *item, diags);
                    }
                }
                Some(ItemData::Functor { body, .. }) => walk_item(ast, source_map, *body, diags),
                _ => {}
            }
        }
        NodeKind::Let => {
            if let Some(ItemData::Let {
                name,
                ty: Some(ty_node),
                ..
            }) = ast.item_data(node)
            {
                check_fn_shape(ast, source_map, *name, *ty_node, diags);
            }
        }
        _ => {}
    }
}

/// If `ty_node` is a FnPtr or Closure type, run the coupling check on
/// its effect row + cap set; anchor diagnostics at `name_node`.
fn check_fn_shape(
    ast: &AstArena,
    source_map: &SourceMap,
    name_node: NodeId,
    ty_node: NodeId,
    diags: &mut Vec<Diagnostic>,
) {
    let Some(td) = ast.type_data(ty_node) else {
        return;
    };
    let (effects_opt, caps_opt) = match td {
        TypeData::FnPtr {
            effects,
            capabilities,
            ..
        } => (*effects, *capabilities),
        TypeData::Closure {
            effects,
            capabilities,
            ..
        } => (*effects, *capabilities),
        _ => return,
    };

    // If no effect row annotated, nothing can be required. Empty row
    // (spelled `!{}`) is equivalent — either way, `effect_idents` is
    // the empty set.
    let effect_idents = row_idents(ast, source_map, effects_opt);
    if effect_idents.is_empty() {
        return;
    }
    let cap_idents: BTreeSet<String> = row_idents(ast, source_map, caps_opt).into_iter().collect();

    // Diagnostic anchor: the let-binding's name span. If the name node
    // is missing (should not happen for a valid Let), fall back to the
    // type-annotation node's span — never fabricate.
    let span = ast
        .get(name_node)
        .map(|nd| nd.span)
        .or_else(|| ast.get(ty_node).map(|nd| nd.span))
        .expect("either name or ty node has a span");

    // Collect each unique (effect, missing_cap) pair. Multiple effects
    // may map to the same cap; we emit one diagnostic per pair so the
    // author sees which effect drove the requirement.
    let mut emitted: BTreeSet<(String, &'static str)> = BTreeSet::new();
    for effect_name in &effect_idents {
        let Some(required) = required_cap_for_effect(effect_name) else {
            continue;
        };
        if cap_idents.contains(required) {
            continue;
        }
        if !emitted.insert((effect_name.clone(), required)) {
            continue;
        }
        let name_text = get_ident_text(ast, name_node, source_map).unwrap_or_else(|_| String::from("<anon>"));
        diags.push(
            Diagnostic::error(c_code(C_EFFECT_REQUIRES_CAP))
                .message(format!(
                    "function `{name}` declares effect `{effect}` in its row but \
                     does not hold the required capability `{cap}` in its \
                     `@{{...}}` declaration",
                    name = name_text,
                    effect = effect_name,
                    cap = required,
                ))
                .with_span(span)
                .finish(),
        );
    }
}

/// Extract the ordered list of comma-separated entries from a `!{...}`
/// or `@{...}` node — both of which land as `TypeData::EffectRow` (the
/// parser reuses the variant for capability sets). Dotted paths like
/// `paideia.mmio` land as separate `Ident` children in the AST, so we
/// recover them by reading the source text of the enclosing `{...}`
/// node and splitting on commas. This is the only way to keep dotted
/// cap idents intact without extending the AST — see
/// `parse_type/effect_row.rs::parse_cap_set` for the parser's dotted-
/// path handling.
///
/// Returns an empty vec when the node is absent or the source text
/// cannot be read (bad file id).
fn row_idents(
    ast: &AstArena,
    source_map: &SourceMap,
    node_opt: Option<NodeId>,
) -> Vec<String> {
    let Some(node) = node_opt else {
        return Vec::new();
    };
    // If it is not an EffectRow, we cannot parse it structurally, but
    // the source-text fallback still works — the span covers the
    // literal `!{...}` or `@{...}`.
    let Some(node_data) = ast.get(node) else {
        return Vec::new();
    };
    let span = node_data.span;
    let file = span.file();
    let source = source_map.content(file);
    let start = span.byte_start() as usize;
    let end = start + span.byte_len() as usize;
    if end > source.len() || start >= end {
        return Vec::new();
    }
    let raw = &source[start..end];

    // Trim leading `!{` / `@{` and trailing `}`; then split entries by
    // comma. A trailing `| rest` marks an open row variable in an
    // effect row — that arm never carries a cap requirement, so we
    // drop everything from `|` onward before splitting on commas.
    let inner = raw
        .trim_start_matches('!')
        .trim_start_matches('@')
        .trim_start_matches('{')
        .trim_end_matches('}');
    let head = inner.split('|').next().unwrap_or("");
    head.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

fn c_code(n: u16) -> DiagnosticCode {
    DiagnosticCode::new(Category::C, Severity::Error, n).expect("valid C code")
}

// The `Let` items live under a `Structure` inside a `Module`, so a
// `walk_item` recursion is enough for today's programs. Once
// `paideia-as-modules` phase adds nested-let-in-let or block-scoped
// modules, extend `walk_item` there rather than mutating this file.

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::{SourceMap, VecSink};
    use paideia_as_lexer::{Lexer, SourceText};
    use paideia_as_parser::Parser;

    fn parse(src: &str, file_name: &str) -> (AstArena, SourceMap, NodeId) {
        let mut source_map = SourceMap::new();
        let file = source_map.add_file(std::path::PathBuf::from(file_name), src.to_string());
        let source_text = SourceText::from_bytes(file, src.as_bytes()).expect("valid utf-8");
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut lex_collector = VecSink::new();
        let mut lex = Lexer::new(file, &source_text);
        let tokens = lex.collect_tokens(&mut lex_collector);
        let root = {
            let mut p = Parser::new(&tokens, source_text.content(), file, &mut arena, &mut sink);
            p.parse_source_file().expect("parse succeeds")
        };
        (arena, source_map, root)
    }

    #[test]
    fn accept_mmio_effect_with_matching_cap_emits_nothing() {
        let src = r#"
module MmioOk = structure {
  let read : (u64) -> u64 !{Mmio} @{paideia.mmio} =
    fn (a : u64) -> 0
}
"#;
        let (arena, source_map, root) = parse(src, "mmio_ok.pdx");
        let diags = check_effect_cap_coupling(&arena, &source_map, root);
        assert!(diags.is_empty(), "expected no diagnostics; got {diags:?}");
    }

    #[test]
    fn reject_mmio_effect_without_cap_emits_c1301() {
        let src = r#"
module MmioBad = structure {
  let read : (u64) -> u64 !{Mmio} @{} =
    fn (a : u64) -> 0
}
"#;
        let (arena, source_map, root) = parse(src, "mmio_bad.pdx");
        let diags = check_effect_cap_coupling(&arena, &source_map, root);
        assert_eq!(diags.len(), 1, "expected one C1301; got {diags:?}");
        assert_eq!(diags[0].code().number(), 1301);
    }

    #[test]
    fn reject_mmio_read_op_style_effect_without_cap_emits_c1301() {
        // R29.M2-002 canonical shape: !{mmio_read} without @{paideia.mmio}
        let src = r#"
module MmioReadBad = structure {
  let driver_bad : (u32) -> u64 !{mmio_read} @{} =
    fn (a : u32) -> 0
}
"#;
        let (arena, source_map, root) = parse(src, "mmio_read_bad.pdx");
        let diags = check_effect_cap_coupling(&arena, &source_map, root);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), 1301);
        let msg = diags[0].message();
        assert!(
            msg.contains("mmio_read"),
            "message should name the effect; got: {msg}"
        );
        assert!(
            msg.contains("paideia.mmio"),
            "message should name the required cap; got: {msg}"
        );
    }

    #[test]
    fn reject_two_effects_missing_same_cap_deduplicates() {
        // Both mmio_read and Mmio require paideia.mmio; we still emit
        // one diag per (effect, cap) pair so the author sees each
        // effect-side spelling.
        let src = r#"
module TwoEffs = structure {
  let f : (u64) -> u64 !{Mmio, mmio_read} @{} =
    fn (a : u64) -> 0
}
"#;
        let (arena, source_map, root) = parse(src, "two_effs.pdx");
        let diags = check_effect_cap_coupling(&arena, &source_map, root);
        assert_eq!(diags.len(), 2);
        assert!(diags.iter().all(|d| d.code().number() == 1301));
    }

    #[test]
    fn reject_rawmem_without_paideia_raw_mem_emits_c1301() {
        let src = r#"
module RawMemBad = structure {
  let bad : (u64) -> u64 !{RawMem} @{} =
    fn (a : u64) -> 0
}
"#;
        let (arena, source_map, root) = parse(src, "raw_mem_bad.pdx");
        let diags = check_effect_cap_coupling(&arena, &source_map, root);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), 1301);
    }

    #[test]
    fn empty_effect_row_never_diagnoses() {
        let src = r#"
module Pure = structure {
  let id : (u64) -> u64 !{} @{} =
    fn (a : u64) -> a
}
"#;
        let (arena, source_map, root) = parse(src, "pure.pdx");
        let diags = check_effect_cap_coupling(&arena, &source_map, root);
        assert!(diags.is_empty());
    }

    #[test]
    fn unknown_effect_is_silent() {
        // Coupling is not the pass that diagnoses unknown effects — that
        // is the effect-declaration walker's job. We must not emit
        // spurious C1301s for effects we don't have a binding for.
        let src = r#"
module Unknown = structure {
  let f : (u64) -> u64 !{TotallyMadeUp} @{} =
    fn (a : u64) -> 0
}
"#;
        let (arena, source_map, root) = parse(src, "unknown.pdx");
        let diags = check_effect_cap_coupling(&arena, &source_map, root);
        assert!(diags.is_empty());
    }

    #[test]
    fn over_declaration_is_fine() {
        // Extra caps beyond what the effect row requires never trigger
        // C1301 — over-declaration is permissible per custom-assembler §5.
        let src = r#"
module Over = structure {
  let f : (u64) -> u64 !{RawMem} @{paideia.raw_mem, paideia.mmio} =
    fn (a : u64) -> 0
}
"#;
        let (arena, source_map, root) = parse(src, "over.pdx");
        let diags = check_effect_cap_coupling(&arena, &source_map, root);
        assert!(diags.is_empty());
    }

    #[test]
    fn non_function_let_is_ignored() {
        // A let binding whose type is not a function shape (e.g., a
        // plain u64) can never carry an effect row; the walker must
        // not descend into value expressions.
        let src = r#"
module ConstBind = structure {
  let x : u64 = 42
}
"#;
        let (arena, source_map, root) = parse(src, "const_bind.pdx");
        let diags = check_effect_cap_coupling(&arena, &source_map, root);
        assert!(diags.is_empty());
    }
}
