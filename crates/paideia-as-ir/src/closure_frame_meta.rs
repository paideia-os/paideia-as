//! Closure frame layout metadata side-table — Issue #1233.
//!
//! Tracks per-function stack frame allocations for closure literals:
//! fat-pointer slots (16 B each) and environment-record slots (variable size).
//! Used by the elaborator's emit pass to emit prologue/epilogue and closure construction.

use crate::node::IrNodeId;
use std::collections::HashMap;

/// Stack frame layout for a function containing closures.
///
/// Computes the total frame size (prologue sub rsp; epilogue add rsp) and
/// assigns non-overlapping storage for each ClosureCons node's fat pair + env record.
#[derive(Debug, Clone)]
pub struct FrameLayout {
    /// Total stack frame size in bytes, 16-aligned (for SysV call alignment).
    /// Includes space for all fat pairs (16 B each) and all env records (per closure).
    pub total_size: u64,
    /// Maps ClosureCons node ID → (fat_offset, env_offset) for stack slot assignment.
    /// Offsets are computed from rsp (not rbp; paideia-as uses rsp-relative addressing).
    pub cons_slots: HashMap<IrNodeId, (u64, u64)>,
}

impl FrameLayout {
    /// Construct a new frame layout with no closures (total_size = 0).
    #[must_use]
    pub fn new() -> Self {
        Self {
            total_size: 0,
            cons_slots: HashMap::new(),
        }
    }

    /// Assign a fat pair + env record slot for a ClosureCons node.
    /// Records (fat_offset, env_offset) in the layout.
    pub fn assign_slot(&mut self, cc_id: IrNodeId, fat_offset: u64, env_offset: u64) {
        self.cons_slots.insert(cc_id, (fat_offset, env_offset));
    }

    /// Retrieve the (fat_offset, env_offset) for a ClosureCons node.
    #[must_use]
    pub fn get_slot(&self, cc_id: IrNodeId) -> Option<(u64, u64)> {
        self.cons_slots.get(&cc_id).copied()
    }

    /// Return the fat pair offset for a ClosureCons node (fat_ptr base address).
    #[must_use]
    pub fn fat_offset(&self, cc_id: IrNodeId) -> Option<u64> {
        self.get_slot(cc_id).map(|(off, _)| off)
    }

    /// Return the environment record offset for a ClosureCons node (captures base address).
    #[must_use]
    pub fn env_offset(&self, cc_id: IrNodeId) -> Option<u64> {
        self.get_slot(cc_id).map(|(_, off)| off)
    }
}

impl Default for FrameLayout {
    fn default() -> Self {
        Self::new()
    }
}

/// Side-table mapping function (caller) Lambda IDs to their frame layouts.
/// Sparse: only entries for Lambdas that contain ClosureCons nodes.
#[derive(Debug, Default)]
pub struct ClosureFrameMetaTable {
    entries: HashMap<IrNodeId, FrameLayout>,
}

impl ClosureFrameMetaTable {
    /// Construct an empty frame layout table.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Insert a caller function's frame layout.
    pub fn insert(&mut self, caller_lambda_id: IrNodeId, layout: FrameLayout) {
        self.entries.insert(caller_lambda_id, layout);
    }

    /// Retrieve a caller function's frame layout by its Lambda ID.
    #[must_use]
    pub fn get(&self, caller_lambda_id: IrNodeId) -> Option<&FrameLayout> {
        self.entries.get(&caller_lambda_id)
    }

    /// Mutable access to a caller function's frame layout for modification.
    pub fn get_mut(&mut self, caller_lambda_id: IrNodeId) -> Option<&mut FrameLayout> {
        self.entries.get_mut(&caller_lambda_id)
    }

    /// Iterate over all caller functions and their frame layouts.
    pub fn iter(&self) -> impl Iterator<Item = (IrNodeId, &FrameLayout)> {
        self.entries.iter().map(|(&id, layout)| (id, layout))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_layout_new_has_zero_size() {
        let layout = FrameLayout::new();
        assert_eq!(layout.total_size, 0);
        assert!(layout.cons_slots.is_empty());
    }

    #[test]
    fn frame_layout_assign_and_retrieve_slot() {
        let mut layout = FrameLayout::new();
        layout.total_size = 32;

        let cc_id = IrNodeId::new(10).unwrap();
        layout.assign_slot(cc_id, 0, 16);

        let slot = layout.get_slot(cc_id);
        assert_eq!(slot, Some((0, 16)));
        assert_eq!(layout.fat_offset(cc_id), Some(0));
        assert_eq!(layout.env_offset(cc_id), Some(16));
    }

    #[test]
    fn frame_layout_multiple_slots() {
        let mut layout = FrameLayout::new();
        layout.total_size = 64;

        let cc1 = IrNodeId::new(1).unwrap();
        let cc2 = IrNodeId::new(2).unwrap();

        layout.assign_slot(cc1, 0, 16);
        layout.assign_slot(cc2, 32, 48);

        assert_eq!(layout.get_slot(cc1), Some((0, 16)));
        assert_eq!(layout.get_slot(cc2), Some((32, 48)));
    }

    #[test]
    fn closure_frame_meta_table_insert_and_get() {
        let mut table = ClosureFrameMetaTable::new();
        let lambda_id = IrNodeId::new(100).unwrap();

        let mut layout = FrameLayout::new();
        layout.total_size = 32;
        layout.assign_slot(IrNodeId::new(10).unwrap(), 0, 16);

        table.insert(lambda_id, layout);

        let retrieved = table.get(lambda_id);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().total_size, 32);
    }

    #[test]
    fn closure_frame_meta_table_missing_entry_returns_none() {
        let table = ClosureFrameMetaTable::new();
        let id = IrNodeId::new(999).unwrap();
        assert!(table.get(id).is_none());
    }

    #[test]
    fn closure_frame_meta_table_iter() {
        let mut table = ClosureFrameMetaTable::new();

        let id1 = IrNodeId::new(1).unwrap();
        let id2 = IrNodeId::new(2).unwrap();

        let mut layout1 = FrameLayout::new();
        layout1.total_size = 16;
        table.insert(id1, layout1);

        let mut layout2 = FrameLayout::new();
        layout2.total_size = 32;
        table.insert(id2, layout2);

        let pairs: Vec<_> = table.iter().collect();
        assert_eq!(pairs.len(), 2);
    }
}
