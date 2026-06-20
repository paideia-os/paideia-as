//! Pass composition + ordering verification.
//!
//! Ensures that multiple pass annotations compose in canonical catalog
//! order regardless of annotation order. Provides both deterministic
//! ordering verification and property-based randomness testing.

use std::collections::BTreeSet;

use super::{OptDiagSink, dispatch::canonical_catalog};
use crate::IrArena;
use crate::node::IrNodeId;

/// Run all requested passes in catalog order; return the sequence of
/// invoked pass names (for ordering verification).
pub fn dispatch_collecting_order(
    arena: &mut IrArena,
    function_root: IrNodeId,
    requested: &BTreeSet<String>,
) -> Vec<String> {
    let catalog = canonical_catalog();
    let mut sink = OptDiagSink::new();
    let mut invoked = Vec::new();
    for pass in &catalog {
        if requested.contains(pass.name()) {
            pass.apply(arena, function_root, &mut sink);
            invoked.push(pass.name().to_string());
        }
    }
    invoked
}

/// The canonical pass-name ordering. Should be invariant across
/// catalog modifications (new passes inserted at the documented
/// catalog position).
pub fn canonical_pass_order() -> Vec<String> {
    canonical_catalog()
        .iter()
        .map(|p| p.name().to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (IrArena, IrNodeId) {
        let arena = IrArena::new();
        // Use a dummy IrNodeId; the passes don't access the arena in current phase.
        let root = IrNodeId::new(1).unwrap();
        (arena, root)
    }

    #[test]
    fn dispatch_runs_in_catalog_order_regardless_of_annotation_order() {
        let (mut arena, root) = setup();
        // Build BTreeSet — its natural ordering is alphabetical, NOT
        // catalog order. Verify dispatch still walks catalog order.
        let requested: BTreeSet<String> = ["peephole", "schedule", "tailcall"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let invoked = dispatch_collecting_order(&mut arena, root, &requested);

        // Verify that despite BTreeSet alphabetical order, catalog order is preserved:
        // peephole, schedule, tailcall in catalog, so order must be maintained.
        let canonical = canonical_pass_order();
        let peephole_pos = canonical.iter().position(|n| n == "peephole").unwrap();
        let schedule_pos = canonical.iter().position(|n| n == "schedule").unwrap();
        let tailcall_pos = canonical.iter().position(|n| n == "tailcall").unwrap();

        assert!(
            peephole_pos < schedule_pos && schedule_pos < tailcall_pos,
            "test setup: passes must be in catalog order"
        );

        // Now verify invoked order matches catalog order.
        assert_eq!(
            invoked,
            vec!["peephole", "schedule", "tailcall"],
            "dispatch must invoke passes in catalog order, not BTreeSet order"
        );
    }

    #[test]
    fn canonical_pass_order_contains_expected_names() {
        let order = canonical_pass_order();
        assert!(
            order.contains(&"noop".to_string()),
            "canonical order should contain noop"
        );
        assert!(
            order.contains(&"peephole".to_string()),
            "canonical order should contain peephole"
        );
        assert!(
            order.contains(&"schedule".to_string()),
            "canonical order should contain schedule"
        );
        assert!(
            order.contains(&"macro-fusion".to_string()),
            "canonical order should contain macro-fusion"
        );
        assert!(
            order.contains(&"align".to_string()),
            "canonical order should contain align"
        );
    }

    #[test]
    fn dispatch_with_empty_request_invokes_nothing() {
        let (mut arena, root) = setup();
        let invoked = dispatch_collecting_order(&mut arena, root, &BTreeSet::new());
        assert!(invoked.is_empty(), "empty request should invoke no passes");
    }

    #[test]
    fn dispatch_with_unknown_pass_name_invokes_nothing() {
        let (mut arena, root) = setup();
        let requested: BTreeSet<String> =
            ["definitely-not-a-pass".to_string()].into_iter().collect();
        let invoked = dispatch_collecting_order(&mut arena, root, &requested);
        assert!(
            invoked.is_empty(),
            "unknown pass names should not be invoked"
        );
    }

    #[test]
    fn invoked_subset_respects_catalog_order() {
        let (mut arena, root) = setup();
        let requested: BTreeSet<String> =
            ["unroll", "noop"].iter().map(|s| s.to_string()).collect();
        let invoked = dispatch_collecting_order(&mut arena, root, &requested);
        let catalog = canonical_pass_order();

        // Verify every invoked pass is in the catalog.
        for name in &invoked {
            assert!(
                catalog.contains(name),
                "invoked pass {} not in catalog",
                name
            );
        }

        // Verify invoked passes maintain catalog order.
        for (i, name) in invoked.iter().enumerate() {
            let pos = catalog
                .iter()
                .position(|n| n == name)
                .expect("pass must be in catalog");
            if i > 0 {
                let prev_name = &invoked[i - 1];
                let prev_pos = catalog.iter().position(|n| n == prev_name).unwrap();
                assert!(
                    prev_pos < pos,
                    "invoked passes must maintain catalog order: {} before {}",
                    prev_name,
                    name
                );
            }
        }
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    fn setup() -> (IrArena, IrNodeId) {
        let arena = IrArena::new();
        let root = IrNodeId::new(1).unwrap();
        (arena, root)
    }

    proptest! {
        #[test]
        fn random_annotation_sequences_produce_stable_output(
            indices in proptest::collection::vec(0usize..20, 0..10)
        ) {
            // Pick random pass indices, dedup, dispatch, assert no panic
            // and the invoked order is consistent with catalog order.
            let catalog = canonical_pass_order();
            let requested: BTreeSet<String> = indices
                .into_iter()
                .filter_map(|i| catalog.get(i % catalog.len()).cloned())
                .collect();
            let (mut arena, root) = setup();
            let invoked = dispatch_collecting_order(&mut arena, root, &requested);

            // Invariant: every invoked name appears in catalog.
            for name in &invoked {
                prop_assert!(
                    catalog.contains(name),
                    "invoked name {} not in catalog",
                    name
                );
            }

            // Invariant: invoked names strictly increase in catalog position.
            for w in invoked.windows(2) {
                let p0 = catalog
                    .iter()
                    .position(|n| n == &w[0])
                    .unwrap();
                let p1 = catalog
                    .iter()
                    .position(|n| n == &w[1])
                    .unwrap();
                prop_assert!(
                    p0 < p1,
                    "invoked passes must be in catalog order: {} at {} before {} at {}",
                    &w[0],
                    p0,
                    &w[1],
                    p1
                );
            }
        }
    }
}
