//! Auto-detect dense enum-variant matches and inject MatchDispatchMeta.
//!
//! When a match on an enum has ≥4 arms covering strictly-consecutive
//! discriminants (and no payload binders / nested patterns), inject the
//! same side-table entries `@jump_table` would produce. Downstream
//! codegen naturally uses the existing jump-table dispatch path.
//!
//! Issue #1052: dense-variant jump-table synthesis in visit_match via meta injection.

use paideia_as_ir::{IrArena, IrKind, IrNodeId, MatchDispatchMeta};

/// Auto-detect dense enum-variant matches and inject MatchDispatchMeta.
///
/// This pass runs AFTER `populate_match_arm_meta` (which populates
/// MatchArmMeta with variant_index for each arm). For every Match IR node:
///
/// 1. Skip if match_dispatch_meta already populated (user @jump_table).
/// 2. Skip if match_scrutinee_table entry missing (not an enum match).
/// 3. Collect variant_index from MatchArmMeta for non-default arms.
/// 4. Skip if any non-default arm has payload_binder or pattern_binding.
/// 5. Require ≥4 covered arms AND covered_arms == (max - min + 1).
///    Strict-consecutive, not just dense.
/// 6. Inject MatchDispatchMeta { jump_table: true, min_arm: min,
///    range, covered_arms, density_ok: true } and
///    match_jump_table_arm_values: Vec<(variant_index, arm_idx)>.
pub(super) fn populate_auto_jump_table_meta(ir: &mut IrArena) {
    // Collect Match node IDs first (to avoid borrow conflicts).
    let mut match_node_ids = Vec::new();
    for node_idx in 0..ir.len() {
        let node_id = IrNodeId::new((node_idx + 1) as u32);
        if let Some(node_id) = node_id {
            if let Some(node) = ir.get(node_id) {
                if node.kind == IrKind::Match {
                    match_node_ids.push(node_id);
                }
            }
        }
    }

    // For each Match node, attempt to inject auto-dispatch metadata.
    for match_node_id in match_node_ids {
        process_match_for_auto_dispatch(ir, match_node_id);
    }
}

