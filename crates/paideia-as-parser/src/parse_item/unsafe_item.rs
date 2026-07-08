//! Unsafe-block item parsing (top-level `unsafe { ... }`).
//! Split out of `parse_item.rs` (2026-07-08).

use paideia_as_ast::{ItemData, NodeId, NodeKind};
use paideia_as_diagnostics::Span;


use crate::parser::{ParseError, Parser};

impl<'tok, 'ast, 'snk> Parser<'tok, 'ast, 'snk> {
    pub(super) fn parse_unsafe_item(&mut self) -> Result<NodeId, ParseError> {
        // The unsafe expression parser is already available via parse_unsafe().
        // We delegate to it and wrap the result.
        let _expr_id = self.parse_unsafe()?;

        // For phase-1, allocate a placeholder UnsafeBlock with empty fields.
        // Later PRs will properly extract unsafe block semantics.
        let unsafe_tok = self
            .peek()
            .map(|t| t.span)
            .unwrap_or_else(|| Span::new(self.file(), 0, 0));

        // Allocate the justification placeholder first to avoid multiple mutable borrows
        let justification = self.arena_mut().alloc(NodeKind::Placeholder, unsafe_tok);

        let item = self.arena_mut().alloc_item(
            NodeKind::UnsafeBlock,
            unsafe_tok,
            ItemData::UnsafeBlock {
                effects: vec![],
                capabilities: vec![],
                justification,
                block: vec![],
            },
        );
        Ok(item)
    }

}
