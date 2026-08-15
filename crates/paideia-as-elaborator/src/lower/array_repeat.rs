//! Array-repeat expression expansion helpers. Split out of `lower.rs` (2026-07-08).
//!
//! `[expr; count]` is lowered to `IrKind::ArrayLit` with N structural children
//! (one per repetition).
//!
//! # Issue #1308 — silent under-allocation
//!
//! Before this fix `extract_repeat_count` was a stub that unconditionally
//! returned `None`, so every `[v; N]` expanded to a *single* child. The
//! declared type `[T; N]` was honoured by the type system but the storage
//! allocator emitted `sizeof(T)` bytes instead of `N * sizeof(T)`, and no
//! diagnostic was raised. Shipping paideia-os symbols (`runqueue`,
//! `_loader_seed_empty_sidecar`) linked 8 bytes short, so every write to
//! index >= 1 landed in whichever symbol the linker placed next.
//!
//! The count is now resolved from the literal's source text (the AST stores
//! integer literals as `Placeholder` nodes carrying only a span, so the value
//! has to be re-read from the `SourceMap` — the same technique used by
//! `unsafe_walker::immediate::extract_integer_from_span` and
//! `cmd_build::layout`). When the count is *not* a resolvable constant the
//! expansion refuses to guess: it emits **P0211** and produces no children, so
//! the build fails loudly rather than under-allocating in silence.

use paideia_as_ast::{AstArena, ExprData, NodeId};
use paideia_as_diagnostics::{
    Category, Diagnostic, DiagnosticCode, DiagnosticSink, Severity, SourceMap, Span,
};

/// Upper bound on repeat expansion.
///
/// `[v; N]` materialises N structural IR children, so an unbounded N is an
/// out-of-memory footgun (`[0; 1 << 40]` would otherwise try to allocate a
/// terabyte of node ids). One million elements covers every realistic kernel
/// table (`[u64; 1024]` frame metadata, `[u8; 4096]` page buffers) with three
/// orders of magnitude of headroom. Anything larger is rejected with P0211
/// rather than attempted.
pub(super) const MAX_REPEAT_COUNT: usize = 1 << 20;

/// Build the P0211 diagnostic code.
fn p0211() -> DiagnosticCode {
    DiagnosticCode::new(Category::P, Severity::Error, 211).expect("P0211 is a valid code")
}

/// Parse an integer literal's source text into a repeat count.
///
/// Accepts decimal, `0x`, `0o` and `0b` forms, `_` digit separators, and the
/// integer type suffixes the lexer permits. Returns `None` for anything that
/// is not a non-negative integer.
fn parse_count_text(text: &str) -> Option<u64> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }

    let (base, digits) =
        if let Some(rest) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
            (16, rest)
        } else if let Some(rest) = text.strip_prefix("0b").or_else(|| text.strip_prefix("0B")) {
            (2, rest)
        } else if let Some(rest) = text.strip_prefix("0o").or_else(|| text.strip_prefix("0O")) {
            (8, rest)
        } else {
            (10, text)
        };

    // Strip an integer type suffix. Longer suffixes first so `u128` is not
    // partially consumed as `u12` + `8`.
    let mut digits = digits;
    for suffix in [
        "usize", "isize", "u128", "i128", "u64", "i64", "u32", "i32", "u16", "i16", "u8", "i8",
    ] {
        if let Some(stripped) = digits.strip_suffix(suffix) {
            digits = stripped;
            break;
        }
    }

    let cleaned: String = digits.chars().filter(|c| *c != '_').collect();
    if cleaned.is_empty() {
        return None;
    }

    u64::from_str_radix(&cleaned, base).ok()
}

/// Read the source text covered by a node's span.
fn span_text<'a>(ast: &AstArena, source_map: &'a SourceMap, node_id: NodeId) -> Option<&'a str> {
    let span = ast.get(node_id)?.span;
    let source = source_map.content(span.file());
    let start = span.byte_start() as usize;
    let end = start.checked_add(span.byte_len() as usize)?;
    if end > source.len() {
        return None;
    }
    source.get(start..end)
}

