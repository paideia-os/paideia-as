//! Alignment optimization pass.
//!
//! Inserts `.align 16` directives before loop-entry branches.
//! Phase-3-m3-007 minimum: emit O1508 diagnostics per alignment site.
//! Real alignment directive insertion lands at the emit stage (m4 or later).

use super::{OptDiagSink, OptPass};
use crate::IrArena;
use crate::node::IrNodeId;

/// The alignment optimization pass.
pub struct AlignPass;

/// Detect loop-entry instructions that should be aligned.
/// Phase-3 minimum: returns indices of instruction sites.
/// Real loop detection deferred to post-m3.
pub fn detect_alignment_sites(ids: &[IrNodeId]) -> Vec<usize> {
    let mut sites = Vec::new();
    for (i, _id) in ids.iter().enumerate() {
        // Phase-3 minimum: heuristic — every jump could be a loop entry.
        // Real loop detection (via CFG analysis) lands at m4.
        sites.push(i);
    }
    sites
}

impl OptPass for AlignPass {
    fn name(&self) -> &'static str {
        "align"
    }

    fn apply(&self, _arena: &mut IrArena, _root: IrNodeId, sink: &mut OptDiagSink) -> bool {
        sink.emit(
            "align",
            "O1508 (would-fire): alignment directive emission dispatched; real insertion deferred to m4".to_string(),
        );
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_alignment_sites_empty_returns_empty() {
        let ids = vec![];
        let sites = detect_alignment_sites(&ids);
        assert!(sites.is_empty());
    }

    #[test]
    fn detect_alignment_sites_one_returns_one() {
        let ids = vec![IrNodeId::new(1).unwrap()];
        let sites = detect_alignment_sites(&ids);
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0], 0);
    }

    #[test]
    fn detect_alignment_sites_three_returns_three() {
        let ids = vec![
            IrNodeId::new(1).unwrap(),
            IrNodeId::new(2).unwrap(),
            IrNodeId::new(3).unwrap(),
        ];
        let sites = detect_alignment_sites(&ids);
        assert_eq!(sites.len(), 3);
        assert_eq!(sites[0], 0);
        assert_eq!(sites[1], 1);
        assert_eq!(sites[2], 2);
    }

    #[test]
    fn align_pass_emits_o1508() {
        let pass = AlignPass;
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();

        let dummy_id = IrNodeId::new(1).unwrap();

        let changed = pass.apply(&mut arena, dummy_id, &mut sink);

        assert!(!changed, "AlignPass should return false");
        assert_eq!(sink.diagnostics.len(), 1, "Expected one diagnostic emitted");
        assert_eq!(sink.diagnostics[0].pass, "align");
        assert!(
            sink.diagnostics[0]
                .message
                .contains("O1508 (would-fire): alignment directive emission dispatched")
        );
    }

    #[test]
    fn align_pass_diagnostic_mentions_deferral() {
        let pass = AlignPass;
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();

        let dummy_id = IrNodeId::new(1).unwrap();

        pass.apply(&mut arena, dummy_id, &mut sink);

        assert!(
            sink.diagnostics[0]
                .message
                .contains("real insertion deferred to m4"),
            "Diagnostic should document the deferral"
        );
    }
}
