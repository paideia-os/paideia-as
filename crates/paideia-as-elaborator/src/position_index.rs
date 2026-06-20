//! Per-source-position elaborator result store.
//!
//! Maps (FileId, ByteOffset) → (TypeId, LinClass, EffectRowId, CapSetId)
//! so LSP queries (hover, definition, references, completion) can answer
//! "what's at this position" without re-running the elaborator.
//!
//! Populated as a side-effect of walker passes (linearity, effect-row,
//! capability) — each walker that already visits AST nodes inserts an
//! entry for the node's span into this index.
//!
//! Lookup is O(log n) via binary search over a sorted span vector.

use std::collections::HashMap;

use paideia_as_ir::LinClass;
use paideia_as_types::TypeId;

/// Unique identifier for a source file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FileId(pub u32);

/// Byte offset within a source file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ByteOffset(pub u32);

/// A single position index entry: stores elaborator results for a span.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PositionEntry {
    /// Start byte offset of the span.
    pub span_start: ByteOffset,
    /// End byte offset of the span (exclusive).
    pub span_end: ByteOffset,
    /// Inferred type, if available.
    pub type_id: Option<TypeId>,
    /// Substructural linearity class, if available.
    pub lin_class: Option<LinClass>,
    /// Effect row reference, if available.
    /// (Placeholder u32 pending EffectRowId definition.)
    pub effect_row_id: Option<u32>,
    /// Capability set reference, if available.
    /// (Placeholder u32 pending CapSetId formalization.)
    pub cap_set_id: Option<u32>,
}

/// Per-source-position elaborator result store.
///
/// Maintains a sorted index of spans → elaborator results for each file.
/// Supports O(log n) lookup of the innermost containing span at a position.
#[derive(Clone, Debug)]
pub struct PositionIndex {
    files: HashMap<FileId, Vec<PositionEntry>>,
}

impl PositionIndex {
    /// Create a new empty position index.
    #[must_use]
    pub fn new() -> Self {
        Self {
            files: HashMap::new(),
        }
    }

    /// Insert an entry during a walker pass.
    ///
    /// Entries are accumulated in traversal order; call [`finish`] once
    /// all passes complete to sort for efficient lookup.
    ///
    /// [`finish`]: Self::finish
    pub fn insert(&mut self, file: FileId, entry: PositionEntry) {
        self.files.entry(file).or_default().push(entry);
    }

    /// Finalize the index by sorting all entries.
    ///
    /// Call this once after all walker passes complete — subsequent
    /// calls to [`at`] will perform binary search on sorted entries.
    ///
    /// [`at`]: Self::at
    pub fn finish(&mut self) {
        for entries in self.files.values_mut() {
            entries.sort_by_key(|e| e.span_start);
        }
    }

    /// Look up the innermost containing span at a position.
    ///
    /// Returns the entry with the smallest span that contains the given
    /// position, or `None` if no span covers the position.
    ///
    /// Time complexity: O(log n) binary search + O(n) linear scan in worst
    /// case (when many overlapping spans exist), but typically O(log n) in
    /// practice due to limited nesting depth.
    pub fn at(&self, file: FileId, pos: ByteOffset) -> Option<&PositionEntry> {
        let entries = self.files.get(&file)?;

        // Binary search for entries with span_start <= pos < span_end.
        // We search for the rightmost entry with span_start <= pos.
        match entries.binary_search_by_key(&pos, |e| e.span_start) {
            Ok(idx) => {
                // Exact match: found an entry starting at pos.
                // Check if pos is within this span.
                if entries[idx].span_end > pos {
                    return Some(&entries[idx]);
                }
                // pos is at the end boundary; fall through to linear scan
            }
            Err(idx) => {
                // idx is the insertion point; actual containing entries
                // are at idx-1, idx-2, etc. (we scan backward for the
                // smallest containing span).
                if idx > 0 {
                    // Scan backward from idx-1 to find the first (innermost)
                    // entry that contains pos.
                    for i in (0..idx).rev() {
                        if entries[i].span_start <= pos && entries[i].span_end > pos {
                            return Some(&entries[i]);
                        }
                    }
                }
            }
        }

        None
    }

    /// Total number of entries across all files.
    #[must_use]
    pub fn entry_count(&self) -> usize {
        self.files.values().map(|v| v.len()).sum()
    }

    /// Get all entries for a specific file, if any.
    #[must_use]
    pub fn entries_for_file(&self, file: FileId) -> Option<&[PositionEntry]> {
        self.files.get(&file).map(|v| v.as_slice())
    }
}

impl Default for PositionIndex {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn position_index_starts_empty() {
        let index = PositionIndex::new();
        assert_eq!(index.entry_count(), 0);
        assert_eq!(index.entries_for_file(FileId(1)), None);
    }