/// Extract a count literal from an AST expression node.
///
/// Returns `Some(count)` when `count_expr_id` is an integer `ExprLiteral`
/// whose value can be recovered from the source text. Returns `None` for any
/// non-literal count (a named constant, an arithmetic expression, a negative
/// literal — all of which are lowered as non-`Literal` node kinds) so the
/// caller can raise P0211 instead of guessing.
pub(super) fn extract_repeat_count(
    ast: &AstArena,
    source_map: &SourceMap,
    count_expr_id: NodeId,
) -> Option<u64> {
    // Integer literals are `ExprData::Literal { lit }` where `lit` is a
    // Placeholder node carrying the token span; the value itself lives only in
    // the source text.
    let ExprData::Literal { lit } = ast.expr_data(count_expr_id)? else {
        return None;
    };
    parse_count_text(span_text(ast, source_map, *lit)?)
}

/// Expand an array repeat expression to N copies of the element.
///
/// Given `[expr; count]`:
/// 1. Resolve `count` as a constant integer literal.
/// 2. On success, return `count` copies of `expr` as children. The same AST
///    node id is repeated; the second lowering pass maps each occurrence
///    through `ast_to_ir` to the same IR node, which is exactly the semantics
///    a repeat literal wants (one shared element value, N slots).
/// 3. On failure — non-constant count, or a count above [`MAX_REPEAT_COUNT`] —
///    emit P0211 and return no children. Returning empty (rather than one
///    element) guarantees the caller cannot emit an under-allocated symbol:
///    the data pass skips zero-element arrays and the build fails on the
///    diagnostic.
pub(super) fn expand_array_repeat(
    ast: &AstArena,
    source_map: &SourceMap,
    sink: &mut dyn DiagnosticSink,
    span: Span,
    expr: NodeId,
    count: NodeId,
) -> Vec<NodeId> {
    let Some(count_val) = extract_repeat_count(ast, source_map, count) else {
        emit_p0211(
            sink,
            span,
            "array repeat count must be a constant integer literal",
        );
        return Vec::new();
    };

    if count_val == 0 {
        emit_p0211(sink, span, "array repeat count must be greater than zero");
        return Vec::new();
    }

    if count_val > MAX_REPEAT_COUNT as u64 {
        emit_p0211(
            sink,
            span,
            &format!("array repeat count {count_val} exceeds the maximum of {MAX_REPEAT_COUNT}"),
        );
        return Vec::new();
    }

    vec![expr; count_val as usize]
}

