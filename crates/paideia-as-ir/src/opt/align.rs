//! Alignment optimization pass.
//!
//! Detects loop-entry candidates and emits alignment markers.
//! Phase-4-m1-009 (flip apply): detects Loop nodes via LoopMetaTable and emits markers.
//! Real `.align 16` directive insertion lands at the emit stage (m2+ follow-up).

use super::{OptDiagSink, OptPass};
use crate::IrArena;
use crate::node::{IrKind, IrNodeId};

/// The alignment optimization pass.
pub struct AlignPass;

/// Detect loop-entry candidates in the IR.
/// Phase-4-m1-009: walks the arena and identifies all Loop nodes via LoopMetaTable.
/// Returns a Vec of Loop node IDs that are candidates for alignment.
pub fn detect_alignment_sites(arena: &IrArena) -> Vec<IrNodeId> {
    let mut sites = Vec::new();

    // Collect all Loop nodes from the arena.
    // Phase-4-m1-009: check arena for Loop IR nodes with loop metadata.
    for node_id in 1..=(arena.len() as u32) {
        if let Some(id) = IrNodeId::new(node_id) {
            if let Some(node) = arena.get(id) {
                // If this is a Loop node and it has metadata, it's an alignment candidate.
                if node.kind == IrKind::Loop && arena.loop_meta().get(id).is_some() {
                    sites.push(id);
                }
            }
        }
    }

    sites
}

impl OptPass for AlignPass {
    fn name(&self) -> &'static str {
        "align"
    }

    fn apply(&self, arena: &mut IrArena, _root: IrNodeId, sink: &mut OptDiagSink) -> bool {
        // Phase-4-m1-009: detect loop-entry candidates using LoopMetaTable.
        let sites = detect_alignment_sites(arena);

        if !sites.is_empty() {
            // Emit O1508 diagnostic with site count.
            sink.emit("align", format!("O1508 rewrote {} sites", sites.len()));
            // Phase-4-m1-009 minimum: emit marker; actual directive insertion at m2+ follow-up.
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loop_meta::LoopMeta;
    use paideia_as_diagnostics::{FileId, Span};

    fn create_test_arena() -> (IrArena, IrNodeId) {
        let mut arena = IrArena::new();
        let file = FileId::new(1).unwrap();
        let span = Span::new(file, 0, 10);
        let func = arena.alloc(IrKind::Functor, span);
        (arena, func)
    }

    #[test]
    fn align_detects_loop_entry() {
        let (mut arena, _func) = create_test_arena();
        let mut sink = OptDiagSink::new();

        // Create a Loop node with metadata
        let file = FileId::new(1).unwrap();
        let span = Span::new(file, 0, 5);
        let loop_id = arena.alloc(IrKind::Loop, span);

        // Add loop metadata
        let meta = LoopMeta {
            entry_label: 100,
            exit_label: 200,
        };
        arena.loop_meta_mut().insert(loop_id, meta);

        // Run the pass
        let pass = AlignPass;
        let changed = pass.apply(&mut arena, loop_id, &mut sink);

        assert!(
            changed,
            "AlignPass should detect loop entry and return true"
        );
        assert_eq!(sink.diagnostics.len(), 1);
        assert_eq!(sink.diagnostics[0].pass, "align");
        assert!(
            sink.diagnostics[0]
                .message
                .contains("O1508 rewrote 1 sites"),
            "Diagnostic should report 1 aligned site"
        );
    }

    #[test]
    fn align_emits_o1508_per_loop() {
        let (mut arena, _func) = create_test_arena();
        let mut sink = OptDiagSink::new();

        let file = FileId::new(1).unwrap();
        let span = Span::new(file, 0, 5);

        // Create 3 Loop nodes with metadata
        let loop1_id = arena.alloc(IrKind::Loop, span);
        let meta1 = LoopMeta {
            entry_label: 100,
            exit_label: 200,
        };
        arena.loop_meta_mut().insert(loop1_id, meta1);

        let loop2_id = arena.alloc(IrKind::Loop, span);
        let meta2 = LoopMeta {
            entry_label: 110,
            exit_label: 210,
        };
        arena.loop_meta_mut().insert(loop2_id, meta2);

        let loop3_id = arena.alloc(IrKind::Loop, span);
        let meta3 = LoopMeta {
            entry_label: 120,
            exit_label: 220,
        };
        arena.loop_meta_mut().insert(loop3_id, meta3);

        let pass = AlignPass;
        let changed = pass.apply(&mut arena, loop1_id, &mut sink);

        assert!(changed, "AlignPass should detect 3 Loop nodes");
        assert_eq!(sink.diagnostics.len(), 1);
        assert!(
            sink.diagnostics[0]
                .message
                .contains("O1508 rewrote 3 sites"),
            "Diagnostic should report 3 aligned sites"
        );
    }

    #[test]
    fn align_skips_non_loop_blocks() {
        let (mut arena, func) = create_test_arena();
        let mut sink = OptDiagSink::new();

        let file = FileId::new(1).unwrap();
        let span = Span::new(file, 0, 5);

        // Create non-Loop nodes (e.g., Load, Store, Var)
        let _load_id = arena.alloc(IrKind::Load, span);
        let _store_id = arena.alloc(IrKind::Store, span);
        let _var_id = arena.alloc(IrKind::Var, span);

        // Run the pass
        let pass = AlignPass;
        let changed = pass.apply(&mut arena, func, &mut sink);

        assert!(!changed, "AlignPass should not rewrite non-loop blocks");
        assert_eq!(sink.diagnostics.len(), 0);
    }
}
