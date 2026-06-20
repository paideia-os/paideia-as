//! Annotation-driven pass dispatcher.

use std::collections::BTreeSet;

use super::{
    BranchHintPass, DsePass, InstructionSchedulingPass, NoOpPass, OptDiagSink, OptPass,
    PeepholePass, PoolConstantsPass,
};
use crate::IrArena;
use crate::node::IrNodeId;

/// Parse a pass-name annotation list. Annotation form (phase-2-m9-001):
/// "#[peephole, dse, unroll(4)]" → BTreeSet { "peephole", "dse", "unroll" }.
/// Argument lists like "unroll(4)" are accepted but the arg is dropped
/// here (individual passes parse their own args at invocation time).
pub fn parse_annotations(annotation: &str) -> BTreeSet<String> {
    let s = annotation.trim();
    let s = s.trim_start_matches("#[").trim_end_matches("]");
    s.split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|p| {
            if let Some(idx) = p.find('(') {
                p[..idx].trim().to_string()
            } else {
                p.to_string()
            }
        })
        .collect()
}

/// The canonical pass catalog. Passes run in this order when requested.
/// Each entry is a boxed pass.
pub fn canonical_catalog() -> Vec<Box<dyn OptPass>> {
    vec![
        Box::new(NoOpPass),
        Box::new(PeepholePass),              // m9-002
        Box::new(InstructionSchedulingPass), // m9-003
        Box::new(DsePass),                   // m9-005
        Box::new(BranchHintPass),            // m9-007
        Box::new(PoolConstantsPass),         // m9-007
    ]
}

/// Dispatch: walks the catalog in order; invokes only passes whose name
/// appears in `requested`. Returns the total number of changes (sum of
/// pass.apply true returns).
pub fn dispatch(
    arena: &mut IrArena,
    function_root: IrNodeId,
    requested: &BTreeSet<String>,
    sink: &mut OptDiagSink,
) -> usize {
    let catalog = canonical_catalog();
    let mut changes = 0usize;
    for pass in &catalog {
        if requested.contains(pass.name()) && pass.apply(arena, function_root, sink) {
            changes += 1;
        }
    }
    changes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_annotations_empty_returns_empty_set() {
        let result = parse_annotations("#[]");
        assert!(result.is_empty());
    }

    #[test]
    fn parse_annotations_parses_three_names() {
        let result = parse_annotations("#[peephole, dse, unroll(4)]");
        assert_eq!(result.len(), 3);
        assert!(result.contains("peephole"));
        assert!(result.contains("dse"));
        assert!(result.contains("unroll"));
    }

    #[test]
    fn parse_annotations_handles_whitespace() {
        let result = parse_annotations("  #[  peephole  ,  dse  ]  ");
        assert_eq!(result.len(), 2);
        assert!(result.contains("peephole"));
        assert!(result.contains("dse"));
    }

    #[test]
    fn dispatch_runs_only_requested_passes() {
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();
        let dummy_id = IrNodeId::new(1).unwrap();

        let mut requested = BTreeSet::new();
        requested.insert("noop".to_string());

        let changes = dispatch(&mut arena, dummy_id, &requested, &mut sink);

        // NoOpPass always returns false, so no changes.
        assert_eq!(changes, 0);
        // Sink should remain empty (NoOpPass doesn't emit).
        assert!(sink.diagnostics.is_empty());
    }

    #[test]
    fn dispatch_skips_unrequested_passes() {
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();
        let dummy_id = IrNodeId::new(1).unwrap();

        let mut requested = BTreeSet::new();
        requested.insert("peephole".to_string()); // Not in catalog

        let changes = dispatch(&mut arena, dummy_id, &requested, &mut sink);

        // No passes matched, so no changes.
        assert_eq!(changes, 0);
    }
}
