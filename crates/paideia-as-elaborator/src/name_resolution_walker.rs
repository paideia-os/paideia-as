//! Name resolution walker for populating NameResolutionTable.
//!
//! Phase-4-m1-006: Walks the IR tree to record (use_span, def_span) pairs
//! in the NameResolutionTable for all identifier resolutions. This enables
//! LSP definition/references queries to work without re-parsing or re-elaborating.
//!
//! The walker visits every Var node and records its resolution to the Let binding
//! it refers to, using the span information from both nodes.

use std::cell::RefCell;

use paideia_as_diagnostics::Span as DiagSpan;
use paideia_as_ir::{IrArena, IrKind, IrNodeId, IrWalker, WalkerCtx};

use crate::name_resolution::{NameResolutionTable, Span};
use crate::position_index::{ByteOffset, FileId};

/// Trait for name resolution table insertion, allowing walkers to record identifier resolutions.
pub trait NameResolutionTableWriter {
    /// Get the current file ID.
    fn file_id(&self) -> FileId;

    /// Record a name resolution: a use site that resolves to a definition site.
    fn record_resolution(&self, use_site: Span, def_site: Span);
}

/// Concrete implementation of NameResolutionTableWriter using RefCell for interior mutability.
pub struct NameResolutionPassState {
    file_id: FileId,
    name_resolution_table: RefCell<NameResolutionTable>,
}

impl NameResolutionPassState {
    /// Create a new pass state for the given file. The name resolution table is
    /// created empty and populated during the name resolution walker pass.
    pub fn new(file_id: FileId) -> Self {
        Self {
            file_id,
            name_resolution_table: RefCell::new(NameResolutionTable::new()),
        }
    }

    /// Consume this pass state and extract the finalized NameResolutionTable.
    pub fn into_name_resolution_table(self) -> NameResolutionTable {
        self.name_resolution_table.into_inner()
    }

    /// Get a reference to the current NameResolutionTable (for inspection).
    pub fn name_resolution_table(&self) -> std::cell::Ref<'_, NameResolutionTable> {
        self.name_resolution_table.borrow()
    }
}

impl NameResolutionTableWriter for NameResolutionPassState {
    fn file_id(&self) -> FileId {
        self.file_id
    }

    fn record_resolution(&self, use_site: Span, def_site: Span) {
        self.name_resolution_table
            .borrow_mut()
            .record(use_site, def_site);
    }
}

/// IR-walker implementation for name resolution table population.
///
/// Walks the IR tree to populate the NameResolutionTable with all identifier
/// resolutions. For each Var node, records the mapping from use site to
/// definition site.
///
/// ## Symbol Proxy Strategy (Phase-2-m1)
///
/// This walker uses **node IDs as symbol proxies**: each `Let` node ID serves
/// as the symbol bound by that Let, and each `Var` node's reference target
/// is the ID of the Let it consumes. The Var node is created with the referent
/// Let's ID during AST lowering (see lower.rs).
///
/// When phase-3 (m2/m5) adds real symbol names to the IR, this walker will
/// switch to real symbol lookup via those payloads.
#[derive(Debug)]
pub struct NameResolutionWalker {
    /// Stack of Let bindings: (Let node ID, Let span).
    /// When entering a scope (Module, Let, Lambda), we push the binding.
    /// When leaving, we pop.
    let_binding_stack: Vec<(IrNodeId, Span)>,
}

impl NameResolutionWalker {
    /// Construct a new walker.
    #[must_use]
    pub fn new() -> Self {
        Self {
            let_binding_stack: Vec::new(),
        }
    }

    /// Record a Let binding on entry.
    fn push_let_binding(&mut self, let_id: IrNodeId, let_span: Span) {
        self.let_binding_stack.push((let_id, let_span));
    }

    /// Record that a Var node uses a binding.
    fn record_var_use(&self, var_id: IrNodeId, var_span: &DiagSpan, ctx: &mut WalkerCtx<'_>) {
        // The Var's ID is the referent Let's ID (phase-2-m1 assumption).
        // Find the corresponding Let span in our stack.
        if let Some((_let_id, def_span)) = self
            .let_binding_stack
            .iter()
            .rev()
            .find(|(let_id, _)| *let_id == var_id)
        {
            // Record the resolution if we have access to the writer.
            if let Some(writer) = ctx.pass_state::<NameResolutionPassState>() {
                let use_site = Span {
                    file: writer.file_id(),
                    start: ByteOffset(var_span.byte_start()),
                    end: ByteOffset(var_span.byte_end()),
                };
                let def_site = *def_span;
                writer.record_resolution(use_site, def_site);
            }
        }
    }
}

impl Default for NameResolutionWalker {
    fn default() -> Self {
        Self::new()
    }
}

impl IrWalker for NameResolutionWalker {
    fn pre_visit(
        &mut self,
        id: IrNodeId,
        node: &paideia_as_ir::IrNodeData,
        _arena: &IrArena,
        ctx: &mut WalkerCtx<'_>,
    ) {
        match node.kind {
            IrKind::Let => {
                // Record this Let binding for future Var lookups.
                // Get the file ID from the writer if available, otherwise use 0.
                let file_id = ctx
                    .pass_state::<NameResolutionPassState>()
                    .map(|w| w.file_id())
                    .unwrap_or(FileId(0));
                let span = Span {
                    file: file_id,
                    start: ByteOffset(node.span.byte_start()),
                    end: ByteOffset(node.span.byte_end()),
                };
                self.push_let_binding(id, span);
            }
            _ => {}
        }
    }

    fn post_visit(
        &mut self,
        id: IrNodeId,
        node: &paideia_as_ir::IrNodeData,
        _arena: &IrArena,
        ctx: &mut WalkerCtx<'_>,
    ) {
        match node.kind {
            IrKind::Var => {
                // Record this variable use against its Let binding.
                self.record_var_use(id, &node.span, ctx);
            }
            IrKind::Let => {
                // Pop the Let binding when leaving its scope.
                // This is a simplified model; real scope tracking would be more complex.
                if !self.let_binding_stack.is_empty() {
                    self.let_binding_stack.pop();
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_resolution_walker_creates_new() {
        let walker = NameResolutionWalker::new();
        assert!(walker.let_binding_stack.is_empty());
    }

    #[test]
    fn name_resolution_pass_state_creates_empty_table() {
        let file_id = FileId(1);
        let state = NameResolutionPassState::new(file_id);
        assert_eq!(state.file_id(), file_id);
        let table = state.name_resolution_table();
        assert_eq!(table.use_count(), 0);
        assert_eq!(table.definition_count(), 0);
    }

    #[test]
    fn name_resolution_pass_state_extracts_table() {
        let file_id = FileId(1);
        let state = NameResolutionPassState::new(file_id);
        {
            let mut table = state.name_resolution_table.borrow_mut();
            let use_site = Span {
                file: file_id,
                start: ByteOffset(10),
                end: ByteOffset(13),
            };
            let def_site = Span {
                file: file_id,
                start: ByteOffset(0),
                end: ByteOffset(3),
            };
            table.record(use_site, def_site);
        }
        let final_table = state.into_name_resolution_table();
        assert_eq!(final_table.use_count(), 1);
        assert_eq!(final_table.definition_count(), 1);
    }
}