/// Emit a P0211 error at `span`.
fn emit_p0211(sink: &mut dyn DiagnosticSink, span: Span, message: &str) {
    let diag = Diagnostic::error(p0211())
        .message(message.to_string())
        .with_span(span)
        .finish();
    let _ = sink.emit(diag);
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_ast::NodeKind;
    use paideia_as_diagnostics::{FileId, VecSink};

    fn setup(source: &str) -> (AstArena, SourceMap, FileId) {
        let mut source_map = SourceMap::new();
        let file = source_map.add_file(std::path::PathBuf::from("t.pdx"), source.to_string());
        (AstArena::new(), source_map, file)
    }

    /// Allocate an `ExprLiteral` whose inner Placeholder spans `[start, start+len)`.
    fn alloc_int_literal(ast: &mut AstArena, file: FileId, start: u32, len: u32) -> NodeId {
        let span = Span::new(file, start, len);
        let lit = ast.alloc(NodeKind::Placeholder, span);
        ast.alloc_expr(NodeKind::ExprLiteral, span, ExprData::Literal { lit })
    }

    #[test]
    fn parse_count_text_handles_every_base_and_separator() {
        assert_eq!(parse_count_text("2"), Some(2));
        assert_eq!(parse_count_text("512"), Some(512));
        assert_eq!(parse_count_text("1_024"), Some(1024));
        assert_eq!(parse_count_text("0x400"), Some(1024));
        assert_eq!(parse_count_text("0o2000"), Some(1024));
        assert_eq!(parse_count_text("0b100"), Some(4));
        assert_eq!(parse_count_text("16u64"), Some(16));
        assert_eq!(parse_count_text("-4"), None);
        assert_eq!(parse_count_text("N"), None);
        assert_eq!(parse_count_text(""), None);
    }

    #[test]
    fn expand_produces_exactly_n_children() {
        // Source is just the count token; the literal spans all of it.
        let (mut ast, source_map, file) = setup("512");
        let count = alloc_int_literal(&mut ast, file, 0, 3);
        let elem = alloc_int_literal(&mut ast, file, 0, 1);
        let mut sink = VecSink::new();

        let children = expand_array_repeat(
            &ast,
            &source_map,
            &mut sink,
            Span::new(file, 0, 3),
            elem,
            count,
        );

        assert_eq!(children.len(), 512, "[v; 512] must expand to 512 children");
        assert!(children.iter().all(|&c| c == elem));
        assert!(
            sink.diagnostics().is_empty(),
            "constant count must not diagnose"
        );
    }

    #[test]
    fn expand_small_count_is_not_special_cased() {
        let (mut ast, source_map, file) = setup("2");
        let count = alloc_int_literal(&mut ast, file, 0, 1);
        let elem = alloc_int_literal(&mut ast, file, 0, 1);
        let mut sink = VecSink::new();

        let children = expand_array_repeat(
            &ast,
            &source_map,
            &mut sink,
            Span::new(file, 0, 1),
            elem,
            count,
        );

        assert_eq!(children.len(), 2, "[v; 2] must expand to 2 children, not 1");
    }

    #[test]
    fn non_constant_count_emits_p0211_and_no_children() {
        // The count node is a Path, not a Literal.
        let (mut ast, source_map, file) = setup("N");
        let span = Span::new(file, 0, 1);
        let seg = ast.alloc(NodeKind::Ident, span);
        let count = ast.alloc_expr(
            NodeKind::ExprPath,
            span,
            ExprData::Path {
                segments: vec![seg],
            },
        );
        let elem = alloc_int_literal(&mut ast, file, 0, 1);
        let mut sink = VecSink::new();

        let children = expand_array_repeat(&ast, &source_map, &mut sink, span, elem, count);

        assert!(
            children.is_empty(),
            "a non-constant count must not silently produce a one-element array"
        );
        assert_eq!(sink.diagnostics().len(), 1);
        assert_eq!(sink.diagnostics()[0].code().to_string(), "P0211");
    }

    #[test]
    fn zero_count_emits_p0211() {
        let (mut ast, source_map, file) = setup("0");
        let count = alloc_int_literal(&mut ast, file, 0, 1);
        let elem = alloc_int_literal(&mut ast, file, 0, 1);
        let mut sink = VecSink::new();

        let children = expand_array_repeat(
            &ast,
            &source_map,
            &mut sink,
            Span::new(file, 0, 1),
            elem,
            count,
        );

        assert!(children.is_empty());
        assert_eq!(sink.diagnostics().len(), 1);
        assert_eq!(sink.diagnostics()[0].code().to_string(), "P0211");
    }

    #[test]
    fn oversized_count_emits_p0211_rather_than_allocating() {
        let text = format!("{}", MAX_REPEAT_COUNT as u64 + 1);
        let len = text.len() as u32;
        let (mut ast, source_map, file) = setup(&text);
        let count = alloc_int_literal(&mut ast, file, 0, len);
        let elem = alloc_int_literal(&mut ast, file, 0, 1);
        let mut sink = VecSink::new();

        let children = expand_array_repeat(
            &ast,
            &source_map,
            &mut sink,
            Span::new(file, 0, len),
            elem,
            count,
        );

        assert!(children.is_empty());
        assert_eq!(sink.diagnostics().len(), 1);
        assert_eq!(sink.diagnostics()[0].code().to_string(), "P0211");
    }
}