    #[test]
    fn position_index_insert_and_lookup() {
        let file = FileId(1);
        let mut index = PositionIndex::new();

        let entry = PositionEntry {
            span_start: ByteOffset(0),
            span_end: ByteOffset(10),
            type_id: None,
            lin_class: Some(LinClass::Unrestricted),
            effect_row_id: None,
            cap_set_id: None,
        };

        index.insert(file, entry);
        index.finish();

        // Lookup within the span should succeed.
        let result = index.at(file, ByteOffset(5));
        assert!(result.is_some());
        assert_eq!(result.unwrap().span_start, ByteOffset(0));
        assert_eq!(result.unwrap().span_end, ByteOffset(10));
    }

    #[test]
    fn position_index_lookup_returns_none_for_uncovered_position() {
        let file = FileId(1);
        let mut index = PositionIndex::new();

        let entry = PositionEntry {
            span_start: ByteOffset(0),
            span_end: ByteOffset(10),
            type_id: None,
            lin_class: Some(LinClass::Linear),
            effect_row_id: None,
            cap_set_id: None,
        };

        index.insert(file, entry);
        index.finish();

        // Lookup outside the span should fail.
        let result = index.at(file, ByteOffset(20));
        assert!(result.is_none());

        // Lookup before the span should fail.
        let result = index.at(file, ByteOffset(0));
        assert!(result.is_some()); // ByteOffset(0) is included in [0, 10)
    }

    #[test]
    fn position_index_lookup_finds_innermost_containing_span() {
        let file = FileId(1);
        let mut index = PositionIndex::new();

        // Outer span [0, 20)
        index.insert(
            file,
            PositionEntry {
                span_start: ByteOffset(0),
                span_end: ByteOffset(20),
                type_id: None,
                lin_class: Some(LinClass::Unrestricted),
                effect_row_id: None,
                cap_set_id: None,
            },
        );

        // Inner span [5, 15)
        index.insert(
            file,
            PositionEntry {
                span_start: ByteOffset(5),
                span_end: ByteOffset(15),
                type_id: None,
                lin_class: Some(LinClass::Linear),
                effect_row_id: None,
                cap_set_id: None,
            },
        );

        index.finish();

        // Lookup at position 10 should return the inner span [5, 15).
        let result = index.at(file, ByteOffset(10));
        assert!(result.is_some());
        let entry = result.unwrap();
        assert_eq!(entry.span_start, ByteOffset(5));
        assert_eq!(entry.span_end, ByteOffset(15));
        assert_eq!(entry.lin_class, Some(LinClass::Linear));
    }

    #[test]
    fn position_index_handles_multiple_files() {
        let file1 = FileId(1);
        let file2 = FileId(2);
        let mut index = PositionIndex::new();

        let entry1 = PositionEntry {
            span_start: ByteOffset(0),
            span_end: ByteOffset(10),
            type_id: None,
            lin_class: Some(LinClass::Unrestricted),
            effect_row_id: None,
            cap_set_id: None,
        };

        let entry2 = PositionEntry {
            span_start: ByteOffset(0),
            span_end: ByteOffset(5),
            type_id: None,
            lin_class: Some(LinClass::Affine),
            effect_row_id: None,
            cap_set_id: None,
        };

        index.insert(file1, entry1);
        index.insert(file2, entry2);
        index.finish();

        assert_eq!(index.entry_count(), 2);

        // Lookup in file1 should return entry1.
        let result = index.at(file1, ByteOffset(5));
        assert!(result.is_some());
        assert_eq!(result.unwrap().lin_class, Some(LinClass::Unrestricted));

        // Lookup in file2 should return entry2.
        let result = index.at(file2, ByteOffset(2));
        assert!(result.is_some());
        assert_eq!(result.unwrap().lin_class, Some(LinClass::Affine));
    }

    #[test]
    fn position_index_finish_sorts_entries_by_span_start() {
        let file = FileId(1);
        let mut index = PositionIndex::new();

        // Insert in reverse order.
        index.insert(
            file,
            PositionEntry {
                span_start: ByteOffset(20),
                span_end: ByteOffset(30),
                type_id: None,
                lin_class: Some(LinClass::Ordered),
                effect_row_id: None,
                cap_set_id: None,
            },
        );

        index.insert(
            file,
            PositionEntry {
                span_start: ByteOffset(10),
                span_end: ByteOffset(15),
                type_id: None,
                lin_class: Some(LinClass::Linear),
                effect_row_id: None,
                cap_set_id: None,
            },
        );

        index.insert(
            file,
            PositionEntry {
                span_start: ByteOffset(0),
                span_end: ByteOffset(5),
                type_id: None,
                lin_class: Some(LinClass::Unrestricted),
                effect_row_id: None,
                cap_set_id: None,
            },
        );

        index.finish();

        let entries = index.entries_for_file(file).unwrap();
        assert_eq!(entries[0].span_start, ByteOffset(0));
        assert_eq!(entries[1].span_start, ByteOffset(10));
        assert_eq!(entries[2].span_start, ByteOffset(20));
    }
}
