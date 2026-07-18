//! Label map for runtime resolution: label name → instruction index.
//!
//! Map from label name to index in the caller's `instructions` slice.
//! The instruction at the mapped index is the label target — its byte
//! offset in the emitted stream (computed by the resolver's layout pass) is
//! the address to substitute for `LabelRef { name }`.

use alloc::collections::BTreeMap;
use alloc::string::String;

/// Returned from `LabelMap::insert` when a name was already registered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateLabel {
    /// The duplicate label name.
    pub name: String,
}

/// Map from label name → index in the caller's `instructions` slice.
///
/// The instruction at the mapped index is the label target — its byte
/// offset in the emitted stream (computed by the resolver's layout pass) is
/// the address to substitute for `LabelRef { name }`.
///
/// Constructed by the caller before `resolve_symbols` runs. Typical use:
/// walk the instruction slice once looking for label markers (or receive an
/// externally-populated map from a lowering pass).
#[derive(Default, Debug, Clone)]
pub struct LabelMap {
    entries: BTreeMap<String, usize>,
}

impl LabelMap {
    /// Create a new empty label map.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a label with its instruction index.
    ///
    /// Returns `Err(DuplicateLabel)` if the label is already registered.
    pub fn insert(&mut self, name: impl Into<String>, index: usize) -> Result<(), DuplicateLabel> {
        let name_str = name.into();
        if self.entries.contains_key(&name_str) {
            return Err(DuplicateLabel { name: name_str });
        }
        self.entries.insert(name_str, index);
        Ok(())
    }

    /// Look up a label by name.
    pub fn get(&self, name: &str) -> Option<usize> {
        self.entries.get(name).copied()
    }

    /// Check if a label is present in the map.
    pub fn contains(&self, name: &str) -> bool {
        self.entries.contains_key(name)
    }

    /// Return the number of labels in the map.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if the map is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
