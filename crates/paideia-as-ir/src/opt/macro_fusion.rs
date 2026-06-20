//! Macro-fusion optimization pass.
//!
//! Detects adjacent (Cmp, Jcc) instruction pairs and marks them as
//! fusion-candidates. Phase-3-m3-007 minimum: emit O1504 diagnostics.
//! Real macro-fusion mechanic (combined encoding) lands when the
//! encoder supports EncodingHint flags (m4 or later).

use super::{OptDiagSink, OptPass};
use crate::IrArena;
use crate::node::IrNodeId;

/// The macro-fusion optimization pass.
pub struct MacroFusionPass;

/// Detect patterns of Cmp followed by Jcc that can be fused.
/// Returns a list of (cmp_idx, jcc_idx) pairs.
pub fn detect_fusion_pairs(ids: &[IrNodeId]) -> Vec<(usize, usize)> {
    let mut pairs = Vec::new();
    for i in 0..ids.len().saturating_sub(1) {
        // Phase-3 minimum: just track consecutive positions.
        // Real detection logic (by Mnemonic) deferred to encoder integration.
        pairs.push((i, i + 1));
    }
    pairs
}

impl OptPass for MacroFusionPass {
    fn name(&self) -> &'static str {
        "macro-fusion"
    }

    fn apply(&self, _arena: &mut IrArena, _root: IrNodeId, sink: &mut OptDiagSink) -> bool {
        sink.emit(
            "macro-fusion",
            "O1504 (would-fire): macro-fusion detection dispatched; real fusion encoding deferred to m4".to_string(),
        );
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_fusion_pairs_empty_returns_empty() {
        let ids = vec![];
        let pairs = detect_fusion_pairs(&ids);
        assert!(pairs.is_empty());
    }

    #[test]
    fn detect_fusion_pairs_single_returns_empty() {
        let ids = vec![IrNodeId::new(1).unwrap()];
        let pairs = detect_fusion_pairs(&ids);
        assert!(pairs.is_empty());
    }

    #[test]
    fn detect_fusion_pairs_two_returns_one() {
        let ids = vec![IrNodeId::new(1).unwrap(), IrNodeId::new(2).unwrap()];
        let pairs = detect_fusion_pairs(&ids);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0], (0, 1));
    }

    #[test]
    fn detect_fusion_pairs_three_returns_two() {
        let ids = vec![
            IrNodeId::new(1).unwrap(),
            IrNodeId::new(2).unwrap(),
            IrNodeId::new(3).unwrap(),
        ];
        let pairs = detect_fusion_pairs(&ids);
        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[0], (0, 1));
        assert_eq!(pairs[1], (1, 2));
    }

    #[test]
    fn macro_fusion_pass_emits_o1504() {
        let pass = MacroFusionPass;
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();

        let dummy_id = IrNodeId::new(1).unwrap();

        let changed = pass.apply(&mut arena, dummy_id, &mut sink);

        assert!(!changed, "MacroFusionPass should return false");
        assert_eq!(sink.diagnostics.len(), 1, "Expected one diagnostic emitted");
        assert_eq!(sink.diagnostics[0].pass, "macro-fusion");
        assert!(
            sink.diagnostics[0]
                .message
                .contains("O1504 (would-fire): macro-fusion detection dispatched")
        );
    }

    #[test]
    fn macro_fusion_pass_diagnostic_mentions_deferral() {
        let pass = MacroFusionPass;
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();

        let dummy_id = IrNodeId::new(1).unwrap();

        pass.apply(&mut arena, dummy_id, &mut sink);

        assert!(
            sink.diagnostics[0]
                .message
                .contains("real fusion encoding deferred to m4"),
            "Diagnostic should document the deferral"
        );
    }
}