/// Process a single Match node to determine if auto-dispatch should be injected.
fn process_match_for_auto_dispatch(ir: &mut IrArena, match_node_id: IrNodeId) {
    // Skip if already has explicit @jump_table metadata (user-annotated).
    if ir.match_dispatch_meta().contains_key(match_node_id) {
        return;
    }

    // Skip if not an enum match (no scrutinee type registered).
    if !ir.match_scrutinee_table().contains_key(match_node_id) {
        return;
    }

    // Get the children (scrutinee + arms).
    let children = ir.children(match_node_id).to_vec();
    if children.is_empty() {
        return;
    }

    // Skip the scrutinee (children[0]); collect metadata for arms.
    let arm_ids: Vec<IrNodeId> = children[1..].to_vec();
    if arm_ids.is_empty() {
        return;
    }

    // Collect (variant_index, arm_idx) for non-default arms.
    let mut variant_arms: Vec<(u32, u32)> = Vec::new();
    let mut has_invalid_arm = false;

    for (arm_idx, &arm_id) in arm_ids.iter().enumerate() {
        if let Some(arm_meta) = ir.match_arm_meta().get(arm_id) {
            // Skip default/wildcard arms.
            if arm_meta.is_default {
                continue;
            }

            // Skip if arm has payload binder (e.g., `Ok(x)`).
            if arm_meta.payload_binder.is_some() {
                has_invalid_arm = true;
                break;
            }

            // Skip if arm has nested pattern binding (e.g., record destructure).
            if arm_meta.pattern_binding.is_some() {
                has_invalid_arm = true;
                break;
            }

            // Skip if arm has guard (guards can't be expressed in jump-table dispatch).
            if arm_meta.guard.is_some() {
                has_invalid_arm = true;
                break;
            }

            // Collect variant_index if present.
            if let Some(variant_idx) = arm_meta.variant_index {
                variant_arms.push((variant_idx, arm_idx as u32));
            }
        }
    }

    // Bail if any arm has payload binder or nested pattern.
    if has_invalid_arm {
        return;
    }

    // Require ≥4 covered arms.
    if variant_arms.len() < 4 {
        return;
    }

    // Extract variant indices and check for strict-consecutive coverage.
    let mut variant_indices: Vec<u32> = variant_arms.iter().map(|(v, _)| *v).collect();
    variant_indices.sort_unstable();
    variant_indices.dedup();

    // Strict-consecutive: covered_arms == (max - min + 1).
    let min_idx = *variant_indices.first().unwrap();
    let max_idx = *variant_indices.last().unwrap();
    let range = (max_idx - min_idx + 1) as u32;
    let covered_arms = variant_indices.len() as u32;

    if covered_arms != range {
        // Not strictly consecutive; bail.
        return;
    }

    // Build arm_value_index_pairs for rodata synthesis (sorted by value).
    let mut arm_value_index_pairs: Vec<(i64, u32)> = variant_arms
        .iter()
        .map(|(variant_idx, arm_idx)| (*variant_idx as i64, *arm_idx))
        .collect();
    arm_value_index_pairs.sort_by_key(|(v, _)| *v);

    // Inject MatchDispatchMeta with density_ok=true (strict-consecutive).
    let meta = MatchDispatchMeta {
        jump_table: true,
        min_arm: min_idx as i64,
        range,
        covered_arms,
        density_ok: true,
    };

    ir.match_dispatch_meta_mut()
        .insert(match_node_id, meta);
    ir.match_jump_table_arm_values_mut()
        .insert(match_node_id, arm_value_index_pairs);
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::{FileId, Span};
    use paideia_as_ir::{MatchArmMeta, PatternBinding, EnumTypeId};

    /// Helper to create a test arena with a Match node and arms.
    fn create_test_match(
        num_arms: usize,
        variant_indices: Vec<Option<u32>>,
        has_payload_binder: bool,
        has_pattern_binding: bool,
    ) -> (IrArena, IrNodeId) {
        let mut ir = IrArena::new();
        let file_id = FileId::new(1).unwrap();
        let span = Span::new(file_id, 0, 1);

        // Allocate scrutinee (dummy Var node).
        let scrutinee_id = ir.alloc(IrKind::Var, span);

        // Allocate arm nodes and metadata.
        let mut arm_ids = Vec::new();
        for i in 0..num_arms {
            let arm_id = ir.alloc(IrKind::Action, span);
            arm_ids.push(arm_id);

            let mut meta = MatchArmMeta::default();
            if i < variant_indices.len() {
                meta.variant_index = variant_indices[i];
            }
            if has_payload_binder && i == 0 {
                meta.payload_binder = Some("x".to_string());
            }
            if has_pattern_binding && i == 0 {
                meta.pattern_binding = Some(PatternBinding::Wildcard);
            }

            ir.match_arm_meta_mut().insert(arm_id, meta);
        }

        // Allocate Match node with scrutinee + arms as children.
        let mut match_children = vec![scrutinee_id];
        match_children.extend(arm_ids);
        let match_id = ir.alloc_with_children(IrKind::Match, span, match_children);

        // Register in match_scrutinee_table (to mark as enum match).
        ir.match_scrutinee_table_mut()
            .insert(match_id, EnumTypeId(0));

        (ir, match_id)
    }

    #[test]
    fn dense_4_variants_injects_meta() {
        let (mut ir, match_id) = create_test_match(
            4,
            vec![Some(0), Some(1), Some(2), Some(3)],
            false,
            false,
        );

        populate_auto_jump_table_meta(&mut ir);

        let meta = ir.match_dispatch_meta().get(match_id);
        assert!(meta.is_some(), "Meta should be injected for 4 consecutive variants");
        let meta = meta.unwrap();
        assert!(meta.jump_table, "jump_table should be true");
        assert_eq!(meta.covered_arms, 4, "covered_arms should be 4");
        assert_eq!(meta.range, 4, "range should be 4");
        assert!(meta.density_ok, "density_ok should be true for consecutive");

        let arm_values = ir.match_jump_table_arm_values().get(match_id);
        assert!(arm_values.is_some(), "arm_values should be populated");
        assert_eq!(
            arm_values.unwrap().len(),
            4,
            "arm_values should have 4 entries"
        );
    }

    #[test]
    fn sparse_variants_no_inject() {
        let (mut ir, match_id) = create_test_match(
            4,
            vec![Some(0), Some(2), Some(4), Some(6)],
            false,
            false,
        );

        populate_auto_jump_table_meta(&mut ir);

        assert!(
            !ir.match_dispatch_meta().contains_key(match_id),
            "Meta should NOT be injected for sparse (non-consecutive) variants"
        );
    }

    #[test]
    fn payload_binder_no_inject() {
        let (mut ir, match_id) = create_test_match(
            4,
            vec![Some(0), Some(1), Some(2), Some(3)],
            true,
            false,
        );

        populate_auto_jump_table_meta(&mut ir);

        assert!(
            !ir.match_dispatch_meta().contains_key(match_id),
            "Meta should NOT be injected when arm has payload_binder"
        );
    }

    #[test]
    fn pattern_binding_no_inject() {
        let (mut ir, match_id) = create_test_match(
            4,
            vec![Some(0), Some(1), Some(2), Some(3)],
            false,
            true,
        );

        populate_auto_jump_table_meta(&mut ir);

        assert!(
            !ir.match_dispatch_meta().contains_key(match_id),
            "Meta should NOT be injected when arm has pattern_binding"
        );
    }

    #[test]
    fn existing_jump_table_meta_not_overwritten() {
        let (mut ir, match_id) = create_test_match(
            4,
            vec![Some(0), Some(1), Some(2), Some(3)],
            false,
            false,
        );

        // Pre-populate with user @jump_table metadata.
        let existing_meta = MatchDispatchMeta {
            jump_table: true,
            min_arm: 0,
            range: 4,
            covered_arms: 4,
            density_ok: false, // Mark as not dense.
        };
        ir.match_dispatch_meta_mut()
            .insert(match_id, existing_meta);

        populate_auto_jump_table_meta(&mut ir);

        let meta = ir.match_dispatch_meta().get(match_id).unwrap();
        assert!(
            !meta.density_ok,
            "Pre-existing metadata should NOT be overwritten"
        );
    }

    #[test]
    fn less_than_4_arms_no_inject() {
        let (mut ir, match_id) = create_test_match(
            3,
            vec![Some(0), Some(1), Some(2)],
            false,
            false,
        );

        populate_auto_jump_table_meta(&mut ir);

        assert!(
            !ir.match_dispatch_meta().contains_key(match_id),
            "Meta should NOT be injected for < 4 arms"
        );
    }

    #[test]
    fn default_arm_ignored() {
        // 4 total arms: 3 variant arms + 1 default.
        let mut ir = IrArena::new();
        let file_id = FileId::new(1).unwrap();
        let span = Span::new(file_id, 0, 1);

        let scrutinee_id = ir.alloc(IrKind::Var, span);

        // Allocate 3 variant arms + 1 default.
        let arm_ids: Vec<_> = (0..4).map(|_| ir.alloc(IrKind::Action, span)).collect();

        for i in 0..3 {
            let mut meta = MatchArmMeta::default();
            meta.variant_index = Some(i as u32);
            ir.match_arm_meta_mut().insert(arm_ids[i], meta);
        }

        // Last arm is default.
        let mut default_meta = MatchArmMeta::default();
        default_meta.is_default = true;
        ir.match_arm_meta_mut()
            .insert(arm_ids[3], default_meta);

        let match_id = ir.alloc_with_children(
            IrKind::Match,
            span,
            vec![scrutinee_id]
                .into_iter()
                .chain(arm_ids.iter().copied())
                .collect::<Vec<_>>(),
        );

        ir.match_scrutinee_table_mut()
            .insert(match_id, EnumTypeId(0));

        populate_auto_jump_table_meta(&mut ir);

        // Only 3 non-default arms, so no injection.
        assert!(
            !ir.match_dispatch_meta().contains_key(match_id),
            "Meta should NOT be injected for < 4 non-default arms"
        );
    }
}
