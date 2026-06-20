//! Shared pass state for walkers to populate PositionIndex.
//!
//! Phase-4-m1-005: Each walker pass (linearity, effect-row, capability)
//! needs access to the PositionIndex and the current FileId so it can
//! insert elaborator results during traversal.
//!
//! This module uses Cell<> to provide interior mutability for the PositionIndex
//! without requiring unsafe code, working around the borrow checker while keeping
//! the forbid(unsafe_code) constraint.

use std::cell::RefCell;

use crate::position_index::{FileId, PositionEntry, PositionIndex};

/// Trait for position index insertion, allowing walkers to record elaborator results.
pub trait PositionIndexWriter {
    /// Get the current file ID.
    fn file_id(&self) -> FileId;

    /// Insert a position entry.
    fn insert_entry(&self, entry: PositionEntry);
}

/// Concrete implementation of PositionIndexWriter using RefCell for interior mutability.
pub struct WalkerPassState {
    file_id: FileId,
    position_index: RefCell<PositionIndex>,
}

impl WalkerPassState {
    /// Create a new pass state for the given file. The position index is
    /// created empty and populated during walker passes.
    pub fn new(file_id: FileId) -> Self {
        Self {
            file_id,
            position_index: RefCell::new(PositionIndex::new()),
        }
    }

    /// Consume this pass state and extract the finalized PositionIndex.
    pub fn into_position_index(self) -> PositionIndex {
        self.position_index.into_inner()
    }

    /// Get a reference to the current PositionIndex (for inspection).
    pub fn position_index(&self) -> std::cell::Ref<'_, PositionIndex> {
        self.position_index.borrow()
    }
}

impl PositionIndexWriter for WalkerPassState {
    fn file_id(&self) -> FileId {
        self.file_id
    }

    fn insert_entry(&self, entry: PositionEntry) {
        self.position_index.borrow_mut().insert(self.file_id, entry);
    }
}
