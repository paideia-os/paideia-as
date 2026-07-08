//! Immediate operand + integer-literal extraction.
//! Split out of `unsafe_walker.rs` (2026-07-08).
//!
//! `extract_integer_from_span` is `pub(crate)` — it is re-exported from
//! `unsafe_walker` so `lower/match_dispatch.rs` can keep the old
//! `crate::unsafe_walker::extract_integer_from_span` import path.

use paideia_as_ast::{AstArena, ExprData, NodeId, NodeKind};
use paideia_as_ir::instruction::Operand;

use super::OperandError;

/// Parse an immediate operand from an ExprLiteral node.
pub(super) fn parse_immediate_from_literal(
    ast: &AstArena,
    literal_node: NodeId,
    source_map: &paideia_as_diagnostics::SourceMap,
) -> Result<Operand, OperandError> {
    let span = ast.get(literal_node).map(|n| n.span).unwrap_or_else(|| {
        paideia_as_diagnostics::Span::new(paideia_as_diagnostics::FileId::new(1).unwrap(), 0, 1)
    });

    // For phase-1, we assume all literals are already interned as i64 values.
    // The parser's literal interning (paideia-as-parser) handles the conversion.
    // We extract the value by looking at the AST structure.
    match ast.expr_data(literal_node) {
        Some(ExprData::Literal { lit }) => {
            // The `lit` node is a Placeholder holding the literal value.
            // For phase-1, we assume the parser has already validated the literal.
            // Extract the integer value from the source span or use a default.
            let value = extract_integer_from_span(ast, *lit, source_map).unwrap_or(0);
            Ok(Operand::Imm64(value))
        }
        _ => Err(OperandError::MalformedOperand(span)),
    }
}

/// Extract an integer value from a span/literal node.
///
/// PA10-006f: Parses integer literals from their source text representation.
/// Supports decimal, hexadecimal (0x), octal (0o), and binary (0b) formats.
/// Returns the parsed u64 value, or None if parsing fails.
pub(crate) fn extract_integer_from_span(
    ast: &AstArena,
    literal_node: NodeId,
    source_map: &paideia_as_diagnostics::SourceMap,
) -> Option<i64> {
    // Get the span of the literal node
    let node = ast.get(literal_node)?;
    let span = node.span;

    // Look up the file content in the source map
    let file_id = span.file();
    let source = source_map.content(file_id);

    // Extract the text from the span
    let start = span.byte_start() as usize;
    let end = start + span.byte_len() as usize;
    if end > source.len() {
        return None;
    }

    let text = &source[start..end];

    // Parse the integer literal
    // Support decimal, hex (0x), octal (0o), and binary (0b) formats
    if let Ok(val) = text.parse::<i64>() {
        return Some(val);
    }

    // Try parsing as hex (u64-first so top-bit-set values survive)
    if text.starts_with("0x") || text.starts_with("0X") {
        if let Ok(val) = u64::from_str_radix(&text[2..], 16) {
            return Some(val as i64);
        }
    }

    // Try parsing as octal
    if text.starts_with("0o") || text.starts_with("0O") {
        if let Ok(val) = u64::from_str_radix(&text[2..], 8) {
            return Some(val as i64);
        }
    }

    // Try parsing as binary
    if text.starts_with("0b") || text.starts_with("0B") {
        if let Ok(val) = u64::from_str_radix(&text[2..], 2) {
            return Some(val as i64);
        }
    }

    // Try parsing as unsigned and converting to signed
    if let Ok(val) = text.parse::<u64>() {
        return Some(val as i64);
    }

    None
}

/// Helper to extract an integer literal value from an ExprLiteral node.
/// Returns None if the node is not an ExprLiteral or the literal cannot be parsed.
pub(super) fn try_extract_integer_literal(
    ast: &AstArena,
    node_id: NodeId,
    source_map: &paideia_as_diagnostics::SourceMap,
) -> Option<i64> {
    let node = ast.get(node_id)?;

    match node.kind {
        NodeKind::ExprLiteral => match ast.expr_data(node_id) {
            Some(ExprData::Literal { lit }) => extract_integer_from_span(ast, *lit, source_map),
            _ => None,
        },
        _ => None,
    }
}
